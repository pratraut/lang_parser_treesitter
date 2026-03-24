//! Shared tree-sitter query utilities.
//!
//! Provides [`TsQuery`] — a thin wrapper that compiles a query once and lets
//! callers iterate over pattern matches on any node.

use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator};

/// A compiled tree-sitter query.
pub struct TsQuery {
    pub query: Query,
}

impl TsQuery {
    /// Compile `source` against `language`.  Panics on invalid query syntax.
    pub fn new(language: &Language, source: &str) -> Self {
        let query = Query::new(language, source)
            .unwrap_or_else(|e| panic!("invalid tree-sitter query: {e}\nQuery:\n{source}"));
        Self { query }
    }

    /// Iterate over all matches of this query within `node`.
    pub fn matches<'tree>(
        &self,
        node: &Node<'tree>,
        source: &'tree [u8],
    ) -> Vec<CaptureMap<'tree>> {
        let mut cursor = QueryCursor::new();
        let mut results = Vec::new();

        let mut raw_matches = cursor.matches(&self.query, *node, source);

        while let Some(m) = raw_matches.next() {
            let mut map = CaptureMap::new();

            for capture in m.captures {
                let name = self.query.capture_names()[capture.index as usize].to_string();
                map.insert(name, capture.node);
            }

            results.push(map);
        }

        results
    }

    pub fn capture_names(&self) -> &[&str] {
        self.query.capture_names()
    }
}

// ── CaptureMap ───────────────────────────────────────────────────────────────

/// Map from capture name → matched node for one query match.
/// Uses owned `String` keys to avoid lifetime entanglement with the `Query`.
#[derive(Debug, Clone)]
pub struct CaptureMap<'tree> {
    inner: Vec<(String, Node<'tree>)>,
}

impl<'tree> CaptureMap<'tree> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn insert(&mut self, name: String, node: Node<'tree>) {
        self.inner.push((name, node));
    }

    /// Look up a node by capture name.
    pub fn get(&self, name: &str) -> Option<Node<'tree>> {
        self.inner.iter().find(|(n, _)| n == name).map(|(_, node)| *node)
    }

    /// Get text of the named capture from source bytes.
    pub fn text<'s>(&self, name: &str, source: &'s [u8]) -> Option<&'s str> {
        self.get(name).and_then(|n| n.utf8_text(source).ok())
    }

    /// Get owned text of the named capture.
    pub fn text_owned(&self, name: &str, source: &[u8]) -> Option<String> {
        self.text(name, source).map(|s| s.to_string())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Node<'tree>)> {
        self.inner.iter().map(|(n, node)| (n.as_str(), *node))
    }
}

impl<'tree> Default for CaptureMap<'tree> {
    fn default() -> Self { Self::new() }
}

// ── call-site extractor ───────────────────────────────────────────────────────

/// Walk a function body node and collect all called function names.
///
/// Looks for call nodes (`call_expression`, `method_call_expression`, `call`)
/// whose `function` / `name` field contains an identifier or member expression,
/// and extracts the leaf function name.
///
/// Returns a deduplicated Vec of names.
pub fn extract_callees(body_node: &Node, source: &[u8], call_node_kinds: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    collect_callees_recursive(body_node, source, call_node_kinds, &mut names);

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

fn collect_callees_recursive(
    node:            &Node,
    source:          &[u8],
    call_node_kinds: &[&str],
    out:             &mut Vec<String>,
) {
    if call_node_kinds.contains(&node.kind()) {
        let fn_node = node.child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"));

        if let Some(fn_node) = fn_node {
            let name = match fn_node.kind() {
                "identifier" | "name" => {
                    fn_node.utf8_text(source).ok().map(|s| s.to_string())
                }
                "member_expression" | "field_expression" | "attribute" => {
                    fn_node.child_by_field_name("property")
                        .or_else(|| fn_node.child_by_field_name("field"))
                        .or_else(|| fn_node.child_by_field_name("attribute"))
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string())
                }
                "scoped_identifier" => {
                    fn_node.child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string())
                }
                _ => fn_node.utf8_text(source).ok().map(|s| s.to_string()),
            };
            if let Some(n) = name {
                if !n.is_empty() { out.push(n); }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_callees_recursive(&child, source, call_node_kinds, out);
    }
}
