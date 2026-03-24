//! Test suite for lang_parser (tree-sitter edition).
//!
//! Run with: `cargo test`

// ── Solidity tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod solidity_tests {
    use crate::{
        core::traits::LanguageParser,
        core::types::Visibility,
        languages::solidity::SolidityParser,
    };

    fn parse(src: &str) -> crate::core::types::SourceFile {
        SolidityParser::new().parse("test.sol", src).expect("parse failed")
    }

    // ── basic parsing ─────────────────────────────────────────────────────────

    #[test]
    fn parse_public_view_function() {
        let sf = parse(r#"
pragma solidity ^0.8.0;
contract C {
    function foo() public view returns (uint256) { return 42; }
}
"#);
        let foo = sf.functions.iter().find(|f| f.name == "foo").expect("foo");
        assert!(matches!(foo.visibility, Visibility::Public));
        assert_eq!(foo.mutability.as_deref(), Some("view"));
        assert!(!foo.returns.is_empty());
    }

    #[test]
    fn parse_external_function() {
        let sf = parse(r#"
contract C {
    function send(address to, uint256 amt) external returns (bool) { return true; }
}
"#);
        let f = sf.functions.iter().find(|f| f.name == "send").unwrap();
        assert!(matches!(f.visibility, Visibility::External));
        assert_eq!(f.params.len(), 2);
    }

    #[test]
    fn parse_internal_private_functions() {
        let sf = parse(r#"
contract C {
    function _internal() internal {}
    function _private() private {}
}
"#);
        assert!(matches!(
            sf.functions.iter().find(|f| f.name == "_internal").unwrap().visibility,
            Visibility::Internal
        ));
        assert!(matches!(
            sf.functions.iter().find(|f| f.name == "_private").unwrap().visibility,
            Visibility::Private
        ));
    }

    #[test]
    fn parse_modifier_definition() {
        let sf = parse(r#"
contract C {
    modifier onlyOwner() {
        require(msg.sender == owner);
        _;
    }
}
"#);
        let m = sf.functions.iter().find(|f| f.is_modifier && f.name == "onlyOwner").unwrap();
        assert!(m.is_modifier);
    }

    #[test]
    fn parse_modifier_invocations_on_function() {
        let sf = parse(r#"
contract C {
    modifier onlyOwner() { _; }
    modifier nonReentrant() { _; }
    function withdraw(uint256 amount) public onlyOwner nonReentrant {}
}
"#);
        let w = sf.functions.iter().find(|f| f.name == "withdraw").unwrap();
        let mods: Vec<_> = w.modifiers.iter().map(|m| m.name.as_str()).collect();
        assert!(mods.contains(&"onlyOwner"),    "mods: {mods:?}");
        assert!(mods.contains(&"nonReentrant"), "mods: {mods:?}");
    }

    #[test]
    fn parse_constructor() {
        let sf = parse(r#"
contract C {
    constructor(string memory name, uint256 supply) {
        _mint(msg.sender, supply);
    }
}
"#);
        assert!(sf.functions.iter().any(|f| f.name == "<constructor>"));
    }

    #[test]
    fn parse_receive_and_fallback() {
        let sf = parse(r#"
contract C {
    receive()  external payable {}
    fallback() external {}
}
"#);
        assert!(sf.functions.iter().any(|f| f.name == "receive"),  "no receive");
        assert!(sf.functions.iter().any(|f| f.name == "fallback"), "no fallback");
    }

    #[test]
    fn parse_interface_functions() {
        let sf = parse(r#"
interface IERC20 {
    function totalSupply() external view returns (uint256);
    function transfer(address to, uint256 amount) external returns (bool);
}
"#);
        assert!(sf.functions.iter().any(|f| f.name == "totalSupply"));
        assert!(sf.functions.iter().any(|f| f.name == "transfer"));
    }

    #[test]
    fn parse_library_function() {
        let sf = parse(r#"
library SafeMath {
    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}
"#);
        let f = sf.functions.iter().find(|f| f.name == "add").unwrap();
        assert!(matches!(f.visibility, Visibility::Internal));
        assert_eq!(f.mutability.as_deref(), Some("pure"));
    }

    #[test]
    fn callee_extraction_from_body() {
        let sf = parse(r#"
contract C {
    function entry() public { _a(); _b(); }
    function _a() internal { _c(); }
    function _b() internal {}
    function _c() internal {}
}
"#);
        let entry = sf.functions.iter().find(|f| f.name == "entry").unwrap();
        assert!(entry.callees.contains(&"_a".to_string()), "callees: {:?}", entry.callees);
        assert!(entry.callees.contains(&"_b".to_string()), "callees: {:?}", entry.callees);
    }

    #[test]
    fn parse_import_paths() {
        let sf = parse(r#"
import "./Ownable.sol";
import { IERC20 } from "./IERC20.sol";
contract C {}
"#);
        assert!(sf.imports.iter().any(|i| i.contains("Ownable")),  "imports: {:?}", sf.imports);
        assert!(sf.imports.iter().any(|i| i.contains("IERC20.sol")), "imports: {:?}", sf.imports);
    }

    #[test]
    fn span_line_numbers_are_correct() {
        let src = "contract C {\n    function foo() public {}\n}\n";
        let sf = parse(src);
        let foo = sf.functions.iter().find(|f| f.name == "foo").unwrap();
        assert_eq!(foo.span.start_line, 2, "foo should start on line 2");
    }

    #[test]
    fn entry_point_detection() {
        let sf = parse(r#"
contract C {
    function pubFn()  public   {}
    function extFn()  external {}
    function intFn()  internal {}
    function privFn() private  {}
}
"#);
        let is_entry = |name: &str| sf.functions.iter()
            .find(|f| f.name == name).map(|f| f.is_entry_point()).unwrap_or(false);
        assert!( is_entry("pubFn"),  "public should be entry");
        assert!( is_entry("extFn"),  "external should be entry");
        assert!(!is_entry("intFn"),  "internal should not be entry");
        assert!(!is_entry("privFn"), "private should not be entry");
    }

    #[test]
    fn source_text_is_populated() {
        let sf = parse(r#"
contract C {
    function foo() public view returns (uint256) { return 1; }
}
"#);
        let foo = sf.functions.iter().find(|f| f.name == "foo").unwrap();
        assert!(foo.source_text.contains("function"), "source_text: {}", foo.source_text);
        assert!(foo.source_text.contains("foo"),      "source_text: {}", foo.source_text);
    }

    #[test]
    fn container_is_set() {
        let sf = parse(r#"
contract MyToken {
    function mint() public {}
}
"#);
        let mint = sf.functions.iter().find(|f| f.name == "mint").unwrap();
        assert_eq!(mint.container.as_deref(), Some("MyToken"));
    }
}

// ── Rust tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod rust_tests {
    use crate::{
        core::traits::LanguageParser,
        core::types::Visibility,
        languages::rust_lang::RustParser,
    };

    fn parse(src: &str) -> crate::core::types::SourceFile {
        RustParser::new().parse("test.rs", src).expect("parse failed")
    }

    #[test]
    fn parse_pub_function() {
        let sf = parse("pub fn greet() { println!(\"hi\"); }");
        let f = sf.functions.iter().find(|f| f.name == "greet").unwrap();
        assert!(matches!(f.visibility, Visibility::Public));
    }

    #[test]
    fn parse_private_function() {
        let sf = parse("fn helper() -> u32 { 42 }");
        let f = sf.functions.iter().find(|f| f.name == "helper").unwrap();
        assert!(matches!(f.visibility, Visibility::Private));
    }

    #[test]
    fn parse_pub_crate_function() {
        let sf = parse("pub(crate) fn internal_helper() {}");
        let f = sf.functions.iter().find(|f| f.name == "internal_helper").unwrap();
        assert!(matches!(f.visibility, Visibility::Internal));
    }

    #[test]
    fn parse_impl_methods() {
        let sf = parse(r#"
struct Foo;
impl Foo {
    pub fn bar(&self) -> u32 { 0 }
    fn baz(&mut self) {}
}
"#);
        let bar = sf.functions.iter().find(|f| f.name == "bar").unwrap();
        let baz = sf.functions.iter().find(|f| f.name == "baz").unwrap();
        assert!(matches!(bar.visibility, Visibility::Public));
        assert!(matches!(baz.visibility, Visibility::Private));
        assert_eq!(bar.container.as_deref(), Some("Foo"));
    }

    #[test]
    fn parse_return_type() {
        let sf = parse("pub fn add(a: u64, b: u64) -> u64 { a + b }");
        let f = sf.functions.iter().find(|f| f.name == "add").unwrap();
        assert!(!f.returns.is_empty());
        assert!(f.returns[0].type_name.contains("u64"), "returns: {:?}", f.returns);
    }

    #[test]
    fn parse_parameters() {
        let sf = parse("pub fn process(name: String, count: usize) {}");
        let f = sf.functions.iter().find(|f| f.name == "process").unwrap();
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].type_name, "String");
        assert_eq!(f.params[1].type_name, "usize");
    }

    #[test]
    fn callee_extraction() {
        let sf = parse(r#"
pub fn entry() { helper_a(); helper_b(); }
fn helper_a() { deep(); }
fn helper_b() {}
fn deep() {}
"#);
        let entry = sf.functions.iter().find(|f| f.name == "entry").unwrap();
        assert!(entry.callees.contains(&"helper_a".to_string()), "{:?}", entry.callees);
        assert!(entry.callees.contains(&"helper_b".to_string()), "{:?}", entry.callees);
    }

    #[test]
    fn method_call_extraction() {
        let sf = parse(r#"
pub fn run(vault: &mut Vault) {
    vault.deposit(100);
    vault.transfer("a", "b", 50);
}
"#);
        let f = sf.functions.iter().find(|f| f.name == "run").unwrap();
        assert!(f.callees.contains(&"deposit".to_string()),  "{:?}", f.callees);
        assert!(f.callees.contains(&"transfer".to_string()), "{:?}", f.callees);
    }

    #[test]
    fn inline_mod_functions() {
        let sf = parse(r#"
mod inner {
    pub fn inner_pub() {}
    fn inner_priv() {}
}
"#);
        assert!(sf.functions.iter().any(|f| f.name == "inner_pub"));
        assert!(sf.functions.iter().any(|f| f.name == "inner_priv"));
    }

    #[test]
    fn span_is_correct() {
        let src = "// comment\npub fn hello() {}\n";
        let sf = parse(src);
        let f = sf.functions.iter().find(|f| f.name == "hello").unwrap();
        assert_eq!(f.span.start_line, 2);
    }
}

// ── Python tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod python_tests {
    use crate::{
        core::traits::LanguageParser,
        core::types::Visibility,
        languages::python::PythonParser,
    };

    fn parse(src: &str) -> crate::core::types::SourceFile {
        PythonParser::new().parse("test.py", src).expect("parse failed")
    }

    #[test]
    fn parse_public_function() {
        let sf = parse("def greet(name: str) -> str:\n    return f'Hello {name}'\n");
        let f = sf.functions.iter().find(|f| f.name == "greet").unwrap();
        assert!(matches!(f.visibility, Visibility::Public));
    }

    #[test]
    fn parse_internal_function() {
        let sf = parse("def _helper():\n    pass\n");
        let f = sf.functions.iter().find(|f| f.name == "_helper").unwrap();
        assert!(matches!(f.visibility, Visibility::Internal));
    }

    #[test]
    fn parse_private_dunder() {
        let sf = parse("def __init__(self):\n    pass\n");
        let f = sf.functions.iter().find(|f| f.name == "__init__").unwrap();
        assert!(matches!(f.visibility, Visibility::Private));
    }

    #[test]
    fn parse_class_methods() {
        let sf = parse(r#"
class MyClass:
    def public_method(self):
        pass
    def _internal(self):
        pass
    def __init__(self):
        pass
"#);
        assert!(sf.functions.iter().any(|f| f.name == "public_method"));
        assert!(sf.functions.iter().any(|f| f.name == "_internal"));
        assert!(sf.functions.iter().any(|f| f.name == "__init__"));
        // Container should be set
        let pm = sf.functions.iter().find(|f| f.name == "public_method").unwrap();
        assert_eq!(pm.container.as_deref(), Some("MyClass"));
    }

    #[test]
    fn parse_decorator() {
        let sf = parse(r#"
class App:
    @staticmethod
    def handler():
        pass
"#);
        let handler = sf.functions.iter().find(|f| f.name == "handler").unwrap();
        assert!(
            handler.modifiers.iter().any(|m| m.name == "staticmethod"),
            "modifiers: {:?}", handler.modifiers
        );
    }

    #[test]
    fn parse_return_annotation() {
        let sf = parse("def compute(x: int, y: int) -> int:\n    return x + y\n");
        let f = sf.functions.iter().find(|f| f.name == "compute").unwrap();
        assert!(!f.returns.is_empty(), "should have return type");
        assert!(f.returns[0].type_name.contains("int"), "returns: {:?}", f.returns);
    }

    #[test]
    fn callee_extraction() {
        let sf = parse(r#"
def entry():
    helper_a()
    helper_b()

def helper_a():
    deep()

def helper_b():
    pass

def deep():
    pass
"#);
        let e = sf.functions.iter().find(|f| f.name == "entry").unwrap();
        assert!(e.callees.contains(&"helper_a".to_string()), "{:?}", e.callees);
        assert!(e.callees.contains(&"helper_b".to_string()), "{:?}", e.callees);
    }

    #[test]
    fn parse_imports() {
        let sf = parse("import os\nimport json\nfrom pathlib import Path\n\ndef foo(): pass\n");
        assert!(sf.imports.iter().any(|i| i.contains("os")),       "{:?}", sf.imports);
        assert!(sf.imports.iter().any(|i| i.contains("json")),     "{:?}", sf.imports);
        assert!(sf.imports.iter().any(|i| i.contains("pathlib")),  "{:?}", sf.imports);
    }
}

// ── Engine / resolver tests ───────────────────────────────────────────────────

#[cfg(test)]
mod engine_tests {
    use crate::engine::ParserEngine;
    use crate::languages::{
        solidity::SolidityParser,
        rust_lang::RustParser,
        python::PythonParser,
    };

    fn sol_engine(files: &[(&str, &str)]) -> crate::core::types::ProjectAnalysis {
        let mut e = ParserEngine::new();
        e.register(Box::new(SolidityParser::new()));
        for (p, s) in files { e.add_source(*p, *s); }
        e.analyze().unwrap()
    }

    fn multi_engine(files: &[(&str, &str)]) -> crate::core::types::ProjectAnalysis {
        let mut e = ParserEngine::new();
        e.register(Box::new(SolidityParser::new()));
        e.register(Box::new(RustParser::new()));
        e.register(Box::new(PythonParser::new()));
        for (p, s) in files { e.add_source(*p, *s); }
        e.analyze().unwrap()
    }

    #[test]
    fn entry_points_are_public_and_external() {
        let a = sol_engine(&[("T.sol", r#"
contract T {
    function pubFn() public  {}
    function extFn() external {}
    function intFn() internal {}
    function privFn() private {}
}
"#)]);
        let names: Vec<_> = a.call_chains.iter().map(|c| c.entry_function.as_str()).collect();
        assert!( names.contains(&"pubFn"),  "{names:?}");
        assert!( names.contains(&"extFn"),  "{names:?}");
        assert!(!names.contains(&"intFn"),  "{names:?}");
        assert!(!names.contains(&"privFn"), "{names:?}");
    }

    #[test]
    fn call_chain_traverses_callees() {
        let a = sol_engine(&[("T.sol", r#"
contract T {
    function entry() public { _a(); }
    function _a() internal { _b(); }
    function _b() internal {}
}
"#)]);
        let chain = a.call_chains.iter().find(|c| c.entry_function == "entry").unwrap();
        let names: Vec<_> = chain.nodes.iter().map(|n| n.function_name.as_str()).collect();
        assert!(names.contains(&"entry"), "{names:?}");
        assert!(names.contains(&"_a"),    "{names:?}");
        assert!(names.contains(&"_b"),    "{names:?}");
    }

    #[test]
    fn call_chain_depth_correct() {
        let a = sol_engine(&[("T.sol", r#"
contract T {
    function entry() public { _a(); }
    function _a() internal { _b(); }
    function _b() internal {}
}
"#)]);
        let chain = a.call_chains.iter().find(|c| c.entry_function == "entry").unwrap();
        let depth_of = |name: &str| chain.nodes.iter().find(|n| n.function_name == name).map(|n| n.depth);
        assert_eq!(depth_of("entry"), Some(0));
        assert_eq!(depth_of("_a"),    Some(1));
        assert_eq!(depth_of("_b"),    Some(2));
    }

    #[test]
    fn modifiers_resolved_in_chain() {
        let a = sol_engine(&[("T.sol", r#"
contract T {
    modifier guard() { require(true); _; }
    function entry() public guard {}
}
"#)]);
        let chain = a.call_chains.iter().find(|c| c.entry_function == "entry").unwrap();
        let entry_node = chain.nodes.iter().find(|n| n.function_name == "entry").unwrap();
        assert!(
            entry_node.applied_modifiers.iter().any(|m| m.name == "guard"),
            "modifiers: {:?}", entry_node.applied_modifiers.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cross_file_resolution() {
        let a = sol_engine(&[
            ("Base.sol", r#"
contract Base {
    function _baseHelper() internal {}
}
"#),
            ("Child.sol", r#"
import "./Base.sol";
contract Child {
    function entry() public { _baseHelper(); }
}
"#),
        ]);
        let chain = a.call_chains.iter().find(|c| c.entry_function == "entry").unwrap();
        let names: Vec<_> = chain.nodes.iter().map(|n| n.function_name.as_str()).collect();
        assert!(names.contains(&"_baseHelper"), "cross-file callee missing: {names:?}");
    }

    #[test]
    fn recursion_guard_prevents_loop() {
        let a = sol_engine(&[("T.sol", r#"
contract T {
    function recurse(uint256 n) public returns (uint256) {
        if (n == 0) return 0;
        return recurse(n - 1);
    }
}
"#)]);
        let chain = a.call_chains.iter().find(|c| c.entry_function == "recurse").unwrap();
        let count = chain.nodes.iter().filter(|n| n.function_name == "recurse").count();
        assert_eq!(count, 1, "recursion should appear exactly once");
    }

    #[test]
    fn multi_language_engine_handles_mixed_files() {
        let a = multi_engine(&[
            ("token.sol", "contract T { function mint() public {} }"),
            ("helper.rs", "pub fn process() {}"),
            ("utils.py",  "def compute(): pass\n"),
        ]);
        assert_eq!(a.source_files.len(), 3);
        let langs: Vec<_> = a.source_files.iter().map(|sf| sf.language.as_str()).collect();
        assert!(langs.contains(&"Solidity"), "{langs:?}");
        assert!(langs.contains(&"Rust"),     "{langs:?}");
        assert!(langs.contains(&"Python"),   "{langs:?}");
    }

    #[test]
    fn unknown_extension_produces_warning() {
        let mut e = ParserEngine::new();
        e.register(Box::new(SolidityParser::new()));
        e.add_source("unknown.xyz", "some content");
        let a = e.analyze().unwrap();
        assert!(!a.warnings.is_empty(), "should warn for unknown extension");
    }

    #[test]
    fn all_definitions_keyed_by_file_and_name() {
        let a = sol_engine(&[("T.sol", r#"
contract T {
    function foo() public {}
    function bar() internal {}
}
"#)]);
        assert!(a.lookup("T.sol", "foo").is_some());
        assert!(a.lookup("T.sol", "bar").is_some());
        assert!(a.lookup("T.sol", "nonexistent").is_none());
    }

    #[test]
    fn chains_in_file_filter() {
        let a = sol_engine(&[
            ("A.sol", "contract A { function aFn() public {} }"),
            ("B.sol", "contract B { function bFn() public {} }"),
        ]);
        let a_chains: Vec<_> = a.chains_in_file("A.sol").collect();
        assert_eq!(a_chains.len(), 1);
        assert_eq!(a_chains[0].entry_function, "aFn");
    }

    #[test]
    fn rust_pub_functions_are_entry_points() {
        let mut e = ParserEngine::new();
        e.register(Box::new(RustParser::new()));
        e.add_source("v.rs", r#"
pub fn public_fn() { private_fn(); }
fn private_fn() {}
"#);
        let a = e.analyze().unwrap();
        let names: Vec<_> = a.call_chains.iter().map(|c| c.entry_function.as_str()).collect();
        assert!( names.contains(&"public_fn"),  "{names:?}");
        assert!(!names.contains(&"private_fn"), "{names:?}");
    }

    #[test]
    fn python_public_functions_are_entry_points() {
        let mut e = ParserEngine::new();
        e.register(Box::new(PythonParser::new()));
        e.add_source("m.py", "def public_fn(): _private()\ndef _private(): pass\n");
        let a = e.analyze().unwrap();
        let names: Vec<_> = a.call_chains.iter().map(|c| c.entry_function.as_str()).collect();
        assert!( names.contains(&"public_fn"), "{names:?}");
        assert!(!names.contains(&"_private"),  "{names:?}");
    }
}
