use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process;

use int2dds_idl::codegen;
use int2dds_idl::naming;
use int2dds_idl::parser;
use int2dds_idl::preprocess;
use int2dds_idl::resolver;

struct Args {
    input_files: Vec<String>,
    rust_output: Option<String>,
    c_output: Option<String>,
    python_output: Option<String>,
    csharp_output: Option<String>,
    xml_output: Option<String>,
    output_dir: Option<String>,
    crate_path: String,
    python_module: String,
    csharp_namespace: String,
    default_string_bound: u32,
    string_pointer: bool,
    rpc_output: Option<String>,
    ros2: bool,
    ros2_package: Option<String>,
    ros2_kind: Option<String>,
    include_dirs: Vec<PathBuf>,
    java_output: Option<String>,
    java_package: Option<String>,
    // Restrict auto-naming to these languages (rust, c, python, csharp, xml, rpc, java).
    // None = unrestricted (batch -o emits every language).
    langs: Option<[bool; 7]>,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();

    let mut input_files: Vec<String> = Vec::new();
    let mut rust_output = None;
    let mut c_output = None;
    let mut python_output = None;
    let mut csharp_output = None;
    let mut xml_output = None;
    let mut output_dir = None;
    let mut crate_path = "int2dds".to_string();
    let mut python_module = "int2dds".to_string();
    let mut csharp_namespace = "GeneratedTypes".to_string();
    let mut default_string_bound = 256u32;
    let mut string_pointer = false;
    let mut rpc_output = None;
    let mut ros2 = false;
    let mut ros2_package = None;
    let mut ros2_kind = None;
    let mut include_dirs = Vec::new();
    let mut java_output = None;
    let mut java_package: Option<String> = None;

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
            "-s" | "--csharp" => {
                i += 1;
                csharp_output = Some(args.get(i).cloned().unwrap_or_default());
            }
            "-j" | "--java" => {
                i += 1;
                java_output = Some(args.get(i).cloned().unwrap_or_default());
            }
            "--java-package" => {
                i += 1;
                java_package = args.get(i).cloned();
            }
            "-x" | "--xml" => {
                i += 1;
                xml_output = Some(args.get(i).cloned().unwrap_or_default());
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
            "--csharp-namespace" => {
                i += 1;
                csharp_namespace = args.get(i).cloned().unwrap_or_default();
            }
            "--string-bound" => {
                i += 1;
                default_string_bound = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(256);
            }
            "--string-pointer" => {
                string_pointer = true;
            }
            "--rpc" => {
                i += 1;
                rpc_output = Some(args.get(i).cloned().unwrap_or_default());
            }
            "--ros2" => {
                ros2 = true;
            }
            "--ros2-package" => {
                i += 1;
                ros2_package = args.get(i).cloned();
            }
            "--ros2-kind" => {
                i += 1;
                ros2_kind = args.get(i).cloned();
            }
            "-I" | "--include" => {
                i += 1;
                if let Some(dir) = args.get(i) {
                    include_dirs.push(PathBuf::from(dir));
                }
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
                input_files.push(args[i].clone());
            }
        }
        i += 1;
    }

    if input_files.is_empty() {
        eprintln!("error: no input file specified");
        print_usage();
        process::exit(1);
    }

    Args {
        input_files,
        rust_output,
        c_output,
        python_output,
        csharp_output,
        xml_output,
        output_dir,
        crate_path,
        python_module,
        csharp_namespace,
        default_string_bound,
        string_pointer,
        rpc_output,
        ros2,
        ros2_package,
        ros2_kind,
        include_dirs,
        java_output,
        java_package,
        langs: None,
    }
}

