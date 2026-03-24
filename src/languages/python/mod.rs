//! Python language back-end using the `tree-sitter-python` grammar.
//!
//! ## Entry-point heuristic
//!
//! - `def name` with no leading `_`       → Public
//! - `def _name` (single underscore)      → Internal
//! - `def __name` (double underscore)     → Private (dunder methods)
//! - Methods inside a class               → container = class name
//!
//! ## Decorator support
//!
//! Python decorators (`@decorator`) are stored as `ModifierRef`s.
//!
//! ## Node kinds used
//!
//! ```text
//! module
//!   function_definition       → top-level function
//!   class_definition
//!     function_definition     → method
//!   decorated_definition      → @decorator function_definition
//!   import_statement          → imports
//!   import_from_statement     → imports
//! ```

use tree_sitter::{Node, Tree};

use crate::core::{
    query::extract_callees,
    traits::{ParseError, TreeSitterParser},
    types::{FunctionDef, ModifierRef, Param, SourceFile, Span, Visibility},
};

pub struct PythonParser;

impl PythonParser {
    pub fn new() -> Self { Self }
}
impl Default for PythonParser {
    fn default() -> Self { Self }
}

const CALL_KINDS: &[&str] = &["call"];

impl TreeSitterParser for PythonParser {
    fn language_name(&self) -> &str { "Python" }
    fn extensions(&self)    -> &[&str] { &["py"] }

    fn tree_sitter_language(&self) -> tree_sitter::Language {
        tree_sitter_python::language()
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
            language: "Python".to_string(),
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
        "function_definition" => {
            if let Some(def) = extract_function(node, src, path, container, &[]) {
                out.push(def);
            }
        }
        "decorated_definition" => {
            let decorators  = collect_decorators(node, src);
            let fn_node     = node.children(&mut node.walk())
                .find(|n| n.kind() == "function_definition" || n.kind() == "class_definition");

            if let Some(inner) = fn_node {
                if inner.kind() == "function_definition" {
                    if let Some(def) = extract_function(&inner, src, path, container, &decorators) {
                        out.push(def);
                    }
                } else {
                    // decorated class
                    visit_node(&inner, src, path, container, out, imports);
                }
            }
        }
        "class_definition" => {
            let class_name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .map(|s| s.to_string());

            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    visit_node(
                        &child, src, path,
                        class_name.as_deref().or(container),
                        out, imports,
                    );
                }
            }
        }
        "import_statement" => {
            // import foo, bar
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "dotted_name" || child.kind() == "aliased_import" {
                    imports.push(child.utf8_text(src).unwrap_or("").to_string());
                }
            }
        }
        "import_from_statement" => {
            // from foo import bar
            if let Some(module) = node.child_by_field_name("module_name") {
                imports.push(module.utf8_text(src).unwrap_or("").to_string());
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                visit_node(&child, src, path, container, out, imports);
            }
        }
    }
}

fn extract_function(
    node:       &Node,
    src:        &[u8],
    path:       &str,
    container:  Option<&str>,
    decorators: &[ModifierRef],
) -> Option<FunctionDef> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src).ok())
        .map(|s| s.to_string())?;

    let visibility = python_visibility(&name);
    let params     = extract_params(node, src);
    let returns    = extract_return_annotation(node, src);

    let body_node = node.child_by_field_name("body");
    let callees   = body_node
        .as_ref()
        .map(|b| extract_callees(b, src, CALL_KINDS))
        .unwrap_or_default();

    Some(FunctionDef {
        name,
        visibility,
        params,
        returns,
        modifiers:   decorators.to_vec(),
        file:        path.to_string(),
        span:        node_to_span(node),
        source_text: node_source_text(node, src),
        is_modifier: false,
        container:   container.map(|s| s.to_string()),
        callees,
        mutability:  None,
    })
}

fn python_visibility(name: &str) -> Visibility {
    if name.starts_with("__") && name.ends_with("__") {
        Visibility::Private   // dunder methods
    } else if name.starts_with("__") {
        Visibility::Private   // name-mangled
    } else if name.starts_with('_') {
        Visibility::Internal  // convention: internal
    } else {
        Visibility::Public
    }
}

fn collect_decorators(decorated_node: &Node, src: &[u8]) -> Vec<ModifierRef> {
    let mut out    = Vec::new();
    let mut cursor = decorated_node.walk();
    for child in decorated_node.children(&mut cursor) {
        if child.kind() == "decorator" {
            // decorator text: `@app.route("/")` or `@staticmethod`
            let text = child.utf8_text(src).unwrap_or("").trim_start_matches('@');
            // Split name from args  (rough: split on first `(`)
            let (name, args) = if let Some(p) = text.find('(') {
                let nm = text[..p].trim().to_string();
                let arg_str = text[p+1..].trim_end_matches(')').trim().to_string();
                let args: Vec<String> = if arg_str.is_empty() { vec![] }
                    else { vec![arg_str] };
                (nm, args)
            } else {
                (text.trim().to_string(), vec![])
            };
            if !name.is_empty() {
                out.push(ModifierRef { name, args });
            }
        }
    }
    out
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
            "identifier" => {
                let name = child.utf8_text(src).unwrap_or("").to_string();
                out.push(Param { name, type_name: String::new() });
            }
            "typed_parameter" => {
                let name = child.children(&mut child.walk())
                    .find(|n| n.kind() == "identifier")
                    .and_then(|n| n.utf8_text(src).ok())
                    .unwrap_or("")
                    .to_string();
                let type_name = child
                    .child_by_field_name("type")
                    .map(|n| node_source_text(&n, src))
                    .unwrap_or_default();
                out.push(Param { name, type_name });
            }
            "default_parameter" | "typed_default_parameter" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(src).ok())
                    .unwrap_or("")
                    .to_string();
                let type_name = child
                    .child_by_field_name("type")
                    .map(|n| node_source_text(&n, src))
                    .unwrap_or_default();
                out.push(Param { name, type_name });
            }
            "list_splat_pattern" | "dictionary_splat_pattern" => {
                // *args or **kwargs
                let text = child.utf8_text(src).unwrap_or("").to_string();
                out.push(Param { name: text, type_name: String::new() });
            }
            _ => {}
        }
    }
    out
}

fn extract_return_annotation(node: &Node, src: &[u8]) -> Vec<Param> {
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
