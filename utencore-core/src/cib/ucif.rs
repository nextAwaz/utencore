//! UCIF (UtenCore C Interface) schema, validation, and serialization.
//!
//! UCIF files describe C library interfaces that compilers want to use.
//! Each .ucif file specifies:
//!  - Which shared libraries to load
//!  - Function prototypes (name, params, return type)
//!  - Struct layouts
//!  - Constants
//!
//! Format version 1: YAML-style descriptor, compiled to binary at build time.
//! Format version 2 (future): structured binary with checksums.

use serde::{Deserialize, Serialize};

use super::marshal::CType;

/// UCIF file format version.
pub const UCIF_VERSION: u32 = 1;

/// UCIF binary magic bytes.
pub const UCIF_MAGIC: [u8; 4] = *b"UCIF";

// ── UCIF schema types ──

/// A complete UCIF interface descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UcifInterface {
    /// Format version
    pub version: u32,
    /// Human-readable interface name (e.g. "libm", "libpython3.12")
    pub name: String,
    /// Description for documentation
    #[serde(default)]
    pub description: String,
    /// Shared library names (without lib prefix or .so suffix)
    pub libraries: Vec<String>,
    /// Function prototypes
    #[serde(default)]
    pub functions: Vec<FuncProto>,
    /// Struct layouts
    #[serde(default)]
    pub structs: Vec<StructDef>,
    /// Named constants
    #[serde(default)]
    pub constants: Vec<ConstDef>,
    /// Type aliases (e.g. "size_t" → ULong)
    #[serde(default)]
    pub typedefs: Vec<(String, CType)>,
}

/// A C function prototype.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncProto {
    pub name: String,
    pub ret: CType,
    pub params: Vec<ParamDef>,
    /// If true, accepts additional variadic arguments
    #[serde(default)]
    pub variadic: bool,
    /// Calling convention (default: cdecl)
    #[serde(default)]
    pub abi: CallingConvention,
}

/// A function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub name: String,
    pub ctype: CType,
}

/// Calling convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallingConvention {
    Cdecl,
    #[serde(rename = "stdcall")]
    StdCall,
}

impl Default for CallingConvention {
    fn default() -> Self { CallingConvention::Cdecl }
}

/// A struct field definition (before layout computation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructFieldDef {
    pub name: String,
    pub ctype: CType,
}

/// A struct definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructFieldDef>,
}

/// A constant definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ConstDef {
    #[serde(rename = "int")]
    Int(i64),
    #[serde(rename = "uint")]
    UInt(u64),
    #[serde(rename = "float")]
    Float(f64),
    /// String constant (literal)
    #[serde(rename = "str")]
    Str(String),
    /// Pointer to a known symbol
    #[serde(rename = "ptr")]
    Ptr(String),
}

// ── Serialization ──

impl UcifInterface {
    /// Serialize to binary (UCIF format, version 1).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&UCIF_MAGIC);
        buf.extend_from_slice(&UCIF_VERSION.to_le_bytes());
        let json = serde_json::to_vec(self).unwrap_or_default();
        buf.extend_from_slice(&(json.len() as u32).to_le_bytes());
        buf.extend_from_slice(&json);
        buf
    }

    /// Deserialize from binary.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 10 {
            return Err("UCIF too short".into());
        }
        if &bytes[..4] != UCIF_MAGIC {
            return Err(format!("Invalid UCIF magic: expected UCIF, got {:?}", &bytes[..4]));
        }
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != UCIF_VERSION {
            return Err(format!("Unsupported UCIF version: {version}"));
        }
        let json_len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        if 12 + json_len > bytes.len() {
            return Err("UCIF truncated".into());
        }
        let json = &bytes[12..12 + json_len];
        serde_json::from_slice(json).map_err(|e| format!("UCIF parse: {e}"))
    }
}

// ── Validation ──

impl UcifInterface {
    /// Validate the interface for correctness.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.name.is_empty() {
            errors.push("Interface name is empty".into());
        }
        if self.libraries.is_empty() {
            errors.push("No libraries specified".into());
        }
        if self.version != UCIF_VERSION {
            errors.push(format!("Unsupported version: {}", self.version));
        }

        // Validate functions
        for (i, f) in self.functions.iter().enumerate() {
            if f.name.is_empty() {
                errors.push(format!("Function {i}: empty name"));
            }
            for (j, p) in f.params.iter().enumerate() {
                if p.name.is_empty() {
                    errors.push(format!("Function '{}': param {j} has empty name", f.name));
                }
            }
        }

        // Validate structs
        for s in &self.structs {
            if s.name.is_empty() {
                errors.push("Struct has empty name".into());
            }
            for f in &s.fields {
                if f.name.is_empty() {
                    errors.push(format!("Struct '{}': field has empty name", s.name));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// ── Builder API for programmatic creation ──

impl UcifInterface {
    pub fn new(name: &str) -> Self {
        UcifInterface {
            version: UCIF_VERSION,
            name: name.to_string(),
            description: String::new(),
            libraries: Vec::new(),
            functions: Vec::new(),
            structs: Vec::new(),
            constants: Vec::new(),
            typedefs: Vec::new(),
        }
    }

    pub fn with_library(mut self, lib: &str) -> Self {
        self.libraries.push(lib.to_string());
        self
    }

    pub fn with_function(mut self, name: &str, ret: CType, params: Vec<ParamDef>) -> Self {
        self.functions.push(FuncProto {
            name: name.to_string(),
            ret,
            params,
            variadic: false,
            abi: CallingConvention::Cdecl,
        });
        self
    }

    pub fn with_struct(mut self, name: &str, fields: Vec<StructFieldDef>) -> Self {
        self.structs.push(StructDef {
            name: name.to_string(),
            fields,
        });
        self
    }
}