fn print_usage() {
    eprintln!(
        "Usage: int2dds-idl [OPTIONS] <INPUT.idl>...

Generates Rust, C, Python, C#, and Java code from OMG IDL files.

OPTIONS:
    -r, --rust <PATH>         Generate Rust output to PATH
    -c, --c-header <PATH>     Generate C header output to PATH
    -p, --python <PATH>       Generate Python output to PATH
    -s, --csharp <PATH>       Generate C# output to PATH
    -j, --java <DIR>          Generate Java output under DIR (a directory: one
                              file per type)
    -x, --xml <PATH>          Generate XML type representation to PATH
    -o, --output-dir <DIR>    Output directory (auto-names files)
    -I, --include <DIR>       Add a search directory for #include resolution (repeatable)
    --crate-path <PATH>       Rust crate path (default: int2dds)
    --python-module <PATH>    Python module path (default: int2dds)
    --csharp-namespace <NS>   C# namespace (default: GeneratedTypes)
    --java-package <PKG>      Java package (default: none -- unnamed package,
                              files land flat in DIR)
    --string-bound <N>        Default unbounded string size in C (default: 256)
    --string-pointer          Use char* pointers for strings (OMG standard)
    --rpc <PATH>            Generate RPC types (includes base types + RPC infrastructure)
    --ros2                    Use ROS2-compatible DDS type naming (scope::dds_::Name_);
                              flat IDL infers the package from a <package>/msg/File.idl path
    --ros2-package <NAME>     Override the package for flat (module-less) IDL under --ros2
    --ros2-kind <KIND>        Interface kind (msg|srv|action) for --ros2-package (default: msg)
    -h, --help                Print help
    -V, --version             Print version"
    );
}

/// Abort the wizard (e.g. on Ctrl-C / ESC).
fn wizard_cancelled() -> ! {
    eprintln!("Cancelled.");
    process::exit(1);
}

/// Shell-style path Tab-completion: each Tab cycles to the next candidate.
#[derive(Default)]
struct PathCompletion {
    state: std::cell::RefCell<CycleState>,
}

#[derive(Default)]
struct CycleState {
    last_returned: Option<String>,
    candidates: Vec<String>,
    idx: usize,
}

impl dialoguer::Completion for PathCompletion {
    fn get(&self, input: &str) -> Option<String> {
        let mut st = self.state.borrow_mut();

        // Same buffer as last suggestion: cycle to the next sibling. A single
        // candidate falls through so the next Tab descends into it.
        if st.last_returned.as_deref() == Some(input) && st.candidates.len() > 1 {
            st.idx = (st.idx + 1) % st.candidates.len();
            let next = st.candidates[st.idx].clone();
            st.last_returned = Some(next.clone());
            return Some(next);
        }

        let candidates = path_candidates(input);
        if candidates.is_empty() {
            st.last_returned = None;
            st.candidates.clear();
            return None;
        }
        let first = candidates[0].clone();
        st.candidates = candidates;
        st.idx = 0;
        st.last_returned = Some(first.clone());
        Some(first)
    }
}

