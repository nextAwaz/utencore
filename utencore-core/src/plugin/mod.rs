//! CCIS-based Plugin System — UtenCore Common Compiler Interface Specification.
//!
//! Every compiler plugin ships a CCIS manifest (JSON) that declares its
//! identity, supported extensions, default GC strategy, and required CIB
//! interfaces. This replaces the old ad-hoc PluginInfo/register_compiler API.
//!
//! Two registration modes:
//!   1. In-process (linked as Rust crate) — register_compiler() with CcisManifest
//!   2. Dynamic (.so/.dll loaded at runtime) — calls ccis_init() for manifest
//!
//! The manifest JSON is used at registration time; the PluginManager holds
//! the parsed struct internally.

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use utencore_bytecode::UtenModule;
use crate::ccis::{self, CcisCompileResult, CompileContext, CompileError, CCIS_ABI_VERSION};
#[allow(deprecated)]
use crate::ccis::CcisManifest;
use crate::error::{UtenError, UtenResult};
use utencore_types::{UValue, BYTECODE_VERSION};

// ═══════════════════════════════════════════════════════════════
// PluginManifest — universal plugin self-description format
// ═══════════════════════════════════════════════════════════════

/// Current plugin manifest format version.
pub const PLUGIN_MANIFEST_VERSION: u32 = 1;

/// A plugin's JSON self-description file.
///
/// Every UtenCore plugin ships a `<name>.plugin.json` alongside its
/// code (`.so`/`.dll`/`.rs`) that declares:
///   - Who it is (name, version, type, author)
///   - What bytecode version it targets (not VM version — separate concern!)
///
/// # Example
///
/// ```json
/// {
///     "manifest_version": 1,
///     "name": "py2uc",
///     "version": "0.0.4",
///     "plugin_type": "compiler",
///     "description": "Python 3 to UtenCore bytecode compiler",
///     "author": "Uten Core Contributors",
///     "bytecode_version": { "min": 1, "max": 2 },
///     "ccis": {
///         "language": "Python 3",
///         "extensions": ["py"],
///         "default_gc": "generational"
///     }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Manifest format version (must match PLUGIN_MANIFEST_VERSION).
    pub manifest_version: u32,

    /// Plugin name (e.g. "py2uc", "utenstd.math").
    pub name: String,

    /// Plugin version string (e.g. "0.0.4").
    pub version: String,

    /// Plugin type discriminator.
    #[serde(rename = "plugin_type")]
    pub plugin_type: PluginType,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,

    /// Author / maintainer string.
    #[serde(default)]
    pub author: String,

    /// Bytecode version range this plugin targets.
    /// The VM checks this at registration; mismatch is a hard error.
    pub bytecode_version: BytecodeVersionRange,

    /// CCIS-specific fields (required when plugin_type == "compiler").
    #[serde(default)]
    pub ccis: Option<CcisPluginInfo>,
}

/// Bytecode version compatibility range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeVersionRange {
    /// Minimum supported bytecode version (inclusive).
    pub min: u32,
    /// Maximum supported bytecode version (inclusive).
    pub max: u32,
}

impl Default for BytecodeVersionRange {
    fn default() -> Self {
        BytecodeVersionRange { min: 1, max: BYTECODE_VERSION }
    }
}

/// CCIS-specific extension for compiler plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcisPluginInfo {
    /// Source language name (e.g. "Python 3", "TypeScript").
    pub language: String,

    /// File extensions this compiler handles (e.g. ["py"]).
    pub extensions: Vec<String>,

    /// Default GC strategy for compiled modules.
    #[serde(default = "default_gc_strategy")]
    pub default_gc: String,
}

fn default_gc_strategy() -> String { "generational".into() }

impl PluginManifest {
    /// Create a new plugin manifest for a compiler plugin.
    pub fn new_compiler(
        name: &str,
        version: &str,
        language: &str,
        extensions: &[String],
    ) -> Self {
        PluginManifest {
            manifest_version: PLUGIN_MANIFEST_VERSION,
            name: name.into(),
            version: version.into(),
            plugin_type: PluginType::Compiler,
            description: String::new(),
            author: String::new(),
            bytecode_version: BytecodeVersionRange::default(),
            ccis: Some(CcisPluginInfo {
                language: language.into(),
                extensions: extensions.to_vec(),
                default_gc: "generational".into(),
            }),
        }
    }

