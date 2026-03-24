//! Solidity language back-end using the `tree-sitter-solidity` grammar.
//!
//! ## Grammar node kinds used
//!
//! ```text
//! source_file
//!   import_directive          → imports
//!   contract_declaration      → container
//!     function_definition     → FunctionDef (public/external/internal/private)
//!     modifier_definition     → FunctionDef (is_modifier=true)
//!     constructor_definition  → FunctionDef
//!     fallback_receive_definition
//!   function_definition       → free functions (Solidity ≥0.7)
//! ```

use tree_sitter::{Node, Tree};

use crate::core::{
    query::extract_callees,
    traits::{ParseError, TreeSitterParser},
    types::{FunctionDef, ModifierRef, Param, SourceFile, Span, Visibility},
};

// ── Public entry point ────────────────────────────────────────────────────────

pub struct SolidityParser;

impl SolidityParser {
    pub fn new() -> Self { Self }
}
impl Default for SolidityParser {
    fn default() -> Self { Self }
}

// ── tree-sitter grammar node kinds ───────────────────────────────────────────

// Call expression node kinds in the Solidity grammar
const CALL_KINDS: &[&str] = &["call_expression"];

impl TreeSitterParser for SolidityParser {
    fn language_name(&self) -> &str { "Solidity" }
    fn extensions(&self)    -> &[&str] { &["sol"] }

    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_solidity::LANGUAGE.into()
    }

    fn extract_definitions(
        &self,
        path:   &str,
        source: &str,
        tree:   &Tree,
    ) -> Result<SourceFile, ParseError> {
        let src = source.as_bytes();
        let root = tree.root_node();

        let mut functions: Vec<FunctionDef> = Vec::new();
        let mut imports:   Vec<String>      = Vec::new();

        // Walk top-level children
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "import_directive" => {
                    if let Some(imp) = extract_import(&child, src) {
                        imports.push(imp);
                    }
                }
                "contract_declaration"
                | "interface_declaration"
                | "library_declaration" => {
                    let container = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(src).ok())
                        .map(|s| s.to_string());

                    let mut body_cursor = child.walk();
                    for member in child.children(&mut body_cursor) {
                        match member.kind() {
                            "function_definition" => {
                                if let Some(def) = extract_function(
                                    &member, src, path, container.as_deref(), false
                                ) {
                                    functions.push(def);
                                }
                            }
                            "modifier_definition" => {
                                if let Some(def) = extract_modifier(
                                    &member, src, path, container.as_deref()
                                ) {
                                    functions.push(def);
                                }
                            }
                            "constructor_definition" => {
                                if let Some(def) = extract_constructor(
                                    &member, src, path, container.as_deref()
                                ) {
                                    functions.push(def);
                                }
                            }
                            "fallback_receive_definition" => {
                                if let Some(def) = extract_fallback_receive(
                                    &member, src, path, container.as_deref()
                                ) {
                                    functions.push(def);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                // Free function (Solidity ≥ 0.7)
                "function_definition" => {
                    if let Some(def) = extract_function(&child, src, path, None, false) {
                        functions.push(def);
                    }
                }
                _ => {}
            }
        }

        Ok(SourceFile {
            path:      path.to_string(),
            content:   source.to_string(),
            functions,
            imports,
            language:  "Solidity".to_string(),
        })
    }
}

// ── Import extraction ─────────────────────────────────────────────────────────

fn extract_import(node: &Node, src: &[u8]) -> Option<String> {
    // The grammar represents: import "path"; or import { X } from "path";
    // We look for any string_literal descendant as the path.
    find_first_string_literal(node, src)
}

fn find_first_string_literal(node: &Node, src: &[u8]) -> Option<String> {
    if node.kind() == "string_literal" || node.kind() == "string" {
        return node.utf8_text(src).ok()
            .map(|s| s.trim_matches(|c| c == '"' || c == '\'').to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(s) = find_first_string_literal(&child, src) {
            return Some(s);
        }
    }
    None
}

// ── Function extraction ───────────────────────────────────────────────────────

fn extract_function(
    node:      &Node,
    src:       &[u8],
    path:      &str,
    container: Option<&str>,
    _is_free:  bool,
) -> Option<FunctionDef> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "<anonymous>".to_string());

    let params   = extract_params(node, src, "parameters");
    let returns  = extract_return_params(node, src);
    let (visibility, mutability) = extract_visibility_mutability(node, src);
    let modifiers = extract_modifier_invocations(node, src);

    let body_node = node.child_by_field_name("body");
    let callees = body_node
        .as_ref()
        .map(|b| extract_callees(b, src, CALL_KINDS))
        .unwrap_or_default();

    let source_text = node_source_text(node, src);
    let span        = node_to_span(node);

    Some(FunctionDef {
        name,
        visibility,
        params,
        returns,
        modifiers,
        file:        path.to_string(),
        span,
        source_text,
        is_modifier: false,
        container:   container.map(|s| s.to_string()),
        callees,
        mutability,
    })
}