/// Sorted full paths matching `input` (directories get a trailing separator).
fn path_candidates(input: &str) -> Vec<String> {
    let (dir, prefix) = match input.rfind(['/', '\\']) {
        Some(idx) => (&input[..=idx], &input[idx + 1..]),
        None => ("", input),
    };
    let sep = if input.contains('\\') { '\\' } else { '/' };
    let search_dir = if dir.is_empty() { Path::new(".") } else { Path::new(dir) };

    let mut matches: Vec<String> = match std::fs::read_dir(search_dir) {
        Ok(rd) => rd
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(prefix) {
                    let suffix =
                        if entry.path().is_dir() { sep.to_string() } else { String::new() };
                    Some(format!("{}{}{}", dir, name, suffix))
                } else {
                    None
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    matches.sort();
    matches
}

/// Find `.idl` files under the cwd (bounded depth) for the wizard's pick list.
fn discover_idl_files() -> Vec<String> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<String>) {
        if depth > 4 {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if matches!(name, "target" | ".git" | "node_modules" | ".cargo") {
                    continue;
                }
                walk(&path, depth + 1, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("idl") {
                if let Some(s) = path.to_str() {
                    out.push(s.replace('\\', "/"));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new("."), 0, &mut out);
    out.sort();
    out
}

/// Interactive prompts for input, languages, output dir, and advanced settings.
fn run_wizard() -> Args {
    use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};

    let theme = ColorfulTheme::default();
    eprintln!("int2dds-idl interactive code generation wizard\n");

    let completion = PathCompletion::default();

    // 1) IDL input files (one or more). Manual entry loops until a blank line.
    let prompt_paths = || -> Vec<String> {
        let mut paths = Vec::new();
        loop {
            let p: String = Input::<String>::with_theme(&theme)
                .with_prompt(if paths.is_empty() {
                    "IDL file path (Tab to autocomplete)"
                } else {
                    "Additional IDL file path (Enter to finish)"
                })
                .completion_with(&completion)
                .allow_empty(true)
                .validate_with(|s: &String| {
                    let t = s.trim();
                    if t.is_empty() || Path::new(t).is_file() {
                        Ok(())
                    } else {
                        Err("file not found")
                    }
                })
                .interact_text()
                .unwrap_or_else(|_| wizard_cancelled());
            let t = p.trim().to_string();
            if t.is_empty() {
                if paths.is_empty() {
                    continue;
                }
                break;
            }
            paths.push(t);
        }
        paths
    };

    let input_files = {
        let discovered = discover_idl_files();
        if discovered.is_empty() {
            prompt_paths()
        } else {
            let mut items = discovered.clone();
            items.push("Enter path manually…".to_string());
            let manual_idx = items.len() - 1;
            let sel = MultiSelect::with_theme(&theme)
                .with_prompt("Select IDL files (space to toggle, enter to confirm)")
                .items(&items)
                .interact()
                .unwrap_or_else(|_| wizard_cancelled());
            let mut files: Vec<String> =
                sel.iter().filter(|&&i| i != manual_idx).map(|&i| discovered[i].clone()).collect();
            if sel.contains(&manual_idx) {
                files.extend(prompt_paths());
            }
            if files.is_empty() {
                files = prompt_paths();
            }
            files
        }
    };

    // 2) Target languages (checkbox multi-select)
    let lang_items = ["Rust", "C", "Python", "C#", "XML", "RPC", "Java"];
    let lang_defaults = [true, false, false, false, false, false, false];
    let chosen = MultiSelect::with_theme(&theme)
        .with_prompt("Languages to generate (↑↓ move, space to toggle, enter to confirm)")
        .items(&lang_items)
        .defaults(&lang_defaults)
        .interact()
        .unwrap_or_else(|_| wizard_cancelled());
    if chosen.is_empty() {
        eprintln!("No language selected.");
        process::exit(1);
    }
    let want = |i: usize| chosen.contains(&i);
    let (gen_rust, gen_c, gen_python, gen_csharp, gen_xml, gen_rpc, gen_java) =
        (want(0), want(1), want(2), want(3), want(4), want(5), want(6));

    // 3) Output directory
    let output_dir: String = Input::with_theme(&theme)
        .with_prompt("Output directory (Tab to autocomplete)")
        .completion_with(&completion)
        .default("generated".to_string())
        .interact_text()
        .unwrap_or_else(|_| wizard_cancelled());

    // 4) Optional advanced settings
    let mut crate_path = "int2dds".to_string();
    let mut python_module = "int2dds".to_string();
    let mut csharp_namespace = "GeneratedTypes".to_string();
    let mut default_string_bound = 256u32;
    let mut string_pointer = false;
    let mut ros2 = false;
    let mut ros2_package = None;
    let mut ros2_kind = None;
    let mut include_dirs = Vec::new();
    let mut java_package: Option<String> = None;

    let advanced = Confirm::with_theme(&theme)
        .with_prompt("Configure advanced options?")
        .default(false)
        .interact()
        .unwrap_or(false);

    if advanced {
        ros2 = Confirm::with_theme(&theme)
            .with_prompt("Use ROS2-compatible type naming?")
            .default(false)
            .interact()
            .unwrap_or(false);
        if ros2 {
            let pkg: String = Input::with_theme(&theme)
                .with_prompt("ROS2 package name (leave empty to infer from file path)")
                .allow_empty(true)
                .default(String::new())
                .interact_text()
                .unwrap_or_else(|_| wizard_cancelled());
            if !pkg.trim().is_empty() {
                ros2_package = Some(pkg.trim().to_string());
                let kinds = ["msg", "srv", "action"];
                let k = Select::with_theme(&theme)
                    .with_prompt("Interface kind")
                    .items(&kinds)
                    .default(0)
                    .interact()
                    .unwrap_or_else(|_| wizard_cancelled());
                ros2_kind = Some(kinds[k].to_string());
            }
        }

        if gen_rust || gen_rpc {
            crate_path = Input::with_theme(&theme)
                .with_prompt("Rust crate-path")
                .default(crate_path)
                .interact_text()
                .unwrap_or_else(|_| wizard_cancelled());
        }
        if gen_python {
            python_module = Input::with_theme(&theme)
                .with_prompt("Python module path")
                .default(python_module)
                .interact_text()
                .unwrap_or_else(|_| wizard_cancelled());
        }
        if gen_csharp {
            csharp_namespace = Input::with_theme(&theme)
                .with_prompt("C# namespace")
                .default(csharp_namespace)
                .interact_text()
                .unwrap_or_else(|_| wizard_cancelled());
        }
        if gen_java {
            let pkg: String = Input::with_theme(&theme)
                .with_prompt("Java package (leave empty for the unnamed package)")
                .allow_empty(true)
                .default(String::new())
                .interact_text()
                .unwrap_or_else(|_| wizard_cancelled());
            let pkg = pkg.trim().to_string();
            java_package = if pkg.is_empty() { None } else { Some(pkg) };
        }
        if gen_c {
            string_pointer = Confirm::with_theme(&theme)
                .with_prompt("Generate C strings as char* pointers?")
                .default(false)
                .interact()
                .unwrap_or(false);
            default_string_bound = Input::with_theme(&theme)
                .with_prompt("Default C string size")
                .default(256u32)
                .interact_text()
                .unwrap_or_else(|_| wizard_cancelled());
        }

        let inc: String = Input::with_theme(&theme)
            .with_prompt("#include search paths (comma-separated, leave empty if none)")
            .allow_empty(true)
            .default(String::new())
            .interact_text()
            .unwrap_or_else(|_| wizard_cancelled());
        for d in inc.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            include_dirs.push(PathBuf::from(d));
        }
    }

    // Each selected language is auto-named per input file under the output dir.
    let dir = output_dir.trim().trim_end_matches(['/', '\\']).to_string();

    eprintln!();
    Args {
        input_files,
        rust_output: None,
        c_output: None,
        python_output: None,
        csharp_output: None,
        xml_output: None,
        output_dir: Some(dir),
        crate_path,
        python_module,
        csharp_namespace,
        default_string_bound,
        string_pointer,
        rpc_output: None,
        ros2,
        ros2_package,
        ros2_kind,
        include_dirs,
        java_output: None,
        java_package,
        langs: Some([gen_rust, gen_c, gen_python, gen_csharp, gen_xml, gen_rpc, gen_java]),
    }
}

fn main() {
    // No arguments -> interactive wizard; any argument -> batch mode.
    let args = if std::env::args().len() <= 1 { run_wizard() } else { parse_args() };

    if let Some(kind) = &args.ros2_kind {
        if !matches!(kind.as_str(), "msg" | "srv" | "action") {
            eprintln!("error: --ros2-kind must be one of msg|srv|action (got '{}')", kind);
            process::exit(1);
        }
    }

    // 잘못된 --java-package 는 입력 파일과 무관한 인자 오류다. 배치 모드가
    // 파일마다 경고하고 넘기는 백엔드 거절과 달리 여기서 바로 멈춘다.
    if args.java_package.is_some() {
        let opts = codegen::java::JavaOptions { package: args.java_package.clone() };
        if let Err(e) = codegen::java::validate_package(&opts) {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }

    // Java 는 타입 이름으로 파일 이름을 정하므로 입력 파일이 달라도 경로가
    // 겹칠 수 있다. 어느 입력이 먼저 썼는지 기억해 두고 덮어쓸 때 알린다.
    let mut java_written: HashMap<String, String> = HashMap::new();
    for input_file in &args.input_files {
        process_file(&args, input_file, &mut java_written);
    }
}

/// The Rust+C default fires only when no language flag and no -o resolved a path.
#[allow(clippy::too_many_arguments)]
fn wants_default_rust_c(
    rust: &Option<String>,
    c: &Option<String>,
    python: &Option<String>,
    rpc: &Option<String>,
    csharp: &Option<String>,
    xml: &Option<String>,
    java: &Option<String>,
) -> bool {
    rust.is_none()
        && c.is_none()
        && python.is_none()
        && rpc.is_none()
        && csharp.is_none()
        && xml.is_none()
        && java.is_none()
}

/// True when the user asked for Java by name: `-j`, or the wizard's Java
/// checkbox (which sets `langs`). `-o` alone only implies Java, so a backend
/// refusal there is a warning, not a failed run.
fn java_requested_explicitly(java_output: &Option<String>, langs: &Option<[bool; 7]>) -> bool {
    java_output.is_some() || langs.is_some_and(|l| l[6])
}

/// Records `path` as written by `input_file`, returning the earlier input file
/// when this run already produced that same Java path.
fn note_java_output(
    written: &mut HashMap<String, String>,
    path: &str,
    input_file: &str,
) -> Option<String> {
    written.insert(path.to_string(), input_file.to_string()).filter(|prev| prev != input_file)
}

/// Generate the selected outputs for a single input IDL file.
fn process_file(args: &Args, input_file: &str, java_written: &mut HashMap<String, String>) {
    // Read input, resolving #include directives into a single translation unit.
    let (source, loaded) = match preprocess::load_with_includes_ex(
        std::path::Path::new(input_file),
        &args.include_dirs,
    ) {
        Ok((s, missing, loaded)) => {
            for inc in &missing {
                eprintln!("warning: could not resolve #include \"{}\" (use -I <dir>)", inc);
            }
            (s, loaded)
        }
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", input_file, e);
            process::exit(1);
        }
    };

    // Parse the full translation unit (root + includes) and, separately, the root
    // file alone. #included types resolve against the full unit but only the root
    // file's own types/constants are emitted (resolve-only includes).
    let parse = |src: &str| match parser::parse_idl(src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}:{}", input_file, e);
            process::exit(1);
        }
    };
    let all_defs = parse(&source);
    let root_src = std::fs::read_to_string(input_file).unwrap_or_else(|_| source.clone());
    let root_defs = parse(&root_src);

    // Resolve
    let mut model = match resolver::resolve_scoped(&root_defs, all_defs) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {}", input_file, e);
            process::exit(1);
        }
    };

    // Map each #included type to the output module its own file is emitted into,
    // so codegen can emit a cross-file import (`from <module> import <Leaf>`).
    // Naming mirrors the per-file auto-naming below (idl_to_output_name); this
    // assumes every included file is also generated as a sibling module.
    model.imported.modules = build_import_modules(input_file, &loaded, &args.include_dirs);

    // Apply ROS2-compatible type naming : rewrite
    // every registered DDS type name to scope::dds_::Name_. Generated struct/field
    // code is untouched; only the registered/TypeObject name changes.
    if args.ros2 {
        let flat_scope = naming::ros2_flat_scope(
            input_file,
            args.ros2_package.as_deref(),
            args.ros2_kind.as_deref(),
        );
        let has_flat = model.qualified_names().any(|q| !q.contains("::"));
        if has_flat && flat_scope.is_none() {
            eprintln!(
                "warning: --ros2 on module-less IDL without a package; type names left unmangled.\n\
                 \x20        provide --ros2-package <NAME> or place the file at <package>/msg/<File>.idl"
            );
        }
        model.map_qualified_names(|q| naming::ros2_type_name(q, flat_scope.as_deref()));
    }

    let idl_filename = input_file.rsplit(['/', '\\']).next().unwrap_or(input_file);
    let base_name = naming::idl_to_output_name(idl_filename);

    // Auto-name into output_dir for languages allowed by `langs` (None = all).
    let allowed = |i: usize| args.langs.map_or(true, |s| s[i]);
    let auto = |ext: &str, i: usize| {
        args.output_dir
            .as_ref()
            .filter(|_| allowed(i))
            .map(|dir| format!("{}/{}.{}", dir, base_name, ext))
    };

    // Determine output paths
    let rust_path = args.rust_output.clone().or_else(|| auto("rs", 0));
    let c_path = args.c_output.clone().or_else(|| auto("h", 1));
    let python_path = args.python_output.clone().or_else(|| auto("py", 2));
    let rpc_path = args.rpc_output.clone().or_else(|| {
        args.output_dir
            .as_ref()
            .filter(|_| allowed(5))
            .map(|dir| format!("{}/{}_rpc.rs", dir, base_name))
    });
    let csharp_path = args.csharp_output.clone().or_else(|| {
        args.output_dir
            .as_ref()
            .filter(|_| allowed(3))
            .map(|dir| format!("{}/{}.cs", dir, naming::to_pascal_case(&base_name)))
    });
    let xml_path = args.xml_output.clone().or_else(|| auto("xml", 4));

    // Java takes a directory, not a file: the backend returns one file per type.
    let java_dir = args
        .java_output
        .clone()
        .or_else(|| args.output_dir.as_ref().filter(|_| allowed(6)).cloned());

    // If no output flag and no -o are given, default to generating Rust and C.
    let (rust_path, c_path, python_path, rpc_path, csharp_path, xml_path) = if wants_default_rust_c(
        &rust_path,
        &c_path,
        &python_path,
        &rpc_path,
        &csharp_path,
        &xml_path,
        &java_dir,
    ) {
        (
            Some(format!("{}.rs", base_name)),
            Some(format!("{}.h", base_name)),
            None,
            None,
            None,
            None,
        )
    } else {
        (rust_path, c_path, python_path, rpc_path, csharp_path, xml_path)
    };

    // Generate Rust
    if let Some(path) = &rust_path {
        let rust_opts = codegen::rust::RustOptions { crate_path: args.crate_path.clone() };
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

    // Generate Python
    if let Some(path) = &python_path {
        let python_opts =
            codegen::python::PythonOptions { int2dds_module: args.python_module.clone() };
        let code = codegen::python::generate(&model, idl_filename, &python_opts);
        if let Err(e) = write_file(path, &code) {
            eprintln!("error: cannot write '{}': {}", path, e);
            process::exit(1);
        }
        eprintln!("generated: {}", path);
    }

    // Generate RPC (base types + RPC infrastructure)
    if let Some(path) = &rpc_path {
        let rust_opts = codegen::rust::RustOptions { crate_path: args.crate_path.clone() };
        let rpc_opts = codegen::rpc::RpcOptions { crate_path: args.crate_path.clone() };
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

    // Generate C#
    if let Some(path) = &csharp_path {
        let csharp_opts =
            codegen::csharp::CSharpOptions { namespace: args.csharp_namespace.clone() };
        let code = codegen::csharp::generate(&model, idl_filename, &csharp_opts);
        if let Err(e) = write_file(path, &code) {
            eprintln!("error: cannot write '{}': {}", path, e);
            process::exit(1);
        }
        eprintln!("generated: {}", path);
    }

    // Generate Java
    if let Some(dir) = &java_dir {
        let java_opts = codegen::java::JavaOptions { package: args.java_package.clone() };
        let generated = match codegen::java::generate(&model, idl_filename, &java_opts) {
            Ok(f) => Some(f),
            // -o 배치는 Java 를 암시할 뿐이다. 다른 언어는 다 처리하는 구성요소
            // 하나 때문에 배치 전체를 죽이지 않는다.
            Err(e) if !java_requested_explicitly(&args.java_output, &args.langs) => {
                eprintln!("warning: skipping Java for {}: {}", input_file, e);
                None
            }
            Err(e) => {
                eprintln!("{}: {}", input_file, e);
                process::exit(1);
            }
        };
        for f in generated.iter().flatten() {
            let path = java_out_path(dir, &f.relative_path);
            if let Some(prev) = note_java_output(java_written, &path, input_file) {
                eprintln!(
                    "warning: '{}' overwrites the file already generated from {}",
                    path, prev
                );
            }
            if let Err(e) = write_file(&path, &f.source) {
                eprintln!("error: cannot write '{}': {}", path, e);
                process::exit(1);
            }
            eprintln!("generated: {}", path);
        }
    }

    // Generate XML type representation
    if let Some(path) = &xml_path {
        let code = match codegen::xml::generate(
            &model,
            idl_filename,
            &codegen::xml::XmlOptions::default(),
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{}: {}", input_file, e);
                process::exit(1);
            }
        };
        if let Err(e) = write_file(path, &code) {
            eprintln!("error: cannot write '{}': {}", path, e);
            process::exit(1);
        }
        eprintln!("generated: {}", path);
    }
}

