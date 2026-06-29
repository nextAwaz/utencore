//! Plugin manifest types — self-description format for compiler and runtime plugins.

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::ccis::{self, CcisCompileResult, CompileContext, CompileError, CCIS_ABI_VERSION};
#[allow(deprecated)]
use crate::ccis::CcisManifest;
use crate::error::{UtenError, UtenResult};
use utencore_types::{UValue, BYTECODE_VERSION};

pub const PLUGIN_MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    #[serde(rename = "plugin_type")]
    pub plugin_type: PluginType,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub bytecode_version: BytecodeVersionRange,
    #[serde(default)]
    pub ccis: Option<CcisPluginInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeVersionRange {
    pub min: u32,
    pub max: u32,
}

impl Default for BytecodeVersionRange {
    fn default() -> Self {
        BytecodeVersionRange { min: 1, max: BYTECODE_VERSION }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcisPluginInfo {
    pub language: String,
    pub extensions: Vec<String>,
    #[serde(default = "default_gc_strategy")]
    pub default_gc: String,
}

fn default_gc_strategy() -> String { "generational".into() }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Compiler,
    Runtime,
    Debugger,
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub plugin_type: PluginType,
}

impl PluginInfo {
    pub fn name(&self) -> &str { &self.manifest.name }
    pub fn version(&self) -> &str { &self.manifest.version }
    pub fn description(&self) -> &str { &self.manifest.description }
    pub fn extensions(&self) -> &[String] {
        self.manifest.ccis.as_ref().map(|i| i.extensions.as_slice()).unwrap_or(&[])
    }
    pub fn default_gc(&self) -> &str {
        self.manifest.ccis.as_ref().map(|i| i.default_gc.as_str()).unwrap_or("generational")
    }
}

pub type CompileFn = Arc<dyn for<'a, 'b> Fn(&mut CompileContext<'a, 'b>) -> Result<(), Vec<CompileError>> + Send + Sync>;
pub type RuntimeHook = Box<dyn Fn(&[UValue]) -> UtenResult<UValue> + Send + Sync>;

impl PluginManifest {
    pub fn new_compiler(name: &str, version: &str, language: &str, extensions: &[String]) -> Self {
        PluginManifest {
            manifest_version: PLUGIN_MANIFEST_VERSION,
            name: name.into(), version: version.into(),
            plugin_type: PluginType::Compiler,
            description: String::new(), author: String::new(),
            bytecode_version: BytecodeVersionRange::default(),
            ccis: Some(CcisPluginInfo {
                language: language.into(), extensions: extensions.to_vec(),
                default_gc: "generational".into(),
            }),
        }
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("PluginManifest parse: {e}"))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.manifest_version != PLUGIN_MANIFEST_VERSION {
            errors.push(format!("manifest_version {} != current {}", self.manifest_version, PLUGIN_MANIFEST_VERSION));
        }
        if self.name.is_empty() { errors.push("plugin name is empty".into()); }
        if self.version.is_empty() { errors.push("plugin version is empty".into()); }
        if self.bytecode_version.min == 0 || self.bytecode_version.min > self.bytecode_version.max {
            errors.push(format!("invalid bytecode version range: {}..{}", self.bytecode_version.min, self.bytecode_version.max));
        }
        if self.plugin_type == PluginType::Compiler && self.ccis.is_none() {
            errors.push("compiler plugins must provide CCIS info".into());
        }
        if let Some(ref ccis) = self.ccis {
            if ccis.extensions.is_empty() { errors.push("CCIS: at least one file extension required".into()); }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn is_bytecode_compatible(&self) -> bool {
        self.bytecode_version.min <= BYTECODE_VERSION && self.bytecode_version.max >= BYTECODE_VERSION
    }

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

#[allow(deprecated)]
impl From<CcisManifest> for PluginManifest {
    fn from(ccis: CcisManifest) -> Self {
        PluginManifest {
            manifest_version: PLUGIN_MANIFEST_VERSION,
            name: ccis.name.clone(), version: ccis.version.clone(),
            plugin_type: PluginType::Compiler,
            description: ccis.description.clone(), author: String::new(),
            bytecode_version: BytecodeVersionRange::default(),
            ccis: Some(CcisPluginInfo {
                language: ccis.language.clone(),
                extensions: ccis.extensions.iter().map(|s| s.clone()).collect(),
                default_gc: ccis.default_gc.clone(),
            }),
        }
    }
}
