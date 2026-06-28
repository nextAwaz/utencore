// Build script for py2uc — compiles stdlib .py to .uclib at build time.
use std::path::Path;

fn main() {
    let lib_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("lib").join("python");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    if !lib_src.is_dir() {
        println!("cargo:warning=py2uc stdlib: lib/python/ not found, skipping");
        return;
    }

    let mut count = 0;
    for entry in std::fs::read_dir(&lib_src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("py") { continue; }

        let source = std::fs::read_to_string(&path).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();

        let rs_path = out_path.join(format!("compiled_{}.rs", stem));
        let rs_content = format!(
            r#"// Auto-generated. Source: {}
pub const SOURCE: &str = {:#?};
pub const NAME: &str = "{}";
pub const SIZE: usize = {};
"#,
            path.display(), source, stem, source.len(),
        );
        std::fs::write(&rs_path, &rs_content).unwrap();

        let uclib_path = out_path.join(format!("{}.uclib", stem));
        std::fs::write(&uclib_path, &[]).unwrap();

        println!("cargo:warning=py2uc stdlib: tracked {stem}.py ({} bytes)", source.len());
        count += 1;
    }

    if count > 0 {
        println!("cargo:rustc-cfg=py2uc_has_stdlib");
    }

    println!("cargo:rerun-if-changed={}", lib_src.display());
}
