//! Rust language back-end using the `tree-sitter-rust` grammar.
//!
//! ## What counts as an entry point?
//!
//! - `pub fn`  (Visibility::Public)
//! - `pub(crate) fn` → Internal
//! - bare `fn` (no pub) → Private
//!
//! ## Node kinds used
//!
//! ```text
//! source_file
//!   function_item                 → top-level fn
//!   impl_item                     → impl block
//!     function_item               → method
//!   mod_item                      → module (recurse)
//! ```

use tree_sitter::{Node, Tree};

use crate::core::{
    query::extract_callees,
    traits::{ParseError, TreeSitterParser},
    types::{FunctionDef, Param, SourceFile, Span, Visibility},
};

pub struct RustParser;

impl RustParser {
    pub fn new() -> Self { Self }
}
impl Default for RustParser {
    fn default() -> Self { Self }
}

// Call node kinds in the Rust grammar
const CALL_KINDS: &[&str] = &["call_expression", "method_call_expression"];

impl TreeSitterParser for RustParser {
    fn language_name(&self) -> &str { "Rust" }
    fn extensions(&self)    -> &[&str] { &["rs"] }

    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_rust::language()
    }

    fn extract_definitions(
        &self,
        path:   &str,
        source: &str,
        tree:   &Tree,
    ) -> Result<SourceFile, ParseError> {
        let src  = source.as_bytes();
        let root = tree.root_node();

        let mut functions: Vec<FunctionDef> = Vec::new();
        let mut imports:   Vec<String>      = Vec::new();

        visit_node(&root, src, path, None, &mut functions, &mut imports);

        Ok(SourceFile {
            path:     path.to_string(),
            content:  source.to_string(),
            functions,
            imports,
            language: "Rust".to_string(),
        })
    }
}

fn visit_node(
    node:      &Node,
    src:       &[u8],
    path:      &str,
    container: Option<&str>,
    out:       &mut Vec<FunctionDef>,
    imports:   &mut Vec<String>,
) {
    match node.kind() {
        "function_item" => {
            if let Some(def) = extract_function(node, src, path, container) {
                out.push(def);
            }
        }
        "impl_item" => {
            // impl Type { ... }  or  impl Trait for Type { ... }
            // The container name is the implementing type
            let container_name = node
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(src).ok())
                .map(|s| s.to_string());

            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    visit_node(
                        &child, src, path,
                        container_name.as_deref().or(container),
                        out, imports,
                    );
                }
            }
        }
        "mod_item" => {
            // Recurse into inline modules
            let mod_name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .map(|s| s.to_string());

            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    visit_node(
                        &child, src, path,
                        mod_name.as_deref().or(container),
                        out, imports,
                    );
                }
            }
        }
        "use_declaration" => {
            // `use path::to::thing;`
            if let Some(arg) = node.child_by_field_name("argument") {
                imports.push(
                    arg.utf8_text(src).unwrap_or("").to_string()
                );
            }
        }
        "trait_item" => {
            // Trait definitions — recurse into their method signatures
            let trait_name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .map(|s| s.to_string());

            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    visit_node(
                        &child, src, path,
                        trait_name.as_deref().or(container),
                        out, imports,
                    );
                }
            }
        }
        _ => {
            // Recurse into any other node
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                visit_node(&child, src, path, container, out, imports);
            }
        }
    }
}

fn extract_function(
    node:      &Node,
    src:       &[u8],
    path:      &str,
    container: Option<&str>,
) -> Option<FunctionDef> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string())?;

    let visibility = extract_visibility(node, src);
    let params     = extract_params(node, src);
    let returns    = extract_return_type(node, src);

    let body_node = node.child_by_field_name("body");
    let callees   = body_node
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
        modifiers:   Vec::new(),   // Rust uses attributes, not modifiers
        file:        path.to_string(),
        span,
        source_text,
        is_modifier: false,
        container:   container.map(|s| s.to_string()),
        callees,
        mutability:  None,
    })
}

fn extract_visibility(node: &Node, src: &[u8]) -> Visibility {
    // The `visibility_modifier` child contains `pub`, `pub(crate)`, etc.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = child.utf8_text(src).unwrap_or("");
            return match text {
                "pub"                => Visibility::Public,
                t if t.contains("crate") || t.contains("super") => Visibility::Internal,
                _                   => Visibility::Public,
            };
        }
    }
    Visibility::Private
}

fn extract_params(node: &Node, src: &[u8]) -> Vec<Param> {
    let params_node = match node.child_by_field_name("parameters") {
        Some(n) => n,
        None    => return Vec::new(),
    };

    let mut out    = Vec::new();
    let mut cursor = params_node.walk();

    for child in params_node.children(&mut cursor) {
        match child.kind() {
            "parameter" => {
                let name = child
                    .child_by_field_name("pattern")
                    .and_then(|n| n.utf8_text(src).ok())
                    .unwrap_or("")
                    .to_string();
                let type_name = child
                    .child_by_field_name("type")
                    .map(|n| node_source_text(&n, src))
                    .unwrap_or_default();
                out.push(Param { name, type_name });
            }
            // `self`, `&self`, `&mut self`
            "self_parameter" | "variadic_parameter" => {
                let text = child.utf8_text(src).unwrap_or("self").to_string();
                out.push(Param { name: text, type_name: "Self".to_string() });
            }
            _ => {}
        }
    }
    out
}

fn extract_return_type(node: &Node, src: &[u8]) -> Vec<Param> {
    match node.child_by_field_name("return_type") {
        None    => Vec::new(),
        Some(r) => vec![Param {
            name:      String::new(),
            type_name: node_source_text(&r, src),
        }],
    }
}

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
