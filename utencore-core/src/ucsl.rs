//! UCSL — Uten Core Sharing Library.
//!
//! A shared-library system for .uclib modules. UCSL enables:
//! - Standardized paths for finding shared .uclib files
//! - Cross-module calling with proper resolution
//! - Automatic dependency tracking during compilation
//! - Runtime module discovery and loading
//!
//! # Directory Layout
//!
//! ```text
//! (project)/
//!   ucsl/                   # Project-local shared libs
//!     index.json            # Optional registry
//!     mylib.uclib
//!
//! ~/.utencore/ucsl/         # User-global shared libs
//!   index.json
//!   stdlib.uclib
//!   math.uclib
//!
//! /usr/share/utencore/ucsl/ # System-wide shared libs
//!   index.json
//!   ...
//! ```
//!
//! # .ucsl Manifest
//!
//! Each shared library MAY have a companion `.ucsl` JSON file alongside the
//! `.uclib` binary, declaring exports and dependencies. Alternatively, the
//! same info can be embedded in the module's metadata header.
//!
//! # Versioning
//!
//! UCSL uses the bytecode version for compatibility, not a separate version.
//! A .uclib with bytecode_version ≤ VM's BYTECODE_VERSION is loadable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// UCSL registry format version.
pub const UCSL_VERSION: u32 = 1;

/// .ucsl manifest file extension.
pub const UCSL_MANIFEST_EXT: &str = ".ucsl";

// ── UCSL Manifest ──

/// A UCSL shared-library manifest (JSON).
///
/// Lives alongside a .uclib file as `<name>.ucsl`, or is embedded in the
/// module's `header.metadata` map under the `"ucsl"` key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UcslManifest {
    pub ucsl_version: u32,
    pub name: String,
    pub version: String,
    pub description: String,
    /// Exported function/value names (what other modules can `from X import Y`)
    #[serde(default)]
    pub exports: Vec<String>,
    /// Dependencies on other UCSL libraries (names only, resolved at load time)
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Minimum bytecode version required
    pub bytecode_version: u32,
}

impl UcslManifest {
    pub fn new(name: &str) -> Self {
        UcslManifest {
            ucsl_version: UCSL_VERSION,
            name: name.to_string(),
            version: "0.1.0".into(),
            description: String::new(),
            exports: Vec::new(),
            dependencies: Vec::new(),
            bytecode_version: utencore_types::BYTECODE_VERSION,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("UCSL manifest parse: {e}"))
    }
}

// ── UCSL Registry ──

/// A registry of available UCSL shared libraries.
#[derive(Debug, Clone, Default)]
pub struct UcslRegistry {
    /// All discovered libraries: name -> (path, manifest)
    pub libraries: HashMap<String, UcslEntry>,
    /// Search paths in priority order
    pub search_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct UcslEntry {
    pub path: PathBuf,
    pub manifest: UcslManifest,
}

impl UcslRegistry {
    /// Create a new registry with default search paths.
    pub fn new() -> Self {
        let mut reg = UcslRegistry::default();
        reg.add_default_paths();
        reg
    }

    /// Add standard UCSL search paths.
    pub fn add_default_paths(&mut self) {
        // 1. Current directory ./ucsl/
        self.search_paths.push(PathBuf::from("./ucsl"));

        // 2. Home directory ~/.utencore/ucsl/
        if let Some(home) = home_dir() {
            self.search_paths.push(home.join(".utencore").join("ucsl"));
        }

        // 3. System path
        self.search_paths.push(PathBuf::from("/usr/share/utencore/ucsl"));

        // 4. Executable-relative ../ucsl/
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                self.search_paths.push(parent.join("ucsl"));
                self.search_paths.push(parent.join("../ucsl"));
            }
        }
    }

    /// Add a custom search path.
    pub fn add_path(&mut self, path: &str) {
        self.search_paths.push(PathBuf::from(path));
    }

    /// Scan all search paths and discover .uclib files with optional .ucsl manifests.
    pub fn discover(&mut self) {
        let paths: Vec<PathBuf> = self.search_paths.clone();
        for path in &paths {
            self.scan_path(path);
        }
    }

    fn scan_path(&mut self, dir: &Path) {
        if !dir.is_dir() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };

        for entry in entries.flatten() {
            let p = entry.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "uclib" && ext != "ucch" {
                continue;
            }
            let name = p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
            if name.is_empty() || self.libraries.contains_key(&name) {
                continue;
            }

            // Try to load companion .ucsl manifest
            let manifest_path = p.with_extension("ucsl");
            let manifest = if manifest_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    UcslManifest::from_json(&content).unwrap_or_else(|_| UcslManifest::new(&name))
                } else {
                    UcslManifest::new(&name)
                }
            } else {
                UcslManifest::new(&name)
            };

            self.libraries.insert(name.clone(), UcslEntry {
                path: p,
                manifest,
            });
        }
    }

    /// Find a library by name in the registry.
    pub fn find(&self, name: &str) -> Option<&UcslEntry> {
        self.libraries.get(name)
    }

    /// Resolve a library's .uclib path by name.
    /// First checks the registry, then does a direct filesystem search.
    pub fn resolve(&self, name: &str) -> Option<PathBuf> {
        // Check registry first
        if let Some(entry) = self.libraries.get(name) {
            if entry.path.exists() {
                return Some(entry.path.clone());
            }
        }

        // Direct filesystem search
        let candidates = [
            format!("{name}.uclib"),
            format!("{name}.ucch"),
            format!("ucsl/{name}.uclib"),
            format!("ucsl/{name}.ucch"),
        ];
        for path in &self.search_paths {
            for candidate in &candidates {
                let full = if candidate.contains('/') {
                    PathBuf::from(candidate)
                } else {
                    path.join(candidate)
                };
                if full.exists() {
                    return Some(full);
                }
            }
        }

        None
    }

    /// List all discovered library names.
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.libraries.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

// ── UCSL dependency tracking ──

/// A dependency entry for UCSL resolution at compile time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UcslDep {
    /// Library name (e.g. "stdlib", "math")
    pub name: String,
    /// Required version constraint (optional, semver-like)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl UcslDep {
    pub fn new(name: &str) -> Self {
        UcslDep { name: name.to_string(), version: None }
    }
}

// ── UCSL metadata keys for .uclib header ──

/// Metadata key in ModuleHeader.metadata for UCSL deps (JSON array of UcslDep).
pub const UCSL_DEPS_KEY: &str = "ucsl_deps";

/// Metadata key for UCSL library name (present when this module IS a shared lib).
pub const UCSL_LIB_NAME_KEY: &str = "ucsl_name";

/// Encode a list of UCSL dependencies into a JSON string for metadata.
pub fn encode_deps(deps: &[UcslDep]) -> String {
    serde_json::to_string(deps).unwrap_or_default()
}

/// Decode UCSL dependencies from a JSON metadata string.
pub fn decode_deps(json: &str) -> Vec<UcslDep> {
    serde_json::from_str(json).unwrap_or_default()
}
