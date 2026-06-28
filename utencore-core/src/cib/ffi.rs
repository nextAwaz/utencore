//! FFI primitives backed by libffi-sys (cross-platform: Linux, macOS, Windows).

use libffi_sys as ffi;
use std::ffi::c_void;

use super::marshal::CType;

// ── Type mapping ──

pub fn ffi_type_for(ct: &CType) -> *mut ffi::ffi_type {
    unsafe {
        match ct {
            CType::Void => &mut ffi::ffi_type_void,
            CType::Bool | CType::Char | CType::UChar => &mut ffi::ffi_type_sint8,
            CType::Short => &mut ffi::ffi_type_sint16,
            CType::UShort => &mut ffi::ffi_type_uint16,
            CType::Int => &mut ffi::ffi_type_sint32,
            CType::UInt => &mut ffi::ffi_type_uint32,
            CType::Long => {
                if std::mem::size_of::<i64>() == 8 { &mut ffi::ffi_type_sint64 }
                else { &mut ffi::ffi_type_sint32 }
            }
            CType::ULong => {
                if std::mem::size_of::<i64>() == 8 { &mut ffi::ffi_type_uint64 }
                else { &mut ffi::ffi_type_uint32 }
            }
            CType::LongLong => &mut ffi::ffi_type_sint64,
            CType::ULongLong => &mut ffi::ffi_type_uint64,
            CType::Float => &mut ffi::ffi_type_float,
            CType::Double => &mut ffi::ffi_type_double,
            CType::Pointer(_) | CType::ConstPointer(_) | CType::FuncPtr(_) => &mut ffi::ffi_type_pointer,
            CType::Array(inner, _) => ffi_type_for(inner),
            _ => &mut ffi::ffi_type_void,
        }
    }
}

// ── ABI ──

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiAbi {
    DefaultAbi = 2, // FFI_DEFAULT_ABI
}

// ── CIF ──

pub struct FfiCif {
    pub cif: ffi::ffi_cif,
    pub arg_types: Vec<*mut ffi::ffi_type>,
}

pub fn prepare_cif(abi: FfiAbi, rtype: *mut ffi::ffi_type, arg_types: &[*mut ffi::ffi_type]) -> Result<FfiCif, String> {
    let nargs = arg_types.len() as u32;
    let mut cif = FfiCif {
        cif: unsafe { std::mem::zeroed() },
        arg_types: arg_types.to_vec(),
    };
    let ret = unsafe {
        ffi::ffi_prep_cif(
            &mut cif.cif,
            abi as u32 as ffi::ffi_abi,
            nargs,
            rtype,
            cif.arg_types.as_mut_ptr(),
        )
    };
    if ret == 0 { // FFI_OK
        Ok(cif)
    } else {
        Err(format!("ffi_prep_cif failed with code {ret}"))
    }
}

// ── Call ──

pub unsafe fn call(cif: &FfiCif, fn_ptr: usize, ret_buf: *mut c_void, arg_ptrs: &mut [*mut c_void]) {
    let pf: unsafe extern "C" fn() = std::mem::transmute(fn_ptr as *const ());
    ffi::ffi_call(
        &cif.cif as *const ffi::ffi_cif as *mut _,
        Some(pf),
        ret_buf,
        arg_ptrs.as_mut_ptr(),
    );
}
