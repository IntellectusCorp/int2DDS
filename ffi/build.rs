use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let output_file = PathBuf::from(&crate_dir).join("include").join(format!("{}.h", package_name));

    // Generate C header file using cbindgen
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("INT2DDS_FFI_H")
        .with_documentation(true)
        .with_cpp_compat(true)
        .with_pragma_once(true)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(&output_file);

    println!("cargo:rerun-if-changed=src");
}
