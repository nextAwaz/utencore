//! C struct layout computation with alignment.
//!
//! Given a struct definition with typed fields, compute:
//!  - Each field's offset (respecting native alignment)
//!  - Total struct size (padded to largest alignment)
//!  - Pack/unpack to/from raw bytes

use utencore_types::*;
use super::marshal::CType;
use super::marshal;

/// A computed struct field with offset and raw C type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CField {
    pub name: String,
    pub ctype: CType,
    pub offset: usize,
}

/// A computed struct layout.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CStructLayout {
    pub name: String,
    pub fields: Vec<CField>,
    pub size: usize,
    pub alignment: usize,
}

impl CStructLayout {
    /// Compute layout from a list of (name, CType) pairs.
    /// Uses native C alignment rules (matches what the platform's C compiler does).
    pub fn compute(name: &str, field_decls: &[(String, CType)]) -> Self {
        let mut fields = Vec::new();
        let mut offset = 0usize;
        let mut max_align = 1usize;

        for (fname, ftype) in field_decls {
            let align = ftype.alignment();
            max_align = max_align.max(align);

            // Align offset to the field's alignment
            if offset % align != 0 {
                offset += align - (offset % align);
            }

            fields.push(CField {
                name: fname.clone(),
                ctype: ftype.clone(),
                offset,
            });

            offset += ftype.size();
        }

        // Pad total size to largest alignment
        if offset % max_align != 0 {
            offset += max_align - (offset % max_align);
        }

        CStructLayout {
            name: name.to_string(),
            fields,
            size: offset,
            alignment: max_align,
        }
    }

    /// Pack UtenCore values into a raw byte buffer matching this struct's layout.
    pub fn pack(&self, field_values: &[(String, UValue)]) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; self.size];

        for (fname, fval) in field_values {
            let Some(field) = self.fields.iter().find(|f| f.name == *fname) else {
                continue;
            };
            let marshalled = marshal::marshal(fval, &field.ctype)?;
            let size = field.ctype.size();
            let off = field.offset;
            if off + size <= buf.len() {
                marshalled.write_to(&mut buf[off..off + size]);
            }
        }

        Ok(buf)
    }

    /// Unpack raw bytes into UtenCore values according to this struct's layout.
    pub fn unpack(&self, data: &[u8]) -> Result<Vec<(String, UValue)>, String> {
        let mut result = Vec::new();
        for field in &self.fields {
            let off = field.offset;
            let size = field.ctype.size();
            if off + size > data.len() {
                continue;
            }
            let val = marshal::unmarshal(&data[off..off + size], &field.ctype)?;
            result.push((field.name.clone(), val));
        }
        Ok(result)
    }

    /// Get the offset of a named field.
    pub fn offset_of(&self, name: &str) -> Option<usize> {
        self.fields.iter().find(|f| f.name == name).map(|f| f.offset)
    }
}
