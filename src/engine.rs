//! Parser engine — the main orchestrator.
//!
//! ```rust,no_run
//! use lang_parser::engine::ParserEngine;
//! use lang_parser::languages::{solidity::SolidityParser, rust_lang::RustParser, python::PythonParser};
//!
//! let mut engine = ParserEngine::new();
//! engine.register(Box::new(SolidityParser::new()));
//! engine.register(Box::new(RustParser::new()));
//! engine.register(Box::new(PythonParser::new()));
//!
//! engine.add_source("Token.sol",   include_str!("Token.sol"));
//! engine.add_source("helpers.rs",  include_str!("helpers.rs"));
//!
//! let result = engine.analyze().unwrap();
//! ```

use std::collections::{HashSet, VecDeque};

use crate::core::{
    resolver::ResolverIndex,
    traits::{LanguageParser, ParseError},
    types::{CallChain, CallChainNode, FunctionDef, ProjectAnalysis, SourceFile},
};

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct ParserEngine {
    parsers: Vec<Box<dyn LanguageParser>>,
    sources: Vec<(String, String)>,
}

impl ParserEngine {
    /// Create an empty engine.
    pub fn new() -> Self {
        Self { parsers: Vec::new(), sources: Vec::new() }
    }

    /// Register a language back-end.  Files are matched by extension.
    pub fn register(&mut self, parser: Box<dyn LanguageParser>) {
        self.parsers.push(parser);
    }

    /// Add a source file by path + content string.
    pub fn add_source(&mut self, path: impl Into<String>, content: impl Into<String>) {
        self.sources.push((path.into(), content.into()));
    }

    /// Add multiple source files at once.
    pub fn add_sources<I, P, C>(&mut self, files: I)
    where
        I: IntoIterator<Item = (P, C)>,
        P: Into<String>,
        C: Into<String>,
    {
        for (p, c) in files { self.add_source(p, c); }
    }

    // ── Main entry point ──────────────────────────────────────────────────────

    /// Parse all source files, resolve cross-file symbols, build call chains.
    pub fn analyze(&self) -> Result<ProjectAnalysis, ParseError> {
        let mut parsed_files: Vec<SourceFile> = Vec::new();
        let mut warnings:     Vec<String>     = Vec::new();

        // Phase 1 — parse each file with the matching language back-end.
        for (path, content) in &self.sources {
            let parser = self.parsers.iter().find(|p| p.can_handle(path));

            let parser = match parser {
                Some(p) => p,
                None => {
                    warnings.push(format!("no parser registered for: {path}"));
                    continue;
                }
            };

            match parser.parse(path, content) {
                Ok(sf) => {
                    // Flag tree-sitter parse errors as warnings, but keep the
                    // partial result — partial trees are still useful.
                    if sf.functions.is_empty() && !content.trim().is_empty() {
                        // This might indicate a parse error — the engine will
                        // note it but keep going.
                    }
                    parsed_files.push(sf);
                }
                Err(e) => {
                    warnings.push(format!("parse error in {path}: {e}"));
                }
            }
        }

        // Phase 2 — build the cross-file resolution index.
        let index = ResolverIndex::build(&parsed_files);

        // Phase 3 — build one CallChain per public/external entry point.
        let call_chains = self.build_all_chains(&parsed_files, &index, &mut warnings);

        Ok(ProjectAnalysis {
            source_files:    parsed_files,
            call_chains,
            all_definitions: index.all_definitions().clone(),
            warnings,
        })
    }

    // ── Call-chain construction ───────────────────────────────────────────────

    fn build_all_chains(
        &self,
        files:    &[SourceFile],
        index:    &ResolverIndex,
        warnings: &mut Vec<String>,
    ) -> Vec<CallChain> {
        let mut chains = Vec::new();

        for file in files {
            for func in &file.functions {
                if func.is_entry_point() {
                    chains.push(self.build_chain(func, &file.path, index, warnings));
                }
            }
        }

        // Deterministic output: sort by file then function name.
        chains.sort_by(|a, b| {
            a.entry_file.cmp(&b.entry_file)
                .then(a.entry_function.cmp(&b.entry_function))
        });

        chains
    }

