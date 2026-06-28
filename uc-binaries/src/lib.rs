// Shared logic for Uten Core binaries: run, compile, dump, REPL.

use std::fs;
use std::path::{Path, PathBuf};
use utencore::bytecode::UtenModule;
use utencore::plugin::PluginManager;
use utencore::vm::{Vm, VmConfig};

// ── Logging setup ──

pub fn setup_logging(debug: bool, quiet: bool) {
    let level = if quiet { "error" } else if debug { "debug" } else { "error" };
    std::env::set_var("RUST_LOG", level);
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error"))
        .format_timestamp_secs()
        .init();
}

// ── Plugin registration ──

pub fn register_plugins() -> PluginManager {
    let mut pm = PluginManager::new();
    py2uc::register(&mut pm);
    pm
}

// ── Read source file ──

pub fn read_source(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| { eprintln!("e: {e}"); std::process::exit(1); })
}

pub fn read_bytes(path: &Path) -> Vec<u8> {
    fs::read(path)
        .unwrap_or_else(|e| { eprintln!("e: {e}"); std::process::exit(1); })
}

// ── Compile source → .uclib bytes ──

pub fn compile_source(pm: &PluginManager, source: &str, filename: &str) -> Vec<u8> {
    pm.compile(source, filename)
        .unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1); })
        .to_bytes()
        .unwrap_or_else(|e| { eprintln!("e: serialize: {e}"); std::process::exit(1); })
}

pub fn compile_module(pm: &PluginManager, source: &str, filename: &str) -> UtenModule {
    pm.compile(source, filename)
        .unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1); })
}

// ── Compile with automatic dependency resolution ──

/// Compile source and automatically resolve `import` / `from X import Y`
/// dependencies. Compiled dependencies are placed in a `ucsl/` directory
/// alongside the output file.
///
/// Returns the compiled main module with UCSL dependency metadata embedded.
pub fn compile_with_deps(pm: &PluginManager, source: &str, filename: &str, out_dir: &std::path::Path) -> UtenModule {
    let mut module = compile_module(pm, source, filename);

    // Scan source for imports
    let deps = scan_imports(source);
    if deps.is_empty() {
        return module;
    }

    // Find stdlib paths
    let stdlib_paths = find_stdlib_paths();
    let ucsl_dir = out_dir.join("ucsl");
    let _ = std::fs::create_dir_all(&ucsl_dir);

    // Track resolved deps for metadata
    let mut resolved_deps: Vec<String> = Vec::new();

    for dep_name in &deps {
        // Skip if already exists in ucsl/
        let dep_uclib = ucsl_dir.join(format!("{dep_name}.uclib"));
        if dep_uclib.exists() {
            resolved_deps.push(dep_name.clone());
            continue;
        }

        // Search for source file
        let dep_source = find_dep_source(dep_name, &stdlib_paths);
        let dep_source = match dep_source {
            Some(s) => s,
            None => {
                eprintln!("warning: dependency '{dep_name}' not found (stdlib searched: {:?})", stdlib_paths);
                continue;
            }
        };

        let dep_src_text = match std::fs::read_to_string(&dep_source) {
            Ok(t) => t,
            Err(e) => { eprintln!("warning: read '{dep_name}' source failed: {e}"); continue; }
        };

        let dep_ext = dep_source.extension().and_then(|e| e.to_str()).unwrap_or("py");
        if !pm.can_compile(dep_ext) {
            eprintln!("warning: no compiler for '{dep_name}' (.{dep_ext})");
            continue;
        }

        // Compile the dependency
        match pm.compile(&dep_src_text, &dep_source.to_string_lossy()) {
            Ok(module) => {
                match module.to_bytes() {
                    Ok(bytes) => {
                        if let Err(e) = std::fs::write(&dep_uclib, &bytes) {
                            eprintln!("warning: write '{dep_name}.uclib' failed: {e}");
                        } else {
                            resolved_deps.push(dep_name.clone());
                        }
                    }
                    Err(e) => eprintln!("warning: serialize '{dep_name}' failed: {e}"),
                }
            }
            Err(e) => {
                eprintln!("warning: compile dependency '{dep_name}' failed: {e}");
            }
        }
    }

    // Embed UCSL deps in module metadata
    if !resolved_deps.is_empty() {
        let deps: Vec<utencore::ucsl::UcslDep> = resolved_deps.iter()
            .map(|n| utencore::ucsl::UcslDep::new(n)).collect();
        module.header.metadata.insert(
            utencore::ucsl::UCSL_DEPS_KEY.into(),
            utencore::ucsl::encode_deps(&deps),
        );
    }

    module
}

/// Scan source text for `import X` and `from X import Y` statements.
fn scan_imports(source: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // Skip comments and non-import lines
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // from X import Y [, Z...]
        if let Some(rest) = trimmed.strip_prefix("from ") {
            if let Some(mod_name) = rest.split_whitespace().next() {
                if !deps.contains(&mod_name.to_string()) {
                    // Skip relative imports
                    if !mod_name.starts_with('.') {
                        deps.push(mod_name.to_string());
                    }
                }
            }
        }
        // import X [, Y...]
        else if let Some(rest) = trimmed.strip_prefix("import ") {
            for part in rest.split(',') {
                let name = part.split_whitespace().next().unwrap_or("").trim();
                if !name.is_empty() && !name.starts_with('.') && !deps.contains(&name.to_string()) {
                    deps.push(name.to_string());
                }
            }
        }
    }
    deps
}

