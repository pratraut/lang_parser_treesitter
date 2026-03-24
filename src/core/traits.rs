//! Traits for language back-ends.
//!
//! # Two levels of abstraction
//!
//! 1. **[`LanguageParser`]** — the minimal contract every back-end must fulfil.
//!    Back-ends that don't use tree-sitter can implement this directly.
//!
//! 2. **[`TreeSitterParser`]** — a richer trait specifically for tree-sitter
//!    back-ends.  Blanket impl of `LanguageParser` is provided automatically
//!    for any type that implements `TreeSitterParser`.
//!
//! Most language back-ends only need to implement `TreeSitterParser`.

use tree_sitter::{Language, Node, Parser, Tree};

use crate::core::types::SourceFile;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ParseError {
    SyntaxError  { file: String, line: usize, message: String },
    InternalError(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::SyntaxError { file, line, message } =>
                write!(f, "syntax error in {file} at line {line}: {message}"),
            ParseError::InternalError(m) =>
                write!(f, "internal error: {m}"),
        }
    }
}
impl std::error::Error for ParseError {}

// ── LanguageParser ────────────────────────────────────────────────────────────

/// Minimal contract every language back-end must satisfy.
pub trait LanguageParser: Send + Sync {
    fn language_name(&self) -> &str;
    fn extensions(&self)    -> &[&str];
    fn parse(&self, path: &str, source: &str) -> Result<SourceFile, ParseError>;

    fn can_handle(&self, path: &str) -> bool {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        self.extensions().contains(&ext)
    }
}

// ── TreeSitterParser ──────────────────────────────────────────────────────────

/// Richer trait for tree-sitter back-ends.
///
/// Implementors only need to provide:
/// - [`tree_sitter_language`] — the grammar `Language` object
/// - [`extensions`] / [`language_name`]
/// - [`extract_definitions`] — walk the CST and extract [`FunctionDef`]s
///
/// The blanket impl below handles parser construction, parsing, and
/// `LanguageParser` forwarding automatically.
pub trait TreeSitterParser: Send + Sync {
    fn language_name(&self) -> &str;
    fn extensions(&self)    -> &[&str];

    /// Return the tree-sitter `Language` for this grammar.
    fn tree_sitter_language(&self) -> Language;

    /// Walk `tree` (rooted in `source`) and extract all definitions.
    ///
    /// - `path`   — source file path (for populating `FunctionDef::file`)
    /// - `source` — raw UTF-8 source text
    /// - `tree`   — fully-parsed tree-sitter CST
    ///
    /// Must populate `FunctionDef::callees` with the **names** of called
    /// functions.  Cross-file resolution happens in the engine.
    fn extract_definitions(
        &self,
        path:   &str,
        source: &str,
        tree:   &Tree,
    ) -> Result<SourceFile, ParseError>;

    /// Build a [`tree_sitter::Parser`] initialised with this language.
    fn build_ts_parser(&self) -> Parser {
        let mut p = Parser::new();
        p.set_language(&self.tree_sitter_language())
            .expect("tree-sitter language version mismatch");
        p
    }

    /// Helper: get the UTF-8 text of a node from the source bytes.
    fn node_text<'s>(&self, node: &Node, source: &'s [u8]) -> &'s str {
        node.utf8_text(source).unwrap_or("")
    }

    /// Helper: get text as an owned String.
    fn node_text_owned(&self, node: &Node, source: &[u8]) -> String {
        self.node_text(node, source).to_string()
    }

    /// Helper: collect all named children with the given field name.
    fn field_children<'a>(node: &'a Node<'a>, field: &str) -> Vec<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> =
            node.children_by_field_name(field, &mut cursor).collect();
        children
    }

    /// Helper: get the first named child with the given field name, if any.
    fn field_child<'a>(node: &'a Node<'a>, field: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let child = node.children_by_field_name(field, &mut cursor).next();
        child
    }

    /// Helper: get text of a single named field child.
    fn field_text<'a>(node: &'a Node<'a>, field: &str, source: &'a [u8]) -> Option<&'a str> {
        let mut cursor = node.walk();
        let child = node.children_by_field_name(field, &mut cursor).next();

        child.and_then(|n| n.utf8_text(source).ok())
    }

    /// Helper: extract the source slice for a node (owned, trimmed).
    fn node_source(&self, node: &Node, source: &[u8]) -> String {
        let bytes = &source[node.start_byte()..node.end_byte()];
        String::from_utf8_lossy(bytes).trim().to_string()
    }

    /// Helper: build a [`Span`] from a node.
    fn node_span(&self, node: &Node) -> crate::core::types::Span {
        let sp = node.start_position();
        let ep = node.end_position();
        crate::core::types::Span::new(
            node.start_byte(), node.end_byte(),
            sp.row + 1, ep.row + 1,
            sp.column,  ep.column,
        )
    }
}

// ── Blanket impl: TreeSitterParser → LanguageParser ───────────────────────────

impl<T: TreeSitterParser> LanguageParser for T {
    fn language_name(&self) -> &str { TreeSitterParser::language_name(self) }
    fn extensions(&self)    -> &[&str] { TreeSitterParser::extensions(self) }

    fn parse(&self, path: &str, source: &str) -> Result<SourceFile, ParseError> {
        let mut parser = self.build_ts_parser();
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError::InternalError(
                format!("tree-sitter returned None for {path}"),
            ))?;

        // Report hard parse errors (tree-sitter always returns a tree, but
        // marks error nodes when it can't parse something).
        if tree.root_node().has_error() {
            // We continue anyway — partial trees are useful.
            // A warning is added by the engine.
        }

        self.extract_definitions(path, source, &tree)
    }
}
