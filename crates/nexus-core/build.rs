//! Embeds the workspace starter templates (`templates/<id>/…`, ADR-0037) into the library: every file becomes an
//! `include_str!` entry keyed by its path relative to `templates/`, so NEXUS can install them offline and the content
//! is versioned and reviewed with the code.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).map(|rd| rd.flatten().collect()).unwrap_or_default();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("templates");
    println!("cargo:rerun-if-changed=templates");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut code = String::from("/// Embedded template files: (path relative to `templates/`, UTF-8 content).\npub(crate) static FILES: &[(&str, &str)] = &[\n");
    for f in files {
        let rel = f.strip_prefix(&root).expect("inside templates").to_string_lossy().replace('\\', "/");
        let abs = f.to_string_lossy().replace('\\', "/");
        code.push_str(&format!("    ({rel:?}, include_str!({abs:?})),\n"));
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("templates_embedded.rs");
    std::fs::write(out, code).expect("write templates_embedded.rs");
}
