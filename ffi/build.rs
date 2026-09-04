use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let package_name = env::var("CARGO_PKG_NAME").unwrap();
    let output_file = PathBuf::from(&crate_dir).join("include").join(format!("{}.h", package_name));

    // Emit a portable deprecation attribute so C consumers get a compiler
    // warning when they call a deprecated entry point. cbindgen does not emit
    // anything for #[deprecated] unless `deprecated_with_note` is set.
    let mut config = cbindgen::Config {
        after_includes: Some(
            "\n#if defined(__GNUC__) || defined(__clang__)\n\
             #define INT2DDS_DEPRECATED(msg) __attribute__((deprecated(msg)))\n\
             #elif defined(_MSC_VER)\n\
             #define INT2DDS_DEPRECATED(msg) __declspec(deprecated(msg))\n\
             #else\n\
             #define INT2DDS_DEPRECATED(msg)\n\
             #endif"
                .to_string(),
        ),
        ..Default::default()
    };
    config.function.deprecated_with_note = Some("INT2DDS_DEPRECATED({})".to_string());

    // Generate C header file using cbindgen
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(config)
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
Int2DdsRet int2dds_dynamic_sample_get_i16   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int16_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_u16   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint16_t *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_i32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int32_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_u32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint32_t *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_i64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, int64_t  *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_u64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, uint64_t *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_f32   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, float    *out);\n\
Int2DdsRet int2dds_dynamic_sample_get_f64   (const uint8_t *bytes, uintptr_t len, const struct Int2DdsTypeObject *type_obj, const char *field_name, double   *out);\n\
\n\
/* Handle-based DynamicData getters (macro-generated; manually declared). */\n\
Int2DdsRet int2dds_dynamic_data_get_bool (const struct Int2DdsDynamicData *data, const char *field_path, bool     *out);\n\
Int2DdsRet int2dds_dynamic_data_get_i8   (const struct Int2DdsDynamicData *data, const char *field_path, int8_t   *out);\n\
Int2DdsRet int2dds_dynamic_data_get_u8   (const struct Int2DdsDynamicData *data, const char *field_path, uint8_t  *out);\n\
Int2DdsRet int2dds_dynamic_data_get_i16  (const struct Int2DdsDynamicData *data, const char *field_path, int16_t  *out);\n\
Int2DdsRet int2dds_dynamic_data_get_u16  (const struct Int2DdsDynamicData *data, const char *field_path, uint16_t *out);\n\
Int2DdsRet int2dds_dynamic_data_get_i32  (const struct Int2DdsDynamicData *data, const char *field_path, int32_t  *out);\n\
Int2DdsRet int2dds_dynamic_data_get_u32  (const struct Int2DdsDynamicData *data, const char *field_path, uint32_t *out);\n\
Int2DdsRet int2dds_dynamic_data_get_i64  (const struct Int2DdsDynamicData *data, const char *field_path, int64_t  *out);\n\
Int2DdsRet int2dds_dynamic_data_get_u64  (const struct Int2DdsDynamicData *data, const char *field_path, uint64_t *out);\n\
Int2DdsRet int2dds_dynamic_data_get_f32  (const struct Int2DdsDynamicData *data, const char *field_path, float    *out);\n\
Int2DdsRet int2dds_dynamic_data_get_f64  (const struct Int2DdsDynamicData *data, const char *field_path, double   *out);\n\
\n\
/* Handle-based DynamicData primitive setters (macro-generated; manually declared). */\n\
Int2DdsRet int2dds_dynamic_data_set_bool (struct Int2DdsDynamicData *data, const char *field, bool     value);\n\
Int2DdsRet int2dds_dynamic_data_set_i8   (struct Int2DdsDynamicData *data, const char *field, int8_t   value);\n\
Int2DdsRet int2dds_dynamic_data_set_u8   (struct Int2DdsDynamicData *data, const char *field, uint8_t  value);\n\
Int2DdsRet int2dds_dynamic_data_set_i16  (struct Int2DdsDynamicData *data, const char *field, int16_t  value);\n\
Int2DdsRet int2dds_dynamic_data_set_u16  (struct Int2DdsDynamicData *data, const char *field, uint16_t value);\n\
Int2DdsRet int2dds_dynamic_data_set_i32  (struct Int2DdsDynamicData *data, const char *field, int32_t  value);\n\
Int2DdsRet int2dds_dynamic_data_set_u32  (struct Int2DdsDynamicData *data, const char *field, uint32_t value);\n\
Int2DdsRet int2dds_dynamic_data_set_i64  (struct Int2DdsDynamicData *data, const char *field, int64_t  value);\n\
Int2DdsRet int2dds_dynamic_data_set_u64  (struct Int2DdsDynamicData *data, const char *field, uint64_t value);\n\
Int2DdsRet int2dds_dynamic_data_set_f32  (struct Int2DdsDynamicData *data, const char *field, float    value);\n\
Int2DdsRet int2dds_dynamic_data_set_f64  (struct Int2DdsDynamicData *data, const char *field, double   value);\n\
\n\
/* DynamicValue scalar constructors (macro-generated; manually declared). */\n\
Int2DdsRet int2dds_dynamic_value_bool    (bool     value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_i8      (int8_t   value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_i16     (int16_t  value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_i32     (int32_t  value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_i64     (int64_t  value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_u8      (uint8_t  value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_u16     (uint16_t value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_u32     (uint32_t value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_u64     (uint64_t value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_f32     (float    value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_f64     (double   value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_byte    (uint8_t  value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_bitmask (uint64_t value, struct Int2DdsDynamicValue **out);\n\
Int2DdsRet int2dds_dynamic_value_bitset  (uint64_t value, struct Int2DdsDynamicValue **out);\n\
\n\
/* DynamicValue scalar extractors (macro-generated; manually declared). */\n\
Int2DdsRet int2dds_dynamic_value_as_bool (const struct Int2DdsDynamicValue *value, bool     *out);\n\
Int2DdsRet int2dds_dynamic_value_as_i8   (const struct Int2DdsDynamicValue *value, int8_t   *out);\n\
Int2DdsRet int2dds_dynamic_value_as_i16  (const struct Int2DdsDynamicValue *value, int16_t  *out);\n\
Int2DdsRet int2dds_dynamic_value_as_i32  (const struct Int2DdsDynamicValue *value, int32_t  *out);\n\
Int2DdsRet int2dds_dynamic_value_as_i64  (const struct Int2DdsDynamicValue *value, int64_t  *out);\n\
Int2DdsRet int2dds_dynamic_value_as_u8   (const struct Int2DdsDynamicValue *value, uint8_t  *out);\n\
Int2DdsRet int2dds_dynamic_value_as_u16  (const struct Int2DdsDynamicValue *value, uint16_t *out);\n\
Int2DdsRet int2dds_dynamic_value_as_u32  (const struct Int2DdsDynamicValue *value, uint32_t *out);\n\
Int2DdsRet int2dds_dynamic_value_as_u64  (const struct Int2DdsDynamicValue *value, uint64_t *out);\n\
Int2DdsRet int2dds_dynamic_value_as_f32  (const struct Int2DdsDynamicValue *value, float    *out);\n\
Int2DdsRet int2dds_dynamic_value_as_f64  (const struct Int2DdsDynamicValue *value, double   *out);\n";
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

    // Embed an ELF SONAME on the Linux cdylib. rustc does NOT set DT_SONAME for
    // cdylibs, so consumers would otherwise record an unversioned dependency on
    // "libint2dds_ffi.so". We inject the soname via the linker.
    //
    // Version source: Cargo sets CARGO_PKG_VERSION_MAJOR in the build environment
    // from this crate's `version` field in Cargo.toml. We track the MAJOR only
    // (libint2dds_ffi.so.<major>), the standard ELF convention where the soname
    // encodes the ABI-compatibility boundary and changes only on a major bump (a
    // deliberate ABI break).
    //
    // TRADE-OFF: pre-1.0 (0.x) a minor bump can also break ABI, but the soname
    // will NOT change, so linked consumers built against an older 0.x must be
    // rebuilt across such a break. Post-1.0 this matches normal expectations.
    //
    // Gated to Linux/ELF: the MSVC (COFF) linker rejects -Wl,-soname, and the
    // concept does not exist on Windows/macOS.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        let major = env::var("CARGO_PKG_VERSION_MAJOR").unwrap();
        // cdylib output name = package name with '-' -> '_': libint2dds_ffi.so
        let lib_name = package_name.replace('-', "_");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,lib{}.so.{}", lib_name, major);
    }

    // macOS: the SONAME analog is the dylib install name plus compat/current
    // versions. rustc sets none of these for a cdylib, so inject them via the
    // linker. @rpath keeps the install name relocatable for consumers.
    if target_os == "macos" {
        let major = env::var("CARGO_PKG_VERSION_MAJOR").unwrap();
        let minor = env::var("CARGO_PKG_VERSION_MINOR").unwrap();
        let patch = env::var("CARGO_PKG_VERSION_PATCH").unwrap();
        let lib_name = package_name.replace('-', "_");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/lib{}.dylib", lib_name);
        println!("cargo:rustc-cdylib-link-arg=-Wl,-compatibility_version,{}.{}", major, minor);
        println!("cargo:rustc-cdylib-link-arg=-Wl,-current_version,{}.{}.{}", major, minor, patch);
    }

    // Windows: embed a PE VERSIONINFO resource so the DLL reports a FileVersion
    // and ProductVersion (the SONAME analog). winresource reads CARGO_PKG_VERSION
    // for the numeric version fields. cfg(windows) guards it to the Windows host
    // (matching the cfg-gated build-dependency); the resource compiler (rc.exe or
    // windres) only exists there. The DLL filename itself stays unversioned.
    #[cfg(windows)]
    {
        if target_os == "windows" {
            let mut res = winresource::WindowsResource::new();
            res.set("ProductName", "int2dds-ffi");
            res.set("FileDescription", "C FFI bindings for int2DDS");
            res.compile().expect("embed Windows VERSIONINFO resource");
        }
    }

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
