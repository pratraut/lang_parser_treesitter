//! # lang_parser
//!
//! A **tree-sitter powered**, extensible multi-language call-chain analyser.
//!
//! ## Supported languages
//!
//! | Language | Extension | Grammar crate |
//! |---|---|---|
//! | Solidity | `.sol` | `tree-sitter-solidity` |
//! | Rust     | `.rs`  | `tree-sitter-rust`     |
//! | Python   | `.py`  | `tree-sitter-python`   |
//!
//! ## Adding a new language
//!
//! 1. Add `tree-sitter-<lang>` to `Cargo.toml`
//! 2. Create `src/languages/<lang>/mod.rs`
//! 3. Implement [`core::traits::TreeSitterParser`] — the blanket impl
//!    automatically provides [`core::traits::LanguageParser`]
//! 4. `engine.register(Box::new(MyParser::new()))`
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use lang_parser::prelude::*;
//!
//! let mut engine = ParserEngine::new();
//! engine.register(Box::new(SolidityParser::new()));
//! engine.register(Box::new(RustParser::new()));
//! engine.register(Box::new(PythonParser::new()));
//!
//! engine.add_source("Token.sol", include_str!("../examples/Token.sol"));
//!
//! let project = engine.analyze().unwrap();
//!
//! for chain in &project.call_chains {
//!     println!("Entry: {} ({}:{})",
//!         chain.entry_function, chain.entry_file,
//!         chain.nodes[0].definition.span.start_line);
//!     for node in &chain.nodes {
//!         for m in &node.applied_modifiers {
//!             println!("  modifier `{}` @ {}:{}", m.name, m.file, m.span.start_line);
//!         }
//!     }
//! }
//! ```

pub mod core;
pub mod engine;
pub mod formatter;
pub mod languages;

#[cfg(test)]
mod tests;

/// Convenience re-exports — `use lang_parser::prelude::*` gives you everything
/// needed for typical usage.
pub mod prelude {
    pub use crate::core::types::{
        CallChain, CallChainNode, FunctionDef, Param, ModifierRef,
        ProjectAnalysis, SourceFile, Span, Visibility,
    };
    pub use crate::core::traits::{LanguageParser, TreeSitterParser, ParseError};
    pub use crate::engine::ParserEngine;
    pub use crate::languages::solidity::SolidityParser;
    pub use crate::languages::rust_lang::RustParser;
    pub use crate::languages::python::PythonParser;
}

// Flat re-exports for the most common types
pub use crate::core::types::{
    CallChain, CallChainNode, FunctionDef, ProjectAnalysis, SourceFile, Span, Visibility,
};
pub use crate::core::traits::{LanguageParser, TreeSitterParser};
pub use crate::engine::ParserEngine;
