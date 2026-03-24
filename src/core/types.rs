//! Language-agnostic data types shared by all back-ends.

use std::collections::HashMap;

// ── Position ─────────────────────────────────────────────────────────────────

/// Byte-accurate source location.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    /// 0-based byte offset of the first character.
    pub start_byte: usize,
    /// 0-based byte offset one past the last character.
    pub end_byte: usize,
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based end line.
    pub end_line: usize,
    /// 0-based start column.
    pub start_col: usize,
    /// 0-based end column.
    pub end_col: usize,
}

impl Span {
    pub fn new(
        start_byte: usize, end_byte: usize,
        start_line: usize, end_line: usize,
        start_col: usize,  end_col: usize,
    ) -> Self {
        Self { start_byte, end_byte, start_line, end_line, start_col, end_col }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.start_line, self.start_col)
    }
}

// ── Visibility ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    /// Solidity `external`, first-class public API.
    External,
    /// Solidity `public`, Python def (no leading _), Rust `pub`.
    Public,
    /// Solidity `internal`, Rust `pub(crate)` / `pub(super)`.
    Internal,
    /// Solidity `private`, Python `__name`, Rust default (no pub).
    Private,
    /// Language doesn't encode visibility explicitly (e.g. interface methods).
    Default,
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::External => write!(f, "external"),
            Visibility::Public   => write!(f, "public"),
            Visibility::Internal => write!(f, "internal"),
            Visibility::Private  => write!(f, "private"),
            Visibility::Default  => write!(f, "default"),
        }
    }
}

// ── Parameter ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Param {
    pub name:      String,
    pub type_name: String,
}

// ── ModifierRef ──────────────────────────────────────────────────────────────

/// Reference to a modifier / decorator applied to a function.
#[derive(Debug, Clone)]
pub struct ModifierRef {
    pub name: String,
    pub args: Vec<String>,
}

// ── FunctionDef ──────────────────────────────────────────────────────────────

/// A fully resolved function (or modifier / decorator / macro) definition.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    /// Canonical name.
    pub name: String,
    /// Visibility / access level.
    pub visibility: Visibility,
    /// Parameter list.
    pub params: Vec<Param>,
    /// Return type(s).
    pub returns: Vec<Param>,
    /// Modifiers / decorators applied to this function.
    pub modifiers: Vec<ModifierRef>,
    /// Source file path.
    pub file: String,
    /// Exact position in source.
    pub span: Span,
    /// Raw source text of the full definition.
    pub source_text: String,
    /// True when this is a modifier / decorator, not a regular function.
    pub is_modifier: bool,
    /// Enclosing contract / class / impl / module name, if any.
    pub container: Option<String>,
    /// Names of functions called from this body (unresolved; engine resolves).
    pub callees: Vec<String>,
    /// State mutability annotation (Solidity view/pure/payable; may be empty).
    pub mutability: Option<String>,
}

impl FunctionDef {
    /// True when this function should be treated as a public entry point.
    pub fn is_entry_point(&self) -> bool {
        matches!(self.visibility, Visibility::Public | Visibility::External)
            && !self.is_modifier
    }
}

// ── SourceFile ───────────────────────────────────────────────────────────────

/// One parsed source file.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path:      String,
    pub content:   String,
    /// All extracted definitions (functions + modifiers).
    pub functions: Vec<FunctionDef>,
    /// Import paths declared in this file.
    pub imports:   Vec<String>,
    /// Language that parsed this file.
    pub language:  String,
}

// ── Call-chain types ─────────────────────────────────────────────────────────

/// One node in a resolved call chain.
#[derive(Debug, Clone)]
pub struct CallChainNode {
    pub function_name:      String,
    pub definition:         FunctionDef,
    pub file:               String,
    /// Depth from entry point (entry = 0).
    pub depth:              usize,
    /// Resolved modifier / decorator definitions applied to this function.
    pub applied_modifiers:  Vec<FunctionDef>,
}

/// The complete call chain rooted at one public/external entry point.
#[derive(Debug, Clone)]
pub struct CallChain {
    pub entry_function:  String,
    pub entry_container: Option<String>,
    pub entry_file:      String,
    /// Nodes in BFS traversal order.
    pub nodes:           Vec<CallChainNode>,
}

impl CallChain {
    /// All unique [`FunctionDef`]s referenced (functions + modifiers, deduped).
    pub fn all_definitions(&self) -> Vec<&FunctionDef> {
        let mut seen = std::collections::HashSet::new();
        let mut out  = Vec::new();
        for node in &self.nodes {
            let k = format!("{}::{}", node.file, node.function_name);
            if seen.insert(k) { out.push(&node.definition); }
            for m in &node.applied_modifiers {
                let mk = format!("{}::{}", m.file, m.name);
                if seen.insert(mk) { out.push(m); }
            }
        }
        out
    }
}

// ── ProjectAnalysis ───────────────────────────────────────────────────────────

/// Top-level result returned by [`crate::engine::ParserEngine::analyze`].
#[derive(Debug, Clone)]
pub struct ProjectAnalysis {
    pub source_files:    Vec<SourceFile>,
    pub call_chains:     Vec<CallChain>,
    /// All definitions keyed by `"file::name"`.
    pub all_definitions: HashMap<String, FunctionDef>,
    pub warnings:        Vec<String>,
}

impl ProjectAnalysis {
    pub fn lookup(&self, file: &str, name: &str) -> Option<&FunctionDef> {
        self.all_definitions.get(&format!("{}::{}", file, name))
    }
    pub fn entry_points(&self) -> impl Iterator<Item = &FunctionDef> {
        self.all_definitions.values().filter(|f| f.is_entry_point())
    }
    pub fn chains_in_file<'a>(&'a self, file: &str) -> impl Iterator<Item = &'a CallChain> {
        let f = file.to_string();
        self.call_chains.iter().filter(move |c| c.entry_file == f)
    }
}
