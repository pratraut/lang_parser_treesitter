//! `cargo run --example analyze`
//!
//! Analyzes the bundled example files across three languages and prints the
//! full call-chain report.

use lang_parser::{
    engine::ParserEngine,
    formatter::{chain_header, def_detail, format_chain, format_project},
    languages::{
        python::PythonParser,
        rust_lang::RustParser,
        solidity::SolidityParser,
    },
};

const TOKEN_SOL:   &str = include_str!("Token.sol");
const OWNABLE_SOL: &str = include_str!("Ownable.sol");
const IERC20_SOL:  &str = include_str!("IERC20.sol");
const LEDGER_PY:   &str = include_str!("ledger.py");
const VAULT_RS:    &str = include_str!("vault.rs");

fn main() {
    // ── 1. Build engine with all three language back-ends ─────────────────────
    let mut engine = ParserEngine::new();
    engine.register(Box::new(SolidityParser::new()));
    engine.register(Box::new(RustParser::new()));
    engine.register(Box::new(PythonParser::new()));

    // ── 2. Add source files ───────────────────────────────────────────────────
    engine.add_source("examples/Token.sol",   TOKEN_SOL);
    engine.add_source("examples/Ownable.sol", OWNABLE_SOL);
    engine.add_source("examples/IERC20.sol",  IERC20_SOL);
    engine.add_source("examples/ledger.py",   LEDGER_PY);
    engine.add_source("examples/vault.rs",    VAULT_RS);

    // ── 3. Run analysis ───────────────────────────────────────────────────────
    let analysis = engine.analyze().expect("analysis failed");

    // ── 4. Project-level summary ──────────────────────────────────────────────
    println!("{}", format_project(&analysis));

    // ── 5. Deep-dive into one chain per language ──────────────────────────────

    // Solidity: Token::transfer
    println!("\n══ Deep-dive: Solidity Token::transfer ═══════════════════════\n");
    if let Some(chain) = analysis.call_chains.iter()
        .find(|c| c.entry_function == "transfer" && c.entry_file.contains("Token.sol"))
    {
        println!("Entry: {}", chain_header(chain));
        print!("{}", format_chain(chain));
        println!("\n── Definitions referenced in this chain ──\n");
        for def in chain.all_definitions() {
            print!("{}", def_detail(def));
        }
    }

    // Rust: Vault::transfer
    println!("\n══ Deep-dive: Rust Vault::transfer ═══════════════════════════\n");
    if let Some(chain) = analysis.call_chains.iter()
        .find(|c| c.entry_function == "transfer" && c.entry_file.contains("vault.rs"))
    {
        println!("Entry: {}", chain_header(chain));
        print!("{}", format_chain(chain));
    }

    // Python: TokenLedger::transfer
    println!("\n══ Deep-dive: Python TokenLedger::transfer ═══════════════════\n");
    if let Some(chain) = analysis.call_chains.iter()
        .find(|c| c.entry_function == "transfer" && c.entry_file.contains("ledger.py"))
    {
        println!("Entry: {}", chain_header(chain));
        print!("{}", format_chain(chain));
    }

    // ── 6. Cross-language stats ───────────────────────────────────────────────
    println!("\n══ Cross-language entry-point summary ════════════════════════\n");
    for sf in &analysis.source_files {
        let entry_count = sf.functions.iter().filter(|f| f.is_entry_point()).count();
        println!(
            "  [{lang}] {path}: {total} defs, {entry} entry points",
            lang  = sf.language,
            path  = sf.path,
            total = sf.functions.len(),
            entry = entry_count,
        );
    }

    if !analysis.warnings.is_empty() {
        println!("\n⚠️  Warnings:");
        for w in &analysis.warnings {
            println!("  • {w}");
        }
    }
}
