use std::process;

use int2dds_idl::codegen;
use int2dds_idl::naming;
use int2dds_idl::parser;
use int2dds_idl::resolver;
use int2dds_idl::types::ExtensibilityKind;

struct Args {
    input_file: String,
    rust_output: Option<String>,
    c_output: Option<String>,
    output_dir: Option<String>,
    crate_path: String,
    default_extensibility: Option<ExtensibilityKind>,
    default_string_bound: u32,
    string_pointer: bool,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();

    let mut input_file = None;
    let mut rust_output = None;
    let mut c_output = None;
    let mut output_dir = None;
    let mut crate_path = "int2dds".to_string();
    let mut default_extensibility: Option<ExtensibilityKind> = None;
    let mut default_string_bound = 256u32;
    let mut string_pointer = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-r" | "--rust" => {
                i += 1;
                rust_output = Some(args.get(i).cloned().unwrap_or_default());
            }
            "-c" | "--c-header" => {
                i += 1;
                c_output = Some(args.get(i).cloned().unwrap_or_default());
            }
            "-o" | "--output-dir" => {
                i += 1;
                output_dir = Some(args.get(i).cloned().unwrap_or_default());
            }
            "--crate-path" => {
                i += 1;
                crate_path = args.get(i).cloned().unwrap_or_default();
            }
            "--extensibility" => {
                i += 1;
                let value = args.get(i).map(|s| s.as_str()).unwrap_or("");
                default_extensibility = match value.to_uppercase().as_str() {
                    "FINAL" => Some(ExtensibilityKind::Final),
                    "APPENDABLE" => Some(ExtensibilityKind::Appendable),
                    "MUTABLE" => Some(ExtensibilityKind::Mutable),
                    _ => {
                        eprintln!("error: invalid extensibility '{}' (use: final, appendable, mutable)", value);
                        process::exit(1);
                    }
                };
            }
            "--string-bound" => {
                i += 1;
                default_string_bound = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(256);
            }
            "--string-pointer" => {
                string_pointer = true;
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            "-V" | "--version" => {
                println!("int2dds-idl {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            arg if arg.starts_with('-') => {
                eprintln!("unknown option: {}", arg);
                process::exit(1);
            }
            _ => {
                input_file = Some(args[i].clone());
            }
        }
        i += 1;
    }

    let input_file = match input_file {
        Some(f) => f,
        None => {
            eprintln!("error: no input file specified");
            print_usage();
            process::exit(1);
        }
    };

    Args {
        input_file,
        rust_output,
        c_output,
        output_dir,
        crate_path,
        default_extensibility,
        default_string_bound,
        string_pointer,
    }
}

fn print_usage() {
    eprintln!(
        "Usage: int2dds-idl [OPTIONS] <INPUT.idl>

Generates Rust and C code from OMG IDL files.

OPTIONS:
    -r, --rust <PATH>       Generate Rust output to PATH
    -c, --c-header <PATH>   Generate C header output to PATH
    -o, --output-dir <DIR>  Output directory (auto-names files)
    --crate-path <PATH>     Rust crate path (default: int2dds)
    --extensibility <TYPE>  Default extensibility for types without @extensibility
                            (final, appendable, mutable). IDL annotations take priority.
    --string-bound <N>      Default unbounded string size in C (default: 256)
    --string-pointer        Use char* pointers for strings (OMG standard)
    -h, --help              Print help
    -V, --version           Print version"
    );
}

fn main() {
    let args = parse_args();

    // Read input
    let source = match std::fs::read_to_string(&args.input_file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", args.input_file, e);
            process::exit(1);
        }
    };

    // Parse
    let definitions = match parser::parse_idl(&source) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}:{}", args.input_file, e);
            process::exit(1);
        }
    };

    // Resolve
    let model = match resolver::resolve(definitions) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {}", args.input_file, e);
            process::exit(1);
        }
    };

    let idl_filename = args
        .input_file
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&args.input_file);
    let base_name = naming::idl_to_output_name(idl_filename);

    // Determine output paths
    let rust_path = args.rust_output.or_else(|| {
        args.output_dir
            .as_ref()
            .map(|dir| format!("{}/{}.rs", dir, base_name))
    });
    let c_path = args.c_output.or_else(|| {
        args.output_dir
            .as_ref()
            .map(|dir| format!("{}/{}.h", dir, base_name))
    });

    // If neither -r, -c, nor -o specified, default to generating both
    let (rust_path, c_path) = if rust_path.is_none() && c_path.is_none() {
        (
            Some(format!("{}.rs", base_name)),
            Some(format!("{}.h", base_name)),
        )
    } else {
        (rust_path, c_path)
    };

    // Generate Rust
    if let Some(path) = &rust_path {
        let rust_opts = codegen::rust::RustOptions {
            crate_path: args.crate_path.clone(),
            default_extensibility: args.default_extensibility,
        };
        let code = codegen::rust::generate(&model, idl_filename, &rust_opts);
        if let Err(e) = write_file(path, &code) {
            eprintln!("error: cannot write '{}': {}", path, e);
            process::exit(1);
        }
        eprintln!("generated: {}", path);
    }

    // Generate C
    if let Some(path) = &c_path {
        let c_opts = codegen::c::COptions {
            default_string_bound: args.default_string_bound,
            string_mode: if args.string_pointer {
                codegen::c::StringMode::Pointer
            } else {
                codegen::c::StringMode::FixedArray
            },
        };
        let code = codegen::c::generate(&model, idl_filename, &c_opts);
        if let Err(e) = write_file(path, &code) {
            eprintln!("error: cannot write '{}': {}", path, e);
            process::exit(1);
        }
        eprintln!("generated: {}", path);
    }
}

/// Write file, creating parent directories if needed
fn write_file(path: &str, content: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
}