fn extract_constructor(
    node:      &Node,
    src:       &[u8],
    path:      &str,
    container: Option<&str>,
) -> Option<FunctionDef> {
    let params    = extract_params(node, src, "parameters");
    let modifiers = extract_modifier_invocations(node, src);
    let mutability = extract_mutability_only(node, src);

    let body_node = node.child_by_field_name("body");
    let callees = body_node
        .as_ref()
        .map(|b| extract_callees(b, src, CALL_KINDS))
        .unwrap_or_default();

    Some(FunctionDef {
        name:       "<constructor>".to_string(),
        visibility:  Visibility::Public,
        params,
        returns:     Vec::new(),
        modifiers,
        file:        path.to_string(),
        span:        node_to_span(node),
        source_text: node_source_text(node, src),
        is_modifier: false,
        container:   container.map(|s| s.to_string()),
        callees,
        mutability,
    })
}

fn extract_fallback_receive(
    node:      &Node,
    src:       &[u8],
    path:      &str,
    container: Option<&str>,
) -> Option<FunctionDef> {
    // Grammar represents receive() and fallback() as fallback_receive_definition
    // Check for a "receive" or "fallback" keyword child
    let name = {
        let mut c = node.walk();
        let mut found = "<fallback>".to_string();
        for child in node.children(&mut c) {
            let text = child.utf8_text(src).unwrap_or("");
            if text == "receive" || text == "fallback" {
                found = text.to_string();
                break;
            }
        }
        found
    };

    let (visibility, mutability) = extract_visibility_mutability(node, src);
    let params   = extract_params(node, src, "parameters");
    let body_node = node.child_by_field_name("body");
    let callees   = body_node
        .as_ref()
        .map(|b| extract_callees(b, src, CALL_KINDS))
        .unwrap_or_default();

    Some(FunctionDef {
        name,
        visibility,
        params,
        returns:     Vec::new(),
        modifiers:   Vec::new(),
        file:        path.to_string(),
        span:        node_to_span(node),
        source_text: node_source_text(node, src),
        is_modifier: false,
        container:   container.map(|s| s.to_string()),
        callees,
        mutability,
    })
}

fn extract_modifier(
    node:      &Node,
    src:       &[u8],
    path:      &str,
    container: Option<&str>,
) -> Option<FunctionDef> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string())?;

    let params    = extract_params(node, src, "parameters");
    let body_node = node.child_by_field_name("body");
    let callees   = body_node
        .as_ref()
        .map(|b| extract_callees(b, src, CALL_KINDS))
        .unwrap_or_default();

    Some(FunctionDef {
        name,
        visibility:  Visibility::Internal,
        params,
        returns:     Vec::new(),
        modifiers:   Vec::new(),
        file:        path.to_string(),
        span:        node_to_span(node),
        source_text: node_source_text(node, src),
        is_modifier: true,
        container:   container.map(|s| s.to_string()),
        callees,
        mutability:  None,
    })
}

