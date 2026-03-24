// build.rs
//
// The tree-sitter-* grammar crates ship their own build scripts that compile
// the C grammar files.  This build.rs exists mainly to document that fact and
// to give us a hook if we ever need custom C compilation flags.

fn main() {
    // Nothing extra needed — each grammar crate handles its own cc compilation.
    // Re-run only if this file changes.
    println!("cargo:rerun-if-changed=build.rs");
}