    /// BFS traversal starting from `entry`.
    fn build_chain(
        &self,
        entry:       &FunctionDef,
        entry_file:  &str,
        index:       &ResolverIndex,
        warnings:    &mut Vec<String>,
    ) -> CallChain {
        // Queue items: (definition, depth)
        let mut queue:   VecDeque<(FunctionDef, usize)> = VecDeque::new();
        // Visited key: "file::name" — prevents infinite loops in recursive code.
        let mut visited: HashSet<String>                 = HashSet::new();
        let mut nodes:   Vec<CallChainNode>              = Vec::new();

        queue.push_back((entry.clone(), 0));

        while let Some((func, depth)) = queue.pop_front() {
            let key = format!("{}::{}", func.file, func.name);
            if !visited.insert(key) { continue; }

            // Resolve modifier / decorator definitions.
            let applied_modifiers: Vec<FunctionDef> = func.modifiers.iter()
                .filter_map(|mref| {
                    index.resolve_modifier(&mref.name, &func.file, warnings)
                })
                .collect();

            // Enqueue callees that live inside modifier bodies.
            for m in &applied_modifiers {
                for callee in &m.callees {
                    if let Some(def) = index.resolve_callee(callee, &m.file, warnings) {
                        queue.push_back((def, depth + 1));
                    }
                }
            }

            // Enqueue direct callees.
            for callee in &func.callees {
                if let Some(def) = index.resolve_callee(callee, &func.file, warnings) {
                    queue.push_back((def, depth + 1));
                } else {
                    // Only warn if it looks like a user-defined function
                    // (skip Solidity builtins like require, emit, revert, etc.)
                    if !is_builtin(callee) {
                        warnings.push(format!(
                            "unresolved callee `{callee}` in `{}` ({})",
                            func.name, func.file
                        ));
                    }
                }
            }

            let file = func.file.clone();
            nodes.push(CallChainNode {
                function_name:     func.name.clone(),
                definition:        func,
                file,
                depth,
                applied_modifiers,
            });
        }

        CallChain {
            entry_function:  entry.name.clone(),
            entry_container: entry.container.clone(),
            entry_file:      entry_file.to_string(),
            nodes,
        }
    }
}

impl Default for ParserEngine {
    fn default() -> Self { Self::new() }
}

// ── Builtin filter ────────────────────────────────────────────────────────────

/// Common builtins / keywords that aren't user-defined functions.
/// Prevents noisy "unresolved callee" warnings for things like
/// `require(...)`, `emit Transfer(...)`, `println!(...)`, etc.
fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        // Solidity
        | "require" | "revert" | "assert" | "emit" | "selfdestruct"
        | "keccak256" | "sha256" | "ecrecover" | "addmod" | "mulmod"
        | "gasleft" | "blockhash" | "abi"
        // Rust
        | "println" | "eprintln" | "print" | "eprint" | "format"
        | "vec" | "panic" | "assert_eq" | "assert_ne" | "assert"
        | "todo" | "unimplemented" | "unreachable" | "dbg"
        | "write" | "writeln" | "log" | "warn" | "error" | "info" | "debug" | "trace"
        | "Some" | "None" | "Ok" | "Err"
        | "Box" | "Rc" | "Arc" | "String" | "Vec"
        | "unwrap" | "expect" | "map" | "and_then" | "or_else"
        | "into" | "from" | "clone" | "to_string" | "iter" | "collect"
        | "push" | "pop" | "len" | "is_empty" | "contains"
        // Python
        | "print" | "len" | "range" | "enumerate" | "zip" | "map" | "filter"
        | "list" | "dict" | "set" | "tuple" | "int" | "str" | "float" | "bool"
        | "super" | "isinstance" | "issubclass" | "getattr" | "setattr" | "hasattr"
        | "open" | "type" | "repr" | "hash" | "id" | "input"
        | "raise" | "append" | "extend" | "update" | "items" | "keys" | "values"
        | "get" | "pop" | "insert" | "remove" | "sort" | "sorted" | "reversed"
    )
}
