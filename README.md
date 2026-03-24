# lang_parser

A **tree-sitter powered**, extensible multi-language call-chain analyser written in Rust.

Parses source files using [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammars to produce a complete call graph with:
- Every **public / external entry point** and its full call chain
- **Exact source positions** (line, column, byte offset) for every definition
- **Modifier / decorator definitions** attached to each call-chain node
- **Cross-file symbol resolution** via the import graph
- **Recursion detection** (no infinite loops)

## Supported languages

| Language | Extension | Grammar crate |
|---|---|---|
| Solidity | `.sol` | [`tree-sitter-solidity`](https://crates.io/crates/tree-sitter-solidity) |
| Rust     | `.rs`  | [`tree-sitter-rust`](https://crates.io/crates/tree-sitter-rust)         |
| Python   | `.py`  | [`tree-sitter-python`](https://crates.io/crates/tree-sitter-python)     |

Any language with a tree-sitter grammar can be added in ~50 lines.

## Quick start

```toml
[dependencies]
lang_parser = "0.1"
```

```rust
use lang_parser::prelude::*;

let mut engine = ParserEngine::new();
engine.register(Box::new(SolidityParser::new()));
engine.register(Box::new(RustParser::new()));
engine.register(Box::new(PythonParser::new()));

engine.add_source("Token.sol",   include_str!("Token.sol"));
engine.add_source("helpers.rs",  include_str!("helpers.rs"));
engine.add_source("utils.py",    include_str!("utils.py"));

let project = engine.analyze().unwrap();

for chain in &project.call_chains {
    println!("Entry: {}  ({}:{})",
        chain.entry_function,
        chain.entry_file,
        chain.nodes[0].definition.span.start_line);

    for node in &chain.nodes {
        println!("  depth={} → {} @ {}:{}",
            node.depth, node.function_name,
            node.file, node.definition.span.start_line);

        for m in &node.applied_modifiers {
            println!("    modifier/decorator `{}` defined at {}:{}",
                m.name, m.file, m.span.start_line);
        }
    }
}
```

Run the bundled demo:

```bash
cargo run --example analyze
cargo test
```

## Architecture

```
lang_parser/
├── src/
│   ├── lib.rs                       ← crate root + prelude
│   ├── engine.rs                    ← ParserEngine orchestrator
│   ├── formatter.rs                 ← human-readable output
│   ├── core/
│   │   ├── types.rs                 ← FunctionDef, CallChain, Span, Visibility…
│   │   ├── traits.rs                ← LanguageParser + TreeSitterParser traits
│   │   ├── query.rs                 ← shared tree-sitter utilities + callee extractor
│   │   └── resolver.rs              ← cross-file symbol resolution (import BFS)
│   └── languages/
│       ├── solidity/mod.rs          ← Solidity back-end
│       ├── rust_lang/mod.rs         ← Rust back-end
│       └── python/mod.rs            ← Python back-end
├── examples/
│   ├── analyze.rs                   ← end-to-end demo (Solidity + Rust + Python)
│   ├── Token.sol / Ownable.sol      ← example Solidity contracts
│   ├── vault.rs                     ← example Rust vault
│   └── ledger.py                    ← example Python ledger
└── build.rs                         ← grammar compilation hook
```

### Data flow

```
Source files (any mix of .sol / .rs / .py)
        │
        ▼  (tree-sitter grammar parses each file into a CST)
[Language back-end]
    tree_sitter::Parser::parse() → Tree (concrete syntax tree)
    extract_definitions() walks the CST:
        • function_definition / function_item / def → FunctionDef
        • modifier_definition / decorator           → ModifierRef
        • import_directive / use_declaration        → import paths
        • call_expression nodes in bodies           → FunctionDef::callees
        │
        ▼
[ResolverIndex::build]
    exact:   "file::name" → FunctionDef
    by_name: "name"       → [(file, FunctionDef)]
    imports: file         → {imported files}   (for BFS)
        │
        ▼
[ParserEngine::build_all_chains]
    For each public/external FunctionDef:
        BFS queue: (FunctionDef, depth)
        Visited set: "file::name" (cycle prevention)
        Each node:
            → resolve modifier/decorator defs
            → enqueue callees (resolved via ResolverIndex)
        │
        ▼
ProjectAnalysis { source_files, call_chains, all_definitions, warnings }
```

## Adding a new language

1. Add the grammar crate to `Cargo.toml`:
   ```toml
   tree-sitter-go = "0.21"
   ```

2. Create `src/languages/go/mod.rs` implementing `TreeSitterParser`:
   ```rust
   use tree_sitter::{Tree};
   use crate::core::traits::{TreeSitterParser, ParseError};
   use crate::core::types::SourceFile;

   pub struct GoParser;

   impl TreeSitterParser for GoParser {
       fn language_name(&self) -> &str { "Go" }
       fn extensions(&self)    -> &[&str] { &["go"] }

       fn tree_sitter_language(&self) -> tree_sitter::Language {
           tree_sitter_go::language()
       }

       fn extract_definitions(
           &self, path: &str, source: &str, tree: &Tree,
       ) -> Result<SourceFile, ParseError> {
           // Walk the CST and populate FunctionDef entries.
           // Use self.node_span(), self.node_source(), extract_callees() helpers.
           todo!()
       }
   }
   ```

3. Add to `src/languages/mod.rs`:
   ```rust
   pub mod go;
   ```

4. Register in your application:
   ```rust
   engine.register(Box::new(GoParser::new()));
   ```

That's it. The engine handles cross-file resolution and chain building automatically.

## Key types

### `FunctionDef`
```rust
pub struct FunctionDef {
    pub name:        String,
    pub visibility:  Visibility,        // Public | External | Internal | Private
    pub params:      Vec<Param>,
    pub returns:     Vec<Param>,
    pub modifiers:   Vec<ModifierRef>,  // modifiers/decorators applied
    pub file:        String,
    pub span:        Span,              // start/end line, col, byte offset
    pub source_text: String,            // raw source of entire definition
    pub is_modifier: bool,
    pub container:   Option<String>,    // contract / class / impl name
    pub callees:     Vec<String>,       // called function names (unresolved)
    pub mutability:  Option<String>,    // Solidity: view/pure/payable
}
```

### `CallChainNode`
```rust
pub struct CallChainNode {
    pub function_name:     String,
    pub definition:        FunctionDef,      // full resolved definition
    pub file:              String,
    pub depth:             usize,            // 0 = entry point
    pub applied_modifiers: Vec<FunctionDef>, // resolved modifier defs
}
```

### `Span`
```rust
pub struct Span {
    pub start_byte: usize,
    pub end_byte:   usize,
    pub start_line: usize,  // 1-based
    pub end_line:   usize,  // 1-based
    pub start_col:  usize,  // 0-based
    pub end_col:    usize,  // 0-based
}
```

## Why tree-sitter?

| Feature | Hand-written lexer | tree-sitter |
|---|---|---|
| Languages supported | One (Solidity) | 100+ out of the box |
| Grammar maintenance | Manual | Community-maintained grammars |
| Error recovery | Basic | Built-in — always produces a tree |
| Incremental re-parse | No | Yes (for IDE integration) |
| CST fidelity | Partial | Full concrete syntax tree |
| Build complexity | None (pure Rust) | Thin C shim per grammar |
