use std::process;

use int2dds_idl::codegen;
use int2dds_idl::naming;
use int2dds_idl::parser;
use int2dds_idl::resolver;

struct Args {
    input_file: String,
    rust_output: Option<String>,
    c_output: Option<String>,
    python_output: Option<String>,
    output_dir: Option<String>,
    crate_path: String,
    _python_module: String,
    default_string_bound: u32,
    string_pointer: bool,
    rpc_output: Option<String>,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();

    let mut input_file = None;
    let mut rust_output = None;
    let mut c_output = None;
    let mut python_output = None;
    let mut output_dir = None;
    let mut crate_path = "int2dds".to_string();
    let mut python_module = "int2dds".to_string();
    let mut default_string_bound = 256u32;
    let mut string_pointer = false;
    let mut rpc_output = None;

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
            "-p" | "--python" => {
                i += 1;
                python_output = Some(args.get(i).cloned().unwrap_or_default());
            }
            "-o" | "--output-dir" => {
                i += 1;
                output_dir = Some(args.get(i).cloned().unwrap_or_default());
            }
            "--crate-path" => {
                i += 1;
                crate_path = args.get(i).cloned().unwrap_or_default();
            }
            "--python-module" => {
                i += 1;
                python_module = args.get(i).cloned().unwrap_or_default();
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
            "--rpc" => {
                i += 1;
                rpc_output = Some(args.get(i).cloned().unwrap_or_default());
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
        python_output,
        output_dir,
        crate_path,
        _python_module: python_module,
        default_string_bound,
        string_pointer,
        rpc_output,
    }
}

fn print_usage() {
    eprintln!(
        "Usage: int2dds-idl [OPTIONS] <INPUT.idl>

Generates Rust, C, and Python code from OMG IDL files.

OPTIONS:
    -r, --rust <PATH>       Generate Rust output to PATH
    -c, --c-header <PATH>   Generate C header output to PATH
    -p, --python <PATH>     Generate Python output to PATH
    -o, --output-dir <DIR>  Output directory (auto-names files)
    --crate-path <PATH>     Rust crate path (default: int2dds)
    --python-module <PATH>  Python module path (default: int2dds)
    --string-bound <N>      Default unbounded string size in C (default: 256)
    --string-pointer        Use char* pointers for strings (OMG standard)
    --rpc <PATH>            Generate RPC types (includes base types + RPC infrastructure)
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
    let python_path = args.python_output.or_else(|| {
        args.output_dir
            .as_ref()
            .map(|dir| format!("{}/{}.py", dir, base_name))
    });
    let rpc_path = args.rpc_output.or_else(|| {
        args.output_dir
            .as_ref()
            .map(|dir| format!("{}/{}_rpc.rs", dir, base_name))
    });

    // If no output flags specified, default to generating Rust and C
    let (rust_path, c_path, _python_path, rpc_path) =
        if rust_path.is_none() && c_path.is_none() && python_path.is_none() && rpc_path.is_none()
        {
            (
                Some(format!("{}.rs", base_name)),
                Some(format!("{}.h", base_name)),
                None,
                None,
            )
        } else {
            (rust_path, c_path, python_path, rpc_path)
        };

    // Generate Rust
    if let Some(path) = &rust_path {
        let rust_opts = codegen::rust::RustOptions {
            crate_path: args.crate_path.clone(),
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

    // // Generate Python
    // if let Some(path) = &python_path {
    //     let python_opts = codegen::python::PythonOptions {
    //         int2dds_module: args.python_module.clone(),
    //     };
    //     let code = codegen::python::generate(&model, idl_filename, &python_opts);
    //     if let Err(e) = write_file(path, &code) {
    //         eprintln!("error: cannot write '{}': {}", path, e);
    //         process::exit(1);
    //     }
    //     eprintln!("generated: {}", path);
    // }

    // Generate RPC (base types + RPC infrastructure)
    if let Some(path) = &rpc_path {
        let rust_opts = codegen::rust::RustOptions {
            crate_path: args.crate_path.clone(),
        };
        let rpc_opts = codegen::rpc::RpcOptions {
            crate_path: args.crate_path.clone(),
        };
        let mut code = String::from("#![allow(non_camel_case_types, dead_code, unused_imports, unreachable_patterns, unused_variables)]\n\n");
        code.push_str(&codegen::rust::generate(&model, idl_filename, &rust_opts));
        code.push('\n');
        code.push_str(&codegen::rpc::generate(&model, &rpc_opts));
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
