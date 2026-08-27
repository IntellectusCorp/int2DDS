//! Compiles the generated C with `clang -fsyntax-only` so the "generated code does
//! not compile" defect class is caught by an actual compiler instead of string
//! matching. Mirrors the consumer contract of `ffi/examples`: C99, `int2dds-ffi.h`
//! included before the generated header, headers found via `-I`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use int2dds_idl::codegen::c::{self, COptions, StringMode};
use int2dds_idl::{parser, resolver};

/// One instance of every construct the C backend emits, biased toward the shapes
/// that broke historically: arrays whose element is not a scalar, and extra
/// dimensions on top of them.
const FIXTURE: &str = r#"
    const long ANSWER = 42;

    enum Color { RED, GREEN, BLUE };

    @bit_bound(8)
    bitmask Flags {
        FLAG_A,
        @position(3) FLAG_B
    };

    bitset Bits {
        bitfield<3> low;
        bitfield<5> high;
    };

    struct Inner {
        long a;
        string name;
    };

    struct Base {
        long id;
    };

    struct Derived : Base {
        double extra;
    };

    struct HeapBase {
        string hs;
    };

    struct HeapDerived : HeapBase {
        long hx;
    };

    union Choice switch(long) {
        case 1: long i;
        case 2: string s;
        default: double d;
    };

    @extensibility(MUTABLE)
    struct Tagged {
        @id(10) long a;
        @id(20) string label;
    };

    @extensibility(MUTABLE)
    struct TaggedDerived : Base {
        @id(30) double more;
    };

    @extensibility(MUTABLE)
    struct TaggedPositional {
        long a;
        string label;
    };

    @extensibility(FINAL)
    struct Covered {
        @key long id;
        octet o;
        boolean flag;
        char ch;
        short s16;
        unsigned short u16v;
        long long s64;
        unsigned long long u64v;
        float f32v;
        double d;
        Color c;
        Flags flags2;
        string txt;
        string<32> bounded_txt;
        wstring<16> wtxt;
        Inner inner;
        Derived der;
        TaggedPositional tp;
        long grid[2][3];
        Color palette[2];
        sequence<Color, 4> cseq;
        Inner inner_grid[2];
        string labels[2];
        sequence<long, 8> bseq;
        sequence<long> useq;
        sequence<string> sseq;
        sequence<Inner, 4> iseq;
    };

    @extensibility(APPENDABLE)
    struct Holder {
        @key long id;
        string words[2];
        string<32> bounded;
        wchar wc;
        wstring<16> ws;
        sequence<wstring<16>, 4> wss4;
        sequence<wstring> wss;
        HeapDerived heap_der;
        sequence<long> lists[2];
        Inner inners[2];
        Inner grid2[2][2];
        long grid3[2][3][4];
        Color colors[2][3];
        sequence<Inner> many;
        map<long, string> lookup;
        Choice choice;
        Flags flags;
        Bits bits;
        Derived derived;
        Tagged tagged;
        TaggedDerived tagged_derived;
        @external long ext;
    };

    @extensibility(FINAL)
    struct OptFinalProbe {
        long id;
        @optional long opt_num;
        @optional string opt_text;
        @optional sequence<double> opt_samples;
        @optional Inner opt_inner;
    };

    @extensibility(APPENDABLE)
    struct OptAppendProbe {
        long id;
        @optional string<32> opt_bounded;
        @optional long opt_arr[2];
    };

    @extensibility(MUTABLE)
    struct OptMutProbe {
        @id(5) long id;
        @optional @id(9) string opt_label;
    };
"#;

fn find_clang() -> Option<&'static str> {
    Command::new("clang").arg("--version").output().ok().map(|_| "clang")
}

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("int2dds_idl_csyn_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_generated_c_passes_clang_syntax_check() {
    let Some(clang) = find_clang() else {
        eprintln!("SKIPPED: clang not found on PATH; generated-C compile gate did not run");
        return;
    };

    let defs = parser::parse_idl(FIXTURE).expect("parse fixture");
    let model = resolver::resolve(defs).expect("resolve fixture");
    let ffi_include = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ffi/include");

    for (tag, mode) in [("fixed", StringMode::FixedArray), ("pointer", StringMode::Pointer)] {
        let opts = COptions { default_string_bound: 64, string_mode: mode };
        let code = c::generate(&model, "SyntaxProbe.idl", &opts);

        let dir = unique_dir(tag);
        fs::write(dir.join("syntax_probe.h"), &code).unwrap();
        fs::write(
            dir.join("main.c"),
            "#include \"int2dds-ffi.h\"\n#include \"syntax_probe.h\"\nint main(void) { return 0; }\n",
        )
        .unwrap();

        let output = Command::new(clang)
            .arg("-fsyntax-only")
            .arg("-std=c99")
            .arg("-Werror=implicit-function-declaration")
            .arg("-I")
            .arg(&ffi_include)
            .arg("-I")
            .arg(&dir)
            .arg(dir.join("main.c"))
            .output()
            .expect("run clang");

        assert!(
            output.status.success(),
            "generated C ({} strings) fails to compile:\n{}\n--- generated header ---\n{}",
            tag,
            String::from_utf8_lossy(&output.stderr),
            code
        );

        fs::remove_dir_all(&dir).ok();
    }
}

/// Pointer-mode strings inside a union are heap-allocated by deserialization, so
/// the union must get a discriminator-switch `_cleanup` and the enclosing struct
/// must call it. The clang gate alone cannot catch a regression here: emitting
/// neither the call nor the definition still compiles (and leaks).
#[test]
fn test_pointer_mode_union_string_cleanup() {
    let defs = parser::parse_idl(FIXTURE).expect("parse fixture");
    let model = resolver::resolve(defs).expect("resolve fixture");

    let pointer = c::generate(
        &model,
        "SyntaxProbe.idl",
        &COptions { default_string_bound: 64, string_mode: StringMode::Pointer },
    );
    let expected_cleanup = "static inline void Choice_cleanup(Choice *val) {
    switch (val->_d) {
    case 2:
        if (val->_u.s) { free(val->_u.s); val->_u.s = NULL; }
        break;
    }
}
";
    assert!(
        pointer.contains(expected_cleanup),
        "pointer-mode union cleanup missing or reshaped:\n{}",
        pointer
    );
    assert!(
        pointer.contains("Choice_cleanup(&val->choice);"),
        "enclosing struct cleanup does not free its union member:\n{}",
        pointer
    );

    let fixed = c::generate(
        &model,
        "SyntaxProbe.idl",
        &COptions { default_string_bound: 64, string_mode: StringMode::FixedArray },
    );
    assert!(!fixed.contains("Choice_cleanup"), "fixed-array mode must not emit union cleanup");
}
