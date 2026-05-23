use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_header: PathBuf = PathBuf::from(&crate_dir).join("include").join("jgpmf_capi.h");

    if let Some(parent) = out_header.parent() {
        std::fs::create_dir_all(parent).expect("failed to create include/ directory");
    }

    let config = cbindgen::Config::from_file(PathBuf::from(&crate_dir).join("cbindgen.toml"))
        .expect("failed to read cbindgen.toml");

    let generated = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate();

    match generated {
        Ok(bindings) => {
            bindings.write_to_file(&out_header);
        }
        Err(e) => {
            // Don't fail the build if cbindgen can't run (e.g. on docs.rs sandboxes);
            // the checked-in header remains authoritative.
            println!("cargo:warning=cbindgen failed to generate header: {e}");
        }
    }

    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