    /// Parse from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("PluginManifest parse: {e}"))
    }

    /// Serialize to a JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Validate the manifest.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.manifest_version != PLUGIN_MANIFEST_VERSION {
            errors.push(format!(
                "manifest_version {} != current {}",
                self.manifest_version, PLUGIN_MANIFEST_VERSION
            ));
        }
        if self.name.is_empty() {
            errors.push("plugin name is empty".into());
        }
        if self.version.is_empty() {
            errors.push("plugin version is empty".into());
        }
        if self.bytecode_version.min == 0 || self.bytecode_version.min > self.bytecode_version.max {
            errors.push(format!(
                "invalid bytecode version range: {}..{}",
                self.bytecode_version.min, self.bytecode_version.max
            ));
        }
        if self.plugin_type == PluginType::Compiler && self.ccis.is_none() {
            errors.push("compiler plugins must provide CCIS info".into());
        }
        if let Some(ref ccis) = self.ccis {
            if ccis.extensions.is_empty() {
                errors.push("CCIS: at least one file extension required".into());
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    /// Check if this plugin's bytecode version range is compatible with the VM.
    pub fn is_bytecode_compatible(&self) -> bool {
        self.bytecode_version.min <= BYTECODE_VERSION
            && self.bytecode_version.max >= BYTECODE_VERSION
    }

    /// Convert to a CCIS manifest (for compiler plugins).
    pub fn to_ccis_manifest(&self) -> Option<CcisManifest> {
        self.ccis.as_ref().map(|info| {
            let ext_refs: Vec<&str> = info.extensions.iter().map(|s| s.as_str()).collect();
            #[allow(deprecated)]
            let mut m = CcisManifest::new(&self.name, &info.language, &ext_refs);
            m.version = self.version.clone();
            m.description = self.description.clone();
            m.default_gc = info.default_gc.clone();
            m
        })
    }
}

/// Convert a legacy CcisManifest into the canonical PluginManifest format.
#[allow(deprecated)]
impl From<CcisManifest> for PluginManifest {
    fn from(ccis: CcisManifest) -> Self {
        let extensions: Vec<String> = ccis.extensions.iter().map(|s| s.clone()).collect();
        PluginManifest {
            manifest_version: PLUGIN_MANIFEST_VERSION,
            name: ccis.name.clone(),
            version: ccis.version.clone(),
            plugin_type: PluginType::Compiler,
            description: ccis.description.clone(),
            author: String::new(),
            bytecode_version: BytecodeVersionRange::default(),
            ccis: Some(CcisPluginInfo {
                language: ccis.language.clone(),
                extensions,
                default_gc: ccis.default_gc.clone(),
            }),
        }
    }
}

/// A compile function: receives compilation context, fills in the module directly.
/// Returns Ok(()) on success, or a list of structured errors.
pub type CompileFn = Arc<dyn for<'a, 'b> Fn(&mut CompileContext<'a, 'b>) -> Result<(), Vec<CompileError>> + Send + Sync>;

/// A runtime hook function
pub type RuntimeHook = Box<dyn Fn(&[UValue]) -> UtenResult<UValue> + Send + Sync>;

/// Plugin type (derived from manifest usage)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Compiler,
    Runtime,
    Debugger,
}

/// A loaded plugin with its manifest.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub plugin_type: PluginType,
}

impl PluginInfo {
    pub fn name(&self) -> &str { &self.manifest.name }
    pub fn version(&self) -> &str { &self.manifest.version }
    pub fn description(&self) -> &str { &self.manifest.description }

    /// Extensions this compiler handles (empty for non-compiler plugins).
    pub fn extensions(&self) -> &[String] {
        self.manifest.ccis.as_ref().map(|i| i.extensions.as_slice()).unwrap_or(&[])
    }

    /// Default GC strategy (from CCIS info).
    pub fn default_gc(&self) -> &str {
        self.manifest.ccis.as_ref().map(|i| i.default_gc.as_str()).unwrap_or("generational")
    }

    /// CIB interface dependencies.
    pub fn cib_interfaces(&self) -> &[String] {
        &[] // CIB deps are now tracked via manifest.metadata
    }
}

/// The plugin manager — central registry for all compiler/runtime plugins.
pub struct PluginManager {
    plugins: Vec<LoadedPlugin>,
    /// Extension -> compiler index
    compilers: HashMap<String, usize>,
    /// In-process compiler functions (extension -> compile_fn)
    inline_compilers: HashMap<String, CompileFn>,
    /// Runtime hooks
    runtime_hooks: HashMap<String, RuntimeHook>,
    /// CIB interface dependencies by plugin name (auto-loaded at VM init)
    pub cib_dependencies: Vec<String>,
}

struct LoadedPlugin {
    info: PluginInfo,
    #[allow(dead_code)]
    lib: Option<libloading::Library>,
}

impl PluginManager {
    pub fn new() -> Self {
        PluginManager {
            plugins: Vec::new(),
            compilers: HashMap::new(),
            inline_compilers: HashMap::new(),
            runtime_hooks: HashMap::new(),
            cib_dependencies: Vec::new(),
        }
    }

    // ── CCIS-based in-process registration ──

    /// Register a compiler plugin via CCIS manifest (legacy).
    ///
    /// Internally converts the `CcisManifest` → `PluginManifest` and delegates
    /// to `register_compiler_from_manifest`, which performs bytecode version
    /// compatibility checking.
    pub fn register_compiler(
        &mut self,
        manifest: CcisManifest,
        compile_fn: CompileFn,
    ) -> UtenResult<()> {
        let plugin_manifest: PluginManifest = manifest.into();
        self.register_compiler_from_manifest(plugin_manifest, compile_fn)
    }

    /// Register a compiler plugin via the PluginManifest format.
    ///
    /// Includes bytecode version compatibility checking — if the plugin targets
    /// a bytecode version the VM doesn't support, registration is rejected.
    pub fn register_compiler_from_manifest(
        &mut self,
        manifest: PluginManifest,
        compile_fn: CompileFn,
    ) -> UtenResult<()> {
        // 1. Validate manifest
        if let Err(errs) = manifest.validate() {
            return Err(UtenError::Plugin(format!(
                "Plugin '{}' manifest invalid: {}",
                manifest.name, errs.join("; ")
            )));
        }

        // 2. Check bytecode version compatibility
        if !manifest.is_bytecode_compatible() {
            return Err(UtenError::Plugin(format!(
                "Plugin '{}' targets bytecode v{}-v{}, but VM supports v{}. \
                 Please use a compatible plugin version.",
                manifest.name,
                manifest.bytecode_version.min,
                manifest.bytecode_version.max,
                BYTECODE_VERSION
            )));
        }

        // 3. Ensure this is a compiler plugin
        let ccis_info = manifest.ccis.as_ref().ok_or_else(|| UtenError::Plugin(format!(
            "Plugin '{}' is not a compiler plugin (missing CCIS info)", manifest.name
        )))?;

        // 4. Store plugin
        let idx = self.plugins.len();
        let info = PluginInfo {
            manifest: manifest.clone(),
            plugin_type: PluginType::Compiler,
        };
        self.plugins.push(LoadedPlugin { info, lib: None });

        // 5. Register compile function for each extension
        for ext in &ccis_info.extensions {
            self.inline_compilers.insert(ext.clone(), compile_fn.clone());
            self.compilers.insert(ext.clone(), idx);
        }

        log::info!(
            "PluginManager: registered '{}' v{} for {:?} (bytecode {}-{}, GC: {})",
            manifest.name, manifest.version, ccis_info.extensions,
            manifest.bytecode_version.min, manifest.bytecode_version.max,
            ccis_info.default_gc
        );
        Ok(())
    }

    /// Load a compiler plugin from a `.plugin.json` manifest file on disk.
    ///
    /// The JSON file is parsed as a PluginManifest, validated, then used
    /// to register the compiler. The compile function must already be registered
    /// (e.g., via `register_compiler_from_manifest`).
    pub fn load_manifest_file(&mut self, path: &str) -> UtenResult<PluginManifest> {
        let json_str = std::fs::read_to_string(path)
            .map_err(|e| UtenError::Plugin(format!("Failed to read manifest '{path}': {e}")))?;
        let manifest = PluginManifest::from_json(&json_str)
            .map_err(|e| UtenError::Plugin(format!("Failed to parse plugin manifest: {e}")))?;

        if let Err(errs) = manifest.validate() {
            return Err(UtenError::Plugin(format!(
                "Plugin '{}' manifest invalid: {}",
                manifest.name, errs.join("; ")
            )));
        }

        if !manifest.is_bytecode_compatible() {
            return Err(UtenError::Plugin(format!(
                "Plugin '{}' targets bytecode v{}-v{}, but VM supports v{}. \
                 Please use a compatible plugin version.",
                manifest.name,
                manifest.bytecode_version.min,
                manifest.bytecode_version.max,
                BYTECODE_VERSION
            )));
        }

        log::info!(
            "PluginManager: loaded manifest '{}' v{} (bytecode {}-{})",
            manifest.name, manifest.version,
            manifest.bytecode_version.min, manifest.bytecode_version.max
        );

        Ok(manifest)
    }

    // ── Plugin directory scanning ──

    /// Scan a directory for dynamic plugin files (.so/.dll) and load them.
    /// Each plugin is expected to export `ccis_init()` returning a JSON manifest,
    /// and optionally `ccis_compile()` for compilation dispatch.
    /// Returns the number of plugins successfully loaded.
    pub fn scan_plugins(&mut self, dir: &str) -> usize {
        let path = std::path::Path::new(dir);
        if !path.is_dir() {
            log::warn!("PluginManager: scan directory '{dir}' not found");
            return 0;
        }

        let mut loaded = 0;
        let Ok(entries) = std::fs::read_dir(path) else { return 0; };

        for entry in entries.flatten() {
            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "so" | "dll" | "dylib" => {
                    match self.load_plugin(&p.to_string_lossy()) {
                        Ok(info) => {
                            log::info!("PluginManager: scanned+loaded '{}' from {}", info.name(), p.display());
                            loaded += 1;
                        }
                        Err(e) => {
                            log::warn!("PluginManager: failed to load plugin '{}': {e}", p.display());
                        }
                    }
                }
                "plugin.json" => {
                    // Try loading as plugin manifest → compile via existing compiler
                    match self.load_manifest_file(&p.to_string_lossy()) {
                        Ok(manifest) => {
                            log::info!("PluginManager: scanned manifest '{}' v{}", manifest.name, manifest.version);
                            loaded += 1;
                        }
                        Err(e) => {
                            log::warn!("PluginManager: failed to load manifest '{}': {e}", p.display());
                        }
                    }
                }
                _ => {}
            }
        }
        loaded
    }

    // ── Dynamic plugin loading (CCIS ABI) ──

    /// Load a dynamic plugin from a shared library.
    ///
    /// Expects the .so/.dll to export:
    ///   `ccis_init()` -> *const c_char (JSON manifest string)
    ///   `ccis_compile(...)` -> CcisCompileResult
    pub fn load_plugin(&mut self, path: &str) -> UtenResult<PluginInfo> {
        unsafe {
            let lib = libloading::Library::new(path)
                .map_err(|e| UtenError::Plugin(format!("Failed to load plugin '{path}': {e}")))?;

            // ── Load CCIS manifest ──
            let init_fn: libloading::Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> =
                lib.get(b"ccis_init")
                    .map_err(|e| UtenError::Plugin(format!(
                        "Plugin '{path}' missing ccis_init: {e}"
                    )))?;

            let c_str = init_fn();
            let json_str = std::ffi::CStr::from_ptr(c_str)
                .to_string_lossy()
                .into_owned();

            // Parse as CcisManifest (C ABI format) → convert to PluginManifest
            let ccis = CcisManifest::from_json(&json_str)
                .map_err(|e| UtenError::Plugin(format!("Plugin '{path}': {e}")))?;
            let manifest: PluginManifest = ccis.into();

            // Validate with bytecode version check
            if let Err(errs) = manifest.validate() {
                return Err(UtenError::Plugin(format!(
                    "Plugin '{}' manifest invalid: {}", manifest.name, errs.join("; ")
                )));
            }
            if !manifest.is_bytecode_compatible() {
                return Err(UtenError::Plugin(format!(
                    "Plugin '{}' targets bytecode v{}-v{}, but VM supports v{}",
                    manifest.name,
                    manifest.bytecode_version.min,
                    manifest.bytecode_version.max,
                    BYTECODE_VERSION
                )));
            }

            // Register compiler for its extensions
            let ccis_info = manifest.ccis.as_ref()
                .ok_or_else(|| UtenError::Plugin(format!(
                    "Plugin '{}' is not a compiler plugin", manifest.name
                )))?;

            let info = PluginInfo {
                manifest: manifest.clone(),
                plugin_type: PluginType::Compiler,
            };

            for ext in &ccis_info.extensions {
                self.compilers.insert(ext.clone(), self.plugins.len());
            }

            log::info!(
                "PluginManager: loaded dynamic plugin '{}' v{} (bytecode {}-{}, GC: {})",
                manifest.name, manifest.version,
                manifest.bytecode_version.min, manifest.bytecode_version.max,
                ccis_info.default_gc
            );

            self.plugins.push(LoadedPlugin {
                info: info.clone(),
                lib: Some(lib),
            });

            Ok(info)
        }
    }

    // ── Compilation dispatch ──

    /// Compile source code (default options).
    pub fn compile(&self, source: &str, filename: &str) -> UtenResult<UtenModule> {
        self.compile_with_options(source, filename, &ccis::CompilerOptions::default())
    }

    /// Compile source code with explicit options.
    /// Options control optimization/debug; GC strategy is from the plugin manifest.
    pub fn compile_with_options(
        &self, source: &str, filename: &str, options: &ccis::CompilerOptions,
    ) -> UtenResult<UtenModule> {
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Find the compiler plugin for this extension
        let compile_fn = self.inline_compilers.get(&ext).ok_or_else(|| {
            let supported = self.list_extensions();
            UtenError::Plugin(format!(
                "No compiler plugin registered for '.{ext}' files.\n\
                 Supported extensions: {}", supported.join(", "))
            )
        })?;

        // Create the module and builder
        let module_name = std::path::Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("module");
        let mut module = UtenModule::new(module_name);

        // Build and finalize (drop ctx before finalize to release borrow)
        let compile_result = {
            let mut builder = crate::bytecode::ModuleBuilder::new(&mut module);
            let mut ctx = ccis::CompileContext {
                source,
                filename,
                builder: &mut builder,
                options,
            };
            let res = compile_fn(&mut ctx);
            std::mem::drop(ctx);
            builder.finalize();
            res
        };
        compile_result.map_err(|errors| {
            let msg = errors.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            UtenError::Plugin(msg)
        })?;

        // Apply GC strategy from manifest
        if let Some(&idx) = self.compilers.get(&ext) {
            let gc = self.plugins[idx].info.default_gc();
            module.header.gc_strategy = gc.to_string();
        }

        Ok(module)
    }

    /// Compile: legacy wrapper returning .uclib bytes (deprecated).
    #[deprecated(since = "0.0.5", note = "use compile() which returns UtenModule")]
    pub fn compile_to_bytes(&self, source: &str, filename: &str) -> UtenResult<Vec<u8>> {
        let module = self.compile(source, filename)?;
        module.to_bytes().map_err(|e| UtenError::Plugin(format!("Serialize: {e}")))
    }

    #[allow(deprecated)]
    pub fn compile_module(&self, source: &str, filename: &str) -> UtenResult<UtenModule> {
        self.compile(source, filename)
    }

    // ── Queries ──

    pub fn get_compiler(&self, extension: &str) -> Option<&PluginInfo> {
        self.compilers.get(extension).map(|&idx| &self.plugins[idx].info)
    }

    pub fn list_extensions(&self) -> Vec<&str> {
        let mut exts: Vec<&str> = self.compilers.keys().map(|s| s.as_str()).collect();
        exts.sort();
        exts
    }

    pub fn can_compile(&self, ext: &str) -> bool {
        self.compilers.contains_key(ext)
    }

    pub fn list_plugins(&self) -> Vec<&PluginInfo> {
        self.plugins.iter().map(|p| &p.info).collect()
    }

    pub fn register_hook(&mut self, name: &str, hook: RuntimeHook) {
        self.runtime_hooks.insert(name.to_string(), hook);
    }

    pub fn call_hook(&self, name: &str, args: &[UValue]) -> UtenResult<UValue> {
        if let Some(hook) = self.runtime_hooks.get(name) {
            hook(args)
        } else {
            Err(UtenError::Plugin(format!("Unknown hook: '{name}'")))
        }
    }
}

// ── CCIS ABI helpers (exported by the VM for dynamic plugins) ──

#[no_mangle]
pub extern "C" fn ccis_plugin_version() -> u32 {
    CCIS_ABI_VERSION
}

/// CCIS compile function — called by the PluginManager when dispatching
/// compilation to a dynamic plugin.
///
/// Parameters (C ABI):
///   source: null-terminated C string with source code
///   filename: null-terminated C string with source filename
///   options_json: null-terminated JSON-encoded CompilerOptions
///
/// Returns: CcisCompileResult { data, len, error }
#[no_mangle]
pub extern "C" fn ccis_compile(
    source: *const std::os::raw::c_char,
    filename: *const std::os::raw::c_char,
    options_json: *const std::os::raw::c_char,
) -> CcisCompileResult {
    let _source = if source.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(source) }.to_string_lossy().into_owned()
    };
    let _filename = if filename.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(filename) }.to_string_lossy().into_owned()
    };
    let err_msg = if options_json.is_null() {
        "options_json is null"
    } else {
        "Dynamic ccis_compile not implemented"
    };
    CcisCompileResult {
        data: std::ptr::null(),
        len: 0,
        error: std::ffi::CString::new(err_msg).unwrap().into_raw(),
    }
}

/// CCIS free result — releases memory allocated by ccis_compile.
#[no_mangle]
pub extern "C" fn ccis_free_result(result: CcisCompileResult) {
    if !result.data.is_null() {
        unsafe { let _ = Vec::from_raw_parts(result.data as *mut u8, result.len, result.len); }
    }
    if !result.error.is_null() {
        unsafe { let _ = std::ffi::CString::from_raw(result.error as *mut std::os::raw::c_char); }
    }
}
