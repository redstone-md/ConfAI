//! Bakes `presets/*.toml` into the binary.
//!
//! Contributors add a preset by dropping a file in `presets/`; nothing else in
//! the tree needs to change.

use std::fs;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let generated = format!(
        "{}\n{}",
        bake(&root.join("presets"), "BUILTIN_PRESETS", "endpoint presets from `presets/`"),
        bake(&root.join("presets/mcp"), "BUILTIN_MCP_PRESETS", "MCP presets from `presets/mcp/`"),
    );

    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("presets.rs");
    fs::write(&out, generated).unwrap_or_else(|err| panic!("writing {}: {err}", out.display()));
}

/// Emit a `const` holding every `*.toml` directly inside `dir`.
fn bake(dir: &Path, name: &str, doc: &str) -> String {
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut entries: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("reading {}: {err}", dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .map(|path| {
            let escaped = path.to_string_lossy().replace('\\', "\\\\");
            format!("    include_str!(\"{escaped}\"),")
        })
        .collect();
    entries.sort();

    format!(
        "/// Sources baked in at build time: {doc}.\n\
         pub const {name}: [&str; {}] = [\n{}\n];\n",
        entries.len(),
        entries.join("\n")
    )
}
