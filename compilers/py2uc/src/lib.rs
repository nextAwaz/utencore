//! py2uc: Python 3 → UtenCore bytecode compiler.
//!
//! Pipeline: source → tokenizer → parser → codegen → .uclib
//! Registered via PluginManifest with bytecode version compatibility declaration.

use std::path::Path;
use utencore::ccis::CcisManifest;
use utencore::plugin::{PluginManager, PluginManifest, BytecodeVersionRange, CcisPluginInfo};
use utencore::BYTECODE_VERSION;

pub mod ast;
pub mod tokenizer;
pub mod parser;
pub mod codegen;

/// Compile Python source into a UtenModule (via CompileContext using ModuleBuilder).
pub fn compile_python<'a, 'b>(ctx: &mut utencore::ccis::CompileContext<'a, 'b>) -> Result<(), Vec<utencore::ccis::CompileError>> {
    let name = Path::new(ctx.filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("py");

    // Respect compiler options
    if ctx.options.optimize > 0 {
        log::info!("py2uc: optimization level {} requested (not yet implemented)", ctx.options.optimize);
    }

    let mut p = parser::Parser::new(ctx.source);
    match p.parse() {
        Ok(program) => {
            let mut errors: Vec<utencore::ccis::CompileError> = Vec::new();
            if ctx.options.emit_debug {
                log::info!("py2uc: compilation successful ({} statements)", program.stmts.len());
            }
            codegen::compile_into_builder(&program, name, ctx.builder)
                .map_err(|e| vec![utencore::ccis::CompileError::error(&e, ctx.filename, 0, 0)])
        }
        Err(e) => {
            let (line, col) = p.current_pos();
            Err(vec![utencore::ccis::CompileError::error(&e, ctx.filename, line, col)])
        }
    }
}

/// Build the PluginManifest for py2uc.
///
/// Declares:
///   - Bytecode version range [1, BYTECODE_VERSION] — compatibility with
///     VM bytecode versions 1 through the current max.
///   - Plugin type: compiler
///   - CCIS info: Python 3, .py files, generational GC
pub fn plugin_manifest() -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        name: "py2uc".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        plugin_type: utencore::plugin::PluginType::Compiler,
        description: "Python 3 to UtenCore bytecode compiler".into(),
        author: "Uten Core Contributors".into(),
        bytecode_version: BytecodeVersionRange {
            min: 1,
            max: BYTECODE_VERSION,
        },
        ccis: Some(CcisPluginInfo {
            language: "Python 3".into(),
            extensions: vec!["py".into()],
            default_gc: "generational".into(),
        }),
    }
}

/// Legacy CCIS manifest (for C ABI compat).
fn ccis_manifest() -> CcisManifest {
    let mut m = CcisManifest::new("py2uc", "Python 3", &["py"]);
    m.description = "Python 3 to UtenCore bytecode compiler".into();
    m.default_gc = "generational".into();
    m
}

/// Register py2uc via the new PluginManifest API.
///
/// This performs automatic bytecode version compatibility checking:
/// if the VM's BYTECODE_VERSION is outside [1, BYTECODE_VERSION],
/// registration is rejected with a clear error.
pub fn register(pm: &mut PluginManager) {
    let manifest = plugin_manifest();
    pm.register_compiler_from_manifest(manifest, std::sync::Arc::new(|ctx| compile_python(ctx)))
        .expect("register py2uc compiler");
}

/// CCIS ABI — dynamic plugin entry point (also exported for static linking).
/// Returns a legacy CcisManifest JSON string for backward-compatible loaders.
#[no_mangle]
pub extern "C" fn ccis_init() -> *const std::os::raw::c_char {
    let json = ccis_manifest().to_json();
    let c_str = std::ffi::CString::new(json).unwrap_or_default();
    c_str.into_raw()
}
