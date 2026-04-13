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

    // Append manually-declared dynamic primitive sample getters that cbindgen
    // cannot see because they are produced by a macro in dynamic.rs.
    let extra = "\n\
/* Dynamic primitive sample getters (macro-generated in dynamic.rs;\n\
 * manually declared here because cbindgen does not expand macros). */\n\
Int2DdsRet int2dds_dynamic_sample_get_bool  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, bool     *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_i8    (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int8_t   *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_u8    (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint8_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_byte  (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint8_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_char8 (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint8_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_i16   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int16_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_u16   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint16_t *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_i32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int32_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_u32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint32_t *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_i64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int64_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_u64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint64_t *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_f32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, float    *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_f64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, double   *out);\n";
    let contents = std::fs::read_to_string(&output_file).expect("read header");
    if !contents.contains("int2dds_dynamic_sample_get_i32") {
        // Insert before the closing #endif of include guard
        let new_contents = if let Some(idx) = contents.rfind("#endif") {
            let (head, tail) = contents.split_at(idx);
            format!("{}{}\n{}", head, extra, tail)
        } else {
            format!("{}{}", contents, extra)
        };
        std::fs::write(&output_file, new_contents).expect("write header");
    }

    println!("cargo:rerun-if-changed=src");
}