/// Find .py source files for a dependency.
fn find_dep_source(name: &str, paths: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    for base in paths {
        // <path>/<name>.py
        let candidate = base.join(format!("{name}.py"));
        if candidate.is_file() {
            return Some(candidate);
        }
        // <path>/<name>/__init__.py
        let init = base.join(name).join("__init__.py");
        if init.is_file() {
            return Some(init);
        }
    }
    None
}

/// Find standard library source directories.
fn find_stdlib_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();

    // Executable-relative — try several depths
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // target/debug/ → ../../../compilers/py2uc/lib/python
            for depth in &["..", "../..", "../../..", "../../../.."] {
                let candidate = dir.join(depth).join("compilers/py2uc/lib/python");
                if candidate.is_dir() {
                    paths.push(candidate);
                    break;
                }
            }
        }
    }

    // Project-relative: compilers/py2uc/lib/python/
    let proj = std::path::PathBuf::from("compilers/py2uc/lib/python");
    if proj.is_dir() { paths.push(proj); }

    // Also check ./ucsl/ for already-compiled shared libs
    let ucsl_dir = std::path::PathBuf::from("ucsl");
    if ucsl_dir.is_dir() { paths.push(ucsl_dir); }

    // Current directory
    paths.push(std::path::PathBuf::from("."));

    paths
}

// ── Load .uclib/.ucch from file ──

pub fn load_module(path: &Path) -> UtenModule {
    let bytes = read_bytes(path);
    UtenModule::from_bytes(&bytes)
        .unwrap_or_else(|e| { eprintln!("e: {e}"); std::process::exit(1); })
}

// ── Run a module ──

pub fn run_module(module: UtenModule) {
    let mut config = VmConfig::default();
    config.jit_enabled = false;
    let mut vm = Vm::with_config(config);
    let mid = vm.load_module(module.clone())
        .unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });
    let main_func = (module.functions.len() - 1) as utencore::types::FuncRef;
    match vm.execute(mid, main_func, vec![]) {
        Ok(val) => { if !matches!(val, utencore::types::UValue::Nil) { println!("{val}"); } }
        Err(_e) => { std::process::exit(1); } // error already printed in run_loop
    }
}

// ── Compile + run (with .ucch cache) ──

pub fn compile_and_run(path: &Path, pm: &PluginManager) {
    let source = read_source(path);
    let module = compile_module(pm, &source, &path.to_string_lossy());

    // Save .ucch cache
    if let Some(cache_path) = cache_path_for(path) {
        if let Ok(bytes) = module.to_cache_bytes() {
            let _ = fs::create_dir_all(cache_path.parent().unwrap());
            let _ = fs::write(&cache_path, &bytes);
        }
    }

    run_module(module);
}

// ── Cache path ──

pub fn cache_path_for(source: &Path) -> Option<PathBuf> {
    let name = source.file_name()?;
    let mut name_bytes = name.to_os_string().into_encoded_bytes();
    if let Some(dot) = name_bytes.iter().rposition(|&b| b == b'.') {
        name_bytes.truncate(dot);
    }
    name_bytes.extend_from_slice(b".ucch");
    let cache_name = unsafe { std::ffi::OsString::from_encoded_bytes_unchecked(name_bytes) };
    let parent = source.parent().unwrap_or(Path::new("."));
    Some(parent.join(".ucch").join(cache_name))
}

// ── Stdlib path ──

pub fn find_stdlib(compiler_name: &str) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    // ../lib/<compiler>/
    let c = exe_dir.parent()?.join("lib").join(compiler_name);
    if c.is_dir() { return Some(c.to_string_lossy().into_owned()); }

    // ./lib/<compiler>/
    let c = std::env::current_dir().ok()?.join("lib").join(compiler_name);
    if c.is_dir() { return Some(c.to_string_lossy().into_owned()); }

    // ~/.utencore/lib/<compiler>/
    if let Some(home) = home_dir() {
        let c = home.join(".utencore").join("lib").join(compiler_name);
        if c.is_dir() { return Some(c.to_string_lossy().into_owned()); }
    }

    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

// ── Dump module ──

pub fn dump_module(module: &UtenModule) {
    let kind = if &module.magic == b"UCCH" { "cache" } else { "library" };
    println!("Module: {} ({kind})", module.header.name);
    println!("Source: {}  compiler: {} v{}",
        module.header.source_lang, module.header.compiler, module.header.compiler_version);
    println!("GC: {}  JIT-recommended: {}", module.header.gc_strategy,
        if module.header.jit_recommended { "yes" } else { "no" });
    println!();
    println!("Strings ({}):", module.strings.len());
    for (i, s) in module.strings.iter().enumerate() {
        println!("  [{i}] \"{s}\"");
    }
    println!();
    println!("Functions ({}):", module.functions.len());
    for (i, f) in module.functions.iter().enumerate() {
        println!("  [{i}] {} (params={}, locals={}, {} bytes)",
            f.name, f.n_params, f.n_locals, f.bytecode.len());
        if f.bytecode.is_empty() { continue; }
        let mut off = 0;
        while off < f.bytecode.len() {
            if let Some(op) = utencore::opcodes::Opcode::from_byte(f.bytecode[off]) {
                let info = utencore::opcodes::opcode_info(op);
                let sz = info.operand_size as usize;
                let mut ops = String::new();
                for j in 0..sz { ops.push_str(&format!("{:02x} ", f.bytecode[off+1+j])); }
                println!("    {off:04x}: {op:20} {ops}");
                off += 1 + sz;
            } else {
                println!("    {off:04x}: ?? ({:02x})", f.bytecode[off]);
                off += 1;
            }
        }
    }
    println!();
    println!("Exports ({}):", module.exports.len());
    for (name, entry) in &module.exports {
        println!("  {name} -> {entry:?}");
    }
}