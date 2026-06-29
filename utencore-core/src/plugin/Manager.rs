//! PluginManager — central registry for compiler and runtime plugins.

use std::collections::HashMap;
use std::sync::Arc;
#[allow(deprecated)]
use crate::ccis::{self, CcisManifest, CompileContext, CompileError};
use crate::error::{UtenError, UtenResult};
use crate::plugin::manifest::*;
use utencore_bytecode::UtenModule;
use utencore_types::{UValue, BYTECODE_VERSION};

struct LoadedPlugin {
    info: PluginInfo,
    #[allow(dead_code)]
    lib: Option<libloading::Library>,
}

/// The plugin manager — central registry for all compiler/runtime plugins.
pub struct PluginManager {
    plugins: Vec<LoadedPlugin>,
    compilers: HashMap<String, usize>,
    inline_compilers: HashMap<String, CompileFn>,
    runtime_hooks: HashMap<String, RuntimeHook>,
    pub cib_dependencies: Vec<String>,
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

    pub fn register_compiler(&mut self, manifest: CcisManifest, compile_fn: CompileFn) -> UtenResult<()> {
        let plugin_manifest: PluginManifest = manifest.into();
        self.register_compiler_from_manifest(plugin_manifest, compile_fn)
    }

    pub fn register_compiler_from_manifest(&mut self, manifest: PluginManifest, compile_fn: CompileFn) -> UtenResult<()> {
        if let Err(errs) = manifest.validate() {
            return Err(UtenError::Plugin(format!("Plugin '{}' manifest invalid: {}", manifest.name, errs.join("; "))));
        }
        if !manifest.is_bytecode_compatible() {
            return Err(UtenError::Plugin(format!(
                "Plugin '{}' targets bytecode v{}-v{}, but VM supports v{}.",
                manifest.name, manifest.bytecode_version.min, manifest.bytecode_version.max, BYTECODE_VERSION
            )));
        }
        let ccis_info = manifest.ccis.as_ref().ok_or_else(|| UtenError::Plugin(format!(
            "Plugin '{}' is not a compiler plugin", manifest.name
        )))?;
        let idx = self.plugins.len();
        self.plugins.push(LoadedPlugin {
            info: PluginInfo { manifest: manifest.clone(), plugin_type: PluginType::Compiler },
            lib: None,
        });
        for ext in &ccis_info.extensions {
            self.inline_compilers.insert(ext.clone(), compile_fn.clone());
            self.compilers.insert(ext.clone(), idx);
        }
        log::info!("PluginManager: registered '{}' v{} for {:?}", manifest.name, manifest.version, ccis_info.extensions);
        Ok(())
    }

    pub fn load_manifest_file(&mut self, path: &str) -> UtenResult<PluginManifest> {
        let json_str = std::fs::read_to_string(path)
            .map_err(|e| UtenError::Plugin(format!("Failed to read manifest '{path}': {e}")))?;
        let manifest = PluginManifest::from_json(&json_str)
            .map_err(|e| UtenError::Plugin(format!("Failed to parse plugin manifest: {e}")))?;
        manifest.validate().map_err(|errs| UtenError::Plugin(format!("Plugin '{}' manifest invalid: {}", manifest.name, errs.join("; "))))?;
        if !manifest.is_bytecode_compatible() {
            return Err(UtenError::Plugin(format!(
                "Plugin '{}' targets bytecode v{}-v{}, but VM supports v{}.",
                manifest.name, manifest.bytecode_version.min, manifest.bytecode_version.max, BYTECODE_VERSION
            )));
        }
        Ok(manifest)
    }

    pub fn scan_plugins(&mut self, dir: &str) -> usize {
        let path = std::path::Path::new(dir);
        if !path.is_dir() { return 0; }
        let mut loaded = 0;
        let Ok(entries) = std::fs::read_dir(path) else { return 0; };
        for entry in entries.flatten() {
            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            match ext {
                "so" | "dll" | "dylib" => {
                    match self.load_plugin(&p.to_string_lossy()) {
                        Ok(info) => { log::info!("PluginManager: loaded '{}' from {}", info.name(), p.display()); loaded += 1; }
                        Err(e) => { log::warn!("PluginManager: failed to load '{}': {e}", p.display()); }
                    }
                }
                "plugin.json" => {
                    match self.load_manifest_file(&p.to_string_lossy()) {
                        Ok(manifest) => { log::info!("PluginManager: scanned manifest '{}' v{}", manifest.name, manifest.version); loaded += 1; }
                        Err(e) => { log::warn!("PluginManager: failed to load manifest '{}': {e}", p.display()); }
                    }
                }
                _ => {}
            }
        }
        loaded
    }

    pub fn load_plugin(&mut self, path: &str) -> UtenResult<PluginInfo> {
        unsafe {
            let lib = libloading::Library::new(path)
                .map_err(|e| UtenError::Plugin(format!("Failed to load plugin '{path}': {e}")))?;
            let init_fn: libloading::Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> =
                lib.get(b"ccis_init")
                    .map_err(|e| UtenError::Plugin(format!("Plugin '{path}' missing ccis_init: {e}")))?;
            let c_str = init_fn();
            let json_str = std::ffi::CStr::from_ptr(c_str).to_string_lossy().into_owned();
            let ccis = CcisManifest::from_json(&json_str)
                .map_err(|e| UtenError::Plugin(format!("Plugin '{path}': {e}")))?;
            let manifest: PluginManifest = ccis.into();
            manifest.validate().map_err(|errs| UtenError::Plugin(format!("Plugin '{}' manifest invalid: {}", manifest.name, errs.join("; "))))?;
            if !manifest.is_bytecode_compatible() {
                return Err(UtenError::Plugin(format!("Plugin '{}' targets bytecode v{}-v{}, but VM supports v{}", manifest.name, manifest.bytecode_version.min, manifest.bytecode_version.max, BYTECODE_VERSION)));
            }
            let ccis_info = manifest.ccis.as_ref().ok_or_else(|| UtenError::Plugin(format!("Plugin '{}' is not a compiler plugin", manifest.name)))?;
            let info = PluginInfo { manifest: manifest.clone(), plugin_type: PluginType::Compiler };
            for ext in &ccis_info.extensions { self.compilers.insert(ext.clone(), self.plugins.len()); }
            self.plugins.push(LoadedPlugin { info: info.clone(), lib: Some(lib) });
            Ok(info)
        }
    }

    pub fn compile(&self, source: &str, filename: &str) -> UtenResult<UtenModule> {
        self.compile_with_options(source, filename, &ccis::CompilerOptions::default())
    }

    pub fn compile_with_options(&self, source: &str, filename: &str, options: &ccis::CompilerOptions) -> UtenResult<UtenModule> {
        let ext = std::path::Path::new(filename).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let compile_fn = self.inline_compilers.get(&ext).ok_or_else(|| {
            let supported = self.list_extensions();
            UtenError::Plugin(format!("No compiler plugin registered for '.{ext}' files.\nSupported extensions: {}", supported.join(", ")))
        })?;
        let module_name = std::path::Path::new(filename).file_stem().and_then(|s| s.to_str()).unwrap_or("module");
        let mut module = UtenModule::new(module_name);
        let compile_result = {
            let mut builder = crate::bytecode::ModuleBuilder::new(&mut module);
            let mut ctx = ccis::CompileContext { source, filename, builder: &mut builder, options };
            let res = compile_fn(&mut ctx);
            std::mem::drop(ctx);
            builder.finalize();
            res
        };
        compile_result.map_err(|errors| {
            UtenError::Plugin(errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"))
        })?;
        if let Some(&idx) = self.compilers.get(&ext) {
            module.header.gc_strategy = self.plugins[idx].info.default_gc().to_string();
        }
        Ok(module)
    }

    pub fn get_compiler(&self, extension: &str) -> Option<&PluginInfo> {
        self.compilers.get(extension).map(|&idx| &self.plugins[idx].info)
    }

    pub fn list_extensions(&self) -> Vec<&str> {
        let mut exts: Vec<&str> = self.compilers.keys().map(|s| s.as_str()).collect();
        exts.sort();
        exts
    }

    pub fn can_compile(&self, ext: &str) -> bool { self.compilers.contains_key(ext) }

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
