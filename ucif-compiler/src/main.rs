//! UCIF Compiler — converts YAML interface definitions to binary .ucif files.
//!
//! Compilers (py2uc, ts2uc, etc.) ship .ucif files describing the C APIs
//! they need. CI{B} reads these at runtime for type-safe FFI calls.
//!
//! Usage:
//!   ucif-compiler python3.ucif.yaml -o python3.ucif
//!   ucif-compiler python3.ucif.yaml --dump   # human-readable output

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use utencore::cib::ucif::{UcifInterface, FuncProto, ParamDef, StructFieldDef, ConstDef, CallingConvention, StructDef};
use utencore::cib::structs::CStructLayout;
use utencore::cib::marshal::CType;
use utencore::types::*;
use utencore::UCIF_MAGIC;

// Re-mapping for compatibility with existing code
type UcifFile = UcifInterface;
type CFuncProto = FuncProto;
type CParam = ParamDef;
type CStructField = StructFieldDef;
type CConstant = ConstDef;

/// A YAML-compatible representation of UcifFile for easy authoring.
#[derive(Debug, serde::Deserialize)]
struct YamlUcif {
    name: String,
    description: Option<String>,
    version: Option<(u16, u16)>,
    libraries: Option<Vec<String>>,
    functions: Option<Vec<YamlFunc>>,
    structs: Option<Vec<YamlStruct>>,
    constants: Option<Vec<YamlConstant>>,
    typedefs: Option<Vec<YamlTypedef>>,
}

