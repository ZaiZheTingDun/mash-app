mod build_support;

fn main() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=../sidecar/cv-code/mash_cv");
    build_support::stage_cv_code(
        &root.join("../sidecar/cv-code/mash_cv"),
        &root.join("resources/mash-cv-code/mash_cv"),
    )
    .expect("stage bundled mash-cv code");
    write_template_inventory();
    tauri_build::build()
}

fn write_template_inventory() {
    use std::{env, fs, path::Path};

    fn collect(root: &Path, directory: &Path, prefix: &str, keys: &mut Vec<String>) {
        for entry in fs::read_dir(directory).expect("read bundled template directory") {
            let path = entry.expect("read bundled template entry").path();
            if path.is_dir() {
                collect(root, &path, prefix, keys);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            {
                let key = path
                    .strip_prefix(root)
                    .unwrap()
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                keys.push(format!("{prefix}/{key}"));
            }
        }
    }

    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest).join("resources/servers");
    println!("cargo:rerun-if-changed=resources/servers");
    let mut keys = Vec::new();
    for server in ["shared", "jp", "cn"] {
        let templates = root.join(server).join("templates");
        collect(&templates, &templates, server, &mut keys);
    }
    keys.sort();
    let output = env::var("OUT_DIR").unwrap();
    fs::write(
        Path::new(&output).join("template-keys.txt"),
        keys.join("\n"),
    )
    .expect("write bundled template inventory");
}