// ── Parameter extraction ──────────────────────────────────────────────────────

fn extract_params(node: &Node, src: &[u8], field: &str) -> Vec<Param> {
    let params_node = match node.child_by_field_name(field) {
        Some(n) => n,
        None    => return Vec::new(),
    };

    let mut out = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        if child.kind() == "parameter" {
            let type_name = child
                .child_by_field_name("type")
                .map(|n| node_source_text(&n, src))
                .unwrap_or_default();
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("")
                .to_string();
            out.push(Param { name, type_name });
        }
    }
    out
}

fn extract_return_params(node: &Node, src: &[u8]) -> Vec<Param> {
    // Solidity return params live in a `return_type_definition` or
    // `parameter_list` under a `return_parameters` field.
    let ret_node = match node.child_by_field_name("return_type")
        .or_else(|| node.child_by_field_name("return_parameters")) {
        Some(n) => n,
        None    => return Vec::new(),
    };

    // If it's a single type (non-tuple), wrap it.
    if ret_node.kind() == "parameter_list" || ret_node.kind() == "return_parameters" {
        extract_params(&ret_node, src, "parameter")
            .into_iter()
            .chain(extract_params_direct(&ret_node, src))
            .collect()
    } else {
        vec![Param {
            name:      String::new(),
            type_name: node_source_text(&ret_node, src),
        }]
    }
}

fn extract_params_direct(node: &Node, src: &[u8]) -> Vec<Param> {
    let mut out    = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "parameter" {
            let type_name = child
                .child_by_field_name("type")
                .map(|n| node_source_text(&n, src))
                .unwrap_or_default();
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("")
                .to_string();
            out.push(Param { name, type_name });
        }
    }
    out
}

// ── Visibility / mutability extraction ───────────────────────────────────────

fn extract_visibility_mutability(node: &Node, src: &[u8]) -> (Visibility, Option<String>) {
    let mut visibility  = Visibility::Default;
    let mut mutability  = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "visibility" => {
                let text = child.utf8_text(src).unwrap_or("");
                visibility = match text {
                    "public"   => Visibility::Public,
                    "external" => Visibility::External,
                    "internal" => Visibility::Internal,
                    "private"  => Visibility::Private,
                    _          => Visibility::Default,
                };
            }
            "state_mutability" => {
                mutability = child.utf8_text(src).ok().map(|s| s.to_string());
            }
            _ => {}
        }
    }
    (visibility, mutability)
}

fn extract_mutability_only(node: &Node, src: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "state_mutability" {
            return child.utf8_text(src).ok().map(|s| s.to_string());
        }
    }
    None
}

// ── Modifier invocation extraction ───────────────────────────────────────────

fn extract_modifier_invocations(node: &Node, src: &[u8]) -> Vec<ModifierRef> {
    let mut out    = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "modifier_invocation" {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .map(|s| s.to_string())
                .unwrap_or_default();

            // Collect arguments
            let args = child
                .child_by_field_name("arguments")
                .map(|args_node| {
                    let mut a_cursor = args_node.walk();
                    args_node.children(&mut a_cursor)
                        .filter(|n| n.kind() != "," && n.kind() != "(" && n.kind() != ")")
                        .map(|n| node_source_text(&n, src))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if !name.is_empty() {
                out.push(ModifierRef { name, args });
            }
        }
    }
    out
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn node_source_text(node: &Node, src: &[u8]) -> String {
    let bytes = &src[node.start_byte()..node.end_byte()];
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn node_to_span(node: &Node) -> Span {
    let sp = node.start_position();
    let ep = node.end_position();
    Span::new(
        node.start_byte(), node.end_byte(),
        sp.row + 1,        ep.row + 1,
        sp.column,         ep.column,
    )
}