/// Build a `qualified/leaf name -> output module basename` map for every type
/// declared in an `#include`d (non-root) file. Both the qualified and the leaf
/// name are inserted so codegen can look up a reference written either way; on a
/// leaf collision across included files the first one wins (an accepted limit).
fn build_import_modules(
    root_file: &str,
    loaded: &[PathBuf],
    _include_dirs: &[PathBuf],
) -> HashMap<String, String> {
    let root_canon = std::fs::canonicalize(root_file).ok();
    let mut map: HashMap<String, String> = HashMap::new();
    for file in loaded {
        // Skip the root file itself; only its includes are imported.
        if root_canon.is_some() && std::fs::canonicalize(file).ok() == root_canon {
            continue;
        }
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let defs = match parser::parse_idl(&src) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let names = match resolver::declared_qualified_names(&defs) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let fname = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let module = naming::idl_to_output_name(fname);
        for q in names {
            let leaf = q.rsplit("::").next().unwrap_or(&q).to_string();
            map.entry(leaf).or_insert_with(|| module.clone());
            map.insert(q, module.clone());
        }
    }
    map
}

/// Join a backend-relative path under the Java output directory.
fn java_out_path(dir: &str, relative: &str) -> String {
    format!("{}/{}", dir.trim_end_matches(['/', '\\']), relative)
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

#[cfg(test)]
mod tests {
    use super::*;
    use dialoguer::Completion;

    // Tests run from the crate root, where `input/` holds the sample .idl files.

    #[test]
    fn completes_leaf_in_current_dir() {
        // A directory in the cwd completes with a trailing separator.
        let cands = path_candidates("inp");
        assert!(cands.iter().any(|c| c == "input/"), "got {:?}", cands);
    }

    #[test]
    fn completes_file_inside_subdirectory() {
        // Navigating into another directory completes its files.
        let cands = path_candidates("input/Hel");
        assert!(cands.iter().any(|c| c == "input/HelloWorld.idl"), "got {:?}", cands);
    }

    #[test]
    fn lists_all_entries_when_prefix_empty() {
        // A bare directory yields every entry as a candidate.
        let cands = path_candidates("input/");
        assert!(cands.len() > 1, "got {:?}", cands);
        assert!(cands.iter().all(|c| c.starts_with("input/")));
    }

    #[test]
    fn tab_cycles_through_candidates() {
        let comp = PathCompletion::default();
        let first = comp.get("input/").expect("first candidate");
        // Pressing Tab again (buffer == first suggestion) advances to the next.
        let second = comp.get(&first).expect("second candidate");
        assert_ne!(first, second, "Tab should cycle to a different candidate");
    }

    #[test]
    fn tab_descends_into_completed_directory() {
        // Tab once completes the directory; Tab again steps inside it.
        let comp = PathCompletion::default();
        let dir = comp.get("inp").expect("dir completion");
        assert_eq!(dir, "input/");
        let inside = comp.get(&dir).expect("descend into dir");
        assert!(inside.starts_with("input/") && inside != "input/", "got {}", inside);
    }

    #[test]
    fn no_match_returns_none() {
        let comp = PathCompletion::default();
        assert!(comp.get("definitely_nonexistent_xyz").is_none());
    }

    #[test]
    fn java_relative_paths_join_under_the_output_dir() {
        // -j takes a directory; the backend's relative path is joined under it.
        assert_eq!(java_out_path("out", "HelloWorld.java"), "out/HelloWorld.java");
        assert_eq!(java_out_path("out/", "com/x/HelloWorld.java"), "out/com/x/HelloWorld.java");
    }

    #[test]
    fn a_java_path_written_twice_in_one_run_is_reported() {
        let mut written = HashMap::new();
        assert_eq!(note_java_output(&mut written, "out/Point2D.java", "Complex_Arrays.idl"), None);
        // 다른 입력이 같은 경로를 쓰면 앞선 입력을 돌려준다.
        assert_eq!(
            note_java_output(&mut written, "out/Point2D.java", "Tuple_Structs.idl").as_deref(),
            Some("Complex_Arrays.idl")
        );
        // 같은 입력을 두 번 준 경우는 덮어쓰기가 아니다.
        assert_eq!(note_java_output(&mut written, "out/Point2D.java", "Tuple_Structs.idl"), None);
    }

    #[test]
    fn only_a_named_java_request_is_a_hard_error() {
        // -j: 사용자가 Java 를 콕 집었다 — 거절이면 실패해야 한다.
        assert!(java_requested_explicitly(&Some("out".to_string()), &None));
        // 마법사에서 Java 를 골랐다 — 이것도 명시다.
        assert!(java_requested_explicitly(
            &None,
            &Some([true, false, false, false, false, false, true])
        ));
        // -o 배치는 Java 를 암시할 뿐이다 — 경고 후 나머지 언어를 계속 낸다.
        assert!(!java_requested_explicitly(&None, &None));
        // 마법사에서 Java 를 고르지 않았으면 명시가 아니다.
        assert!(!java_requested_explicitly(
            &None,
            &Some([true, false, false, false, false, false, false])
        ));
    }

    #[test]
    fn java_output_suppresses_the_rust_c_default() {
        let none = || None::<String>;
        // Nothing requested -> the Rust+C default fires.
        assert!(wants_default_rust_c(
            &none(),
            &none(),
            &none(),
            &none(),
            &none(),
            &none(),
            &none()
        ));
        // -j alone requested Java, so the default must not fire.
        assert!(!wants_default_rust_c(
            &none(),
            &none(),
            &none(),
            &none(),
            &none(),
            &none(),
            &Some("out".to_string())
        ));
        // A non-Java flag suppresses it too, as it always has.
        assert!(!wants_default_rust_c(
            &none(),
            &none(),
            &none(),
            &none(),
            &Some("out.cs".to_string()),
            &none(),
            &none()
        ));
    }
}