#[derive(Debug, serde::Deserialize)]
struct YamlFunc {
    name: String,
    ret: String,
    params: Option<Vec<YamlParam>>,
    is_variadic: Option<bool>,
    library: Option<String>,
    convention: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct YamlParam {
    name: String,
    ctype: String,
}

#[derive(Debug, serde::Deserialize)]
struct YamlStruct {
    name: String,
    fields: Vec<YamlField>,
    size: Option<usize>,
    alignment: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct YamlField {
    name: String,
    ctype: String,
    offset: usize,
}

#[derive(Debug, serde::Deserialize)]
struct YamlConstant {
    #[serde(rename = "name")]
    names: Vec<String>,
    #[serde(rename = "type")]
    ctype: String,
    value: serde_yaml::Value,
}

#[derive(Debug, serde::Deserialize)]
struct YamlTypedef {
    alias: String,
    ctype: String,
}

impl YamlUcif {
    fn to_ucif(&self) -> Result<UcifFile, String> {
        let mut functions = Vec::new();
        if let Some(ref funcs) = self.functions {
            for f in funcs {
                functions.push(CFuncProto {
                    name: f.name.clone(),
                    ret: parse_ctype(&f.ret)?,
                    params: f.params.as_ref().map(|p| {
                        p.iter().map(|param| {
                            Ok(CParam {
                                name: param.name.clone(),
                                ctype: parse_ctype(&param.ctype)?,
                            })
                        }).collect::<Result<Vec<_>, String>>()
                    }).unwrap_or(Ok(Vec::new()))?,
                    is_variadic: f.is_variadic.unwrap_or(false),
                    library: f.library.clone(),
                    convention: match f.convention.as_deref() {
                        Some("stdcall") => CallingConvention::StdCall,
                        _ => CallingConvention::Cdecl,
                    },
                });
            }
        }

        let mut structs = Vec::new();
        if let Some(ref sts) = self.structs {
            for s in sts {
                let mut fields = Vec::new();
                for f in &s.fields {
                    fields.push(CStructField {
                        name: f.name.clone(),
                        ctype: parse_ctype(&f.ctype)?,
                        offset: f.offset,
                    });
                }
                structs.push(CStructLayout {
                    name: s.name.clone(),
                    fields,
                    size: s.size.unwrap_or(0),
                    alignment: s.alignment.unwrap_or(8),
                });
            }
        }

        let mut constants = Vec::new();
        if let Some(ref cons) = self.constants {
            for c in cons {
                for name in &c.names {
                    let cval = match c.ctype.as_str() {
                        "Int" | "Long" => {
                            let v = c.value.as_i64().ok_or("expected integer")?;
                            CConstant::Int(v)
                        }
                        "UInt" | "ULong" => {
                            let v = c.value.as_u64().ok_or("expected unsigned")?;
                            CConstant::UInt(v)
                        }
                        "Float" | "Double" => {
                            let v = c.value.as_f64().ok_or("expected float")?;
                            CConstant::Float(v)
                        }
                        "String" => {
                            let v = c.value.as_str().ok_or("expected string")?;
                            CConstant::String(v.to_string())
                        }
                        "Pointer" => {
                            let v = c.value.as_u64().ok_or("expected usize")?;
                            CConstant::Pointer(v as usize)
                        }
                        _ => return Err(format!("Unknown constant type: {}", c.ctype)),
                    };
                    constants.push((name.clone(), cval));
                }
            }
        }

        let mut typedefs = Vec::new();
        if let Some(ref tds) = self.typedefs {
            for t in tds {
                typedefs.push((t.alias.clone(), parse_ctype(&t.ctype)?));
            }
        }

        Ok(UcifFile {
            magic: *UCIF_MAGIC,
            version: self.version.unwrap_or((0, 1)),
            name: self.name.clone(),
            description: self.description.clone().unwrap_or_default(),
            libraries: self.libraries.clone().unwrap_or_default(),
            functions,
            structs,
            constants,
            typedefs,
        })
    }
}

fn parse_ctype(s: &str) -> Result<CType, String> {
    Ok(match s {
        "Void" => CType::Void,
        "Bool" => CType::Bool,
        "Char" => CType::Char,
        "UChar" => CType::UChar,
        "Short" => CType::Short,
        "UShort" => CType::UShort,
        "Int" => CType::Int,
        "UInt" => CType::UInt,
        "Long" => CType::Long,
        "ULong" => CType::ULong,
        "LongLong" => CType::LongLong,
        "ULongLong" => CType::ULongLong,
        "Float" => CType::Float,
        "Double" => CType::Double,
        "VarArg" => CType::VarArg,
        _ if s.starts_with("Pointer(") && s.ends_with(')') => {
            let inner = &s[8..s.len()-1];
            CType::Pointer(Box::new(parse_ctype(inner)?))
        }
        _ if s.starts_with("ConstPointer(") && s.ends_with(')') => {
            let inner = &s[13..s.len()-1];
            CType::ConstPointer(Box::new(parse_ctype(inner)?))
        }
        _ if s.starts_with("Struct(") && s.ends_with(')') => {
            CType::Struct(s[7..s.len()-1].to_string())
        }
        _ => return Err(format!("Unknown C type: '{s}'")),
    })
}

#[derive(Parser)]
#[command(name = "ucif-compiler", about = "Compile YAML UCIF to binary .ucif")]
struct Cli {
    /// YAML input file
    input: PathBuf,
    /// Output .ucif file
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Dump compiled output as human-readable text
    #[arg(long)]
    dump: bool,
}

fn main() {
    let cli = Cli::parse();

    let yaml_str = fs::read_to_string(&cli.input)
        .expect("Failed to read input file");

    let yaml_ucif: YamlUcif = serde_yaml::from_str(&yaml_str)
        .expect("Failed to parse YAML");

    let ucif = yaml_ucif.to_ucif()
        .expect("Failed to convert to UCIF");

    if cli.dump {
        println!("UCIF Interface: {}", ucif.name);
        println!("  Description: {}", ucif.description);
        println!("  Version: {}.{}", ucif.version.0, ucif.version.1);
        println!("  Libraries: {:?}", ucif.libraries);
        println!("  Functions ({}):", ucif.functions.len());
        for (i, f) in ucif.functions.iter().enumerate() {
            let params: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
            println!("    [{i}] {:?} {}({:?})", f.ret, f.name, params);
        }
        println!("  Structs ({}):", ucif.structs.len());
        for s in &ucif.structs {
            println!("    {} ({} bytes):", s.name, s.size);
            for f in &s.fields {
                println!("      +{offset}: {name}: {ctype:?}",
                    offset = f.offset, name = f.name, ctype = f.ctype);
            }
        }
        println!("  Constants ({}):", ucif.constants.len());
        for (k, v) in &ucif.constants {
            println!("    {k} = {v:?}");
        }
        println!("  Typedefs ({}):", ucif.typedefs.len());
        for (alias, ctype) in &ucif.typedefs {
            println!("    {alias} = {ctype:?}");
        }
        return;
    }

    let output_path = cli.output.unwrap_or_else(|| {
        let mut p = cli.input.clone();
        p.set_extension("ucif");
        p
    });

    let bytes = bincode::serialize(&ucif)
        .expect("Failed to serialize UCIF");

    fs::write(&output_path, &bytes)
        .expect("Failed to write output");

    println!("Wrote {} bytes to {}", bytes.len(), output_path.display());
}
