//! Embeds the dashboard (`frontend/`) into the binary: generates
//! `$OUT_DIR/frontend_files.rs`, a table of `(path, bytes, etag)` that
//! `src/frontend.rs` serves. Only what the page loads is embedded; the
//! Tailwind input, the vendor lock and READMEs are left out.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

/// Files under `frontend/` that the page never requests.
fn skip(rel: &str) -> bool {
    rel == "tailwind.css" || rel == "vendor.lock" || rel.ends_with(".md")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read frontend dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// FNV-1a, so an unchanged file keeps its ETag across builds.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend");
    // A directory makes Cargo rerun on any change inside it.
    println!("cargo:rerun-if-changed=frontend");

    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();

    let mut table = String::from("pub static FILES: &[(&str, &[u8], &str)] = &[\n");
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .expect("under frontend/")
            .to_string_lossy()
            .replace('\\', "/");
        if skip(&rel) {
            continue;
        }
        let etag = format!("\"{:016x}\"", fnv(&fs::read(&path).expect("read file")));
        writeln!(
            table,
            "    ({rel:?}, include_bytes!({:?}), {etag:?}),",
            path.display()
        )
        .unwrap();
    }
    table.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("frontend_files.rs");
    fs::write(out, table).expect("write file table");
}
