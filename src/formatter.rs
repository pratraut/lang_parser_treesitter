//! Human-readable formatters for analysis output.

use std::fmt::Write;

use crate::core::types::{CallChain, CallChainNode, FunctionDef, ProjectAnalysis};

// ── Project report ────────────────────────────────────────────────────────────

pub fn format_project(analysis: &ProjectAnalysis) -> String {
    let mut out = String::new();

    writeln!(out, "╔═══════════════════════════════════════════════════════════════╗").unwrap();
    writeln!(out, "║            lang_parser ── Tree-sitter Analysis Report         ║").unwrap();
    writeln!(out, "╚═══════════════════════════════════════════════════════════════╝").unwrap();
    writeln!(out).unwrap();

    // Files
    writeln!(out, "📁  Source files parsed: {}", analysis.source_files.len()).unwrap();
    for sf in &analysis.source_files {
        writeln!(
            out,
            "    [{lang}] {path}  —  {n} definitions",
            lang = sf.language,
            path = sf.path,
            n    = sf.functions.len(),
        ).unwrap();
    }
    writeln!(out).unwrap();

    // Warnings
    if !analysis.warnings.is_empty() {
        writeln!(out, "⚠️  Warnings ({}):", analysis.warnings.len()).unwrap();
        for w in &analysis.warnings {
            writeln!(out, "   ! {w}").unwrap();
        }
        writeln!(out).unwrap();
    }

    // Call chains
    writeln!(out, "🔗  Call chains: {}", analysis.call_chains.len()).unwrap();
    writeln!(out, "{}", "─".repeat(65)).unwrap();
    for (i, chain) in analysis.call_chains.iter().enumerate() {
        writeln!(out, "\n[{:>3}] {}", i + 1, chain_header(chain)).unwrap();
        write!(out, "{}", format_chain(chain)).unwrap();
    }

    // Definitions index
    writeln!(out, "\n{}", "─".repeat(65)).unwrap();
    writeln!(out, "📖  All definitions: {}", analysis.all_definitions.len()).unwrap();
    let mut defs: Vec<_> = analysis.all_definitions.values().collect();
    defs.sort_by(|a, b| {
        a.file.cmp(&b.file).then(a.span.start_line.cmp(&b.span.start_line))
    });
    for d in defs {
        writeln!(out, "    {}", def_one_line(d)).unwrap();
    }

    out
}

// ── Chain formatting ──────────────────────────────────────────────────────────

pub fn chain_header(chain: &CallChain) -> String {
    let container = chain.entry_container.as_deref()
        .map(|c| format!("{c}::"))
        .unwrap_or_default();
    let line = chain.nodes.first()
        .map(|n| n.definition.span.start_line)
        .unwrap_or(0);
    format!("{}{} ← {}:{}", container, chain.entry_function, chain.entry_file, line)
}

pub fn format_chain(chain: &CallChain) -> String {
    let mut out = String::new();
    for node in &chain.nodes {
        format_chain_node(&mut out, node);
    }
    out
}

fn format_chain_node(out: &mut String, node: &CallChainNode) {
    let indent = "  ".repeat(node.depth + 1);
    let container = node.definition.container.as_deref()
        .map(|c| format!("{c}::"))
        .unwrap_or_default();
    let mutability = node.definition.mutability.as_deref()
        .map(|m| format!(" [{m}]"))
        .unwrap_or_default();

    writeln!(
        out,
        "{indent}→ {container}{}{mutability}",
        node.function_name,
    ).unwrap();
    writeln!(
        out,
        "{indent}  📍 {}  line {}",
        node.file, node.definition.span.start_line
    ).unwrap();
    writeln!(
        out,
        "{indent}  👁  {}",
        node.definition.visibility
    ).unwrap();

    if !node.applied_modifiers.is_empty() {
        let names: Vec<_> = node.applied_modifiers.iter().map(|m| m.name.as_str()).collect();
        writeln!(out, "{indent}  🔒 modifiers/decorators: {}", names.join(", ")).unwrap();
        for m in &node.applied_modifiers {
            writeln!(
                out,
                "{indent}     `{}` defined at {}:{}",
                m.name, m.file, m.span.start_line
            ).unwrap();
        }
    }

    if !node.definition.callees.is_empty() {
        writeln!(out, "{indent}  📞 calls: {}", node.definition.callees.join(", ")).unwrap();
    }
}

// ── Definition formatting ─────────────────────────────────────────────────────

pub fn def_one_line(def: &FunctionDef) -> String {
    let kind      = if def.is_modifier { "modifier " } else { "function " };
    let container = def.container.as_deref()
        .map(|c| format!("{c}::"))
        .unwrap_or_default();
    let mutability = def.mutability.as_deref()
        .map(|m| format!(" {m}"))
        .unwrap_or_default();
    format!(
        "{kind}[{vis}] {container}{name}{mutability}  @ {}:{}",
        def.file,
        def.span.start_line,
        vis  = def.visibility,
        name = def.name,
    )
}

pub fn def_detail(def: &FunctionDef) -> String {
    let mut out = String::new();
    let kind    = if def.is_modifier { "MODIFIER" } else { "FUNCTION" };

    writeln!(out, "┌─ {kind}: {} ──────────────────────────────────", def.name).unwrap();
    if let Some(c) = &def.container {
        writeln!(out, "│  container:  {c}").unwrap();
    }
    writeln!(out, "│  file:       {}", def.file).unwrap();
    writeln!(out, "│  lines:      {}–{}", def.span.start_line, def.span.end_line).unwrap();
    writeln!(out, "│  bytes:      {}–{}", def.span.start_byte, def.span.end_byte).unwrap();
    writeln!(out, "│  visibility: {}", def.visibility).unwrap();
    if let Some(m) = &def.mutability {
        writeln!(out, "│  mutability: {m}").unwrap();
    }
    if !def.params.is_empty() {
        writeln!(out, "│  params:").unwrap();
        for p in &def.params {
            if p.type_name.is_empty() {
                writeln!(out, "│    • {}", p.name).unwrap();
            } else {
                writeln!(out, "│    • {} {}", p.type_name, p.name).unwrap();
            }
        }
    }
    if !def.returns.is_empty() {
        writeln!(out, "│  returns:").unwrap();
        for r in &def.returns {
            if r.name.is_empty() {
                writeln!(out, "│    • {}", r.type_name).unwrap();
            } else {
                writeln!(out, "│    • {} {}", r.type_name, r.name).unwrap();
            }
        }
    }
    if !def.modifiers.is_empty() {
        writeln!(out, "│  modifiers / decorators:").unwrap();
        for m in &def.modifiers {
            if m.args.is_empty() {
                writeln!(out, "│    • {}", m.name).unwrap();
            } else {
                writeln!(out, "│    • {}({})", m.name, m.args.join(", ")).unwrap();
            }
        }
    }
    if !def.callees.is_empty() {
        writeln!(out, "│  calls: {}", def.callees.join(", ")).unwrap();
    }
    writeln!(out, "│").unwrap();
    writeln!(out, "│  source:").unwrap();
    for line in def.source_text.lines() {
        writeln!(out, "│    {line}").unwrap();
    }
    writeln!(out, "└───────────────────────────────────────────────────").unwrap();
    out
}
