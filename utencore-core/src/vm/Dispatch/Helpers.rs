//! Dispatch helper methods — extracted from the match for readability.
//! Each method handles a specific complex opcode arm.

use crate::vm::Vm;
use crate::error::{UtenError, UtenResult};
use utencore_bytecode::ExportEntry;
use utencore_types::*;

impl Vm {
    // ── OOP dispatchers ──

    pub(super) fn op_new_namespace(&mut self, operand: u32) -> UtenResult<()> {
        let nid = operand;
        let mid = self.current_module_id() as u16;
        let ns = HeapObject::Namespace { name: nid, members: Vec::new(), module_id: mid };
        self.stack.push(UValue::Gc(self.gc.alloc(ns), ValueTag::Namespace));
        Ok(())
    }

    pub(super) fn op_new_class(&mut self) -> UtenResult<()> {
        let ns_handle = self.pop_gc(ValueTag::Namespace)?;
        let class_obj = HeapObject::Class {
            name: 0, fields: Vec::new(), methods: Vec::new(),
            parent: None, constructor: None,
        };
        let class_handle = self.gc.alloc(class_obj);
        if let HeapObject::Namespace { ref mut members, .. } = self.gc.get_mut(ns_handle) {
            members.push((0, UValue::Gc(class_handle, ValueTag::Class)));
        }
        self.stack.push(UValue::Gc(class_handle, ValueTag::Class));
        Ok(())
    }

    pub(super) fn op_get_attr(&mut self, target_sid: StringId) -> UtenResult<()> {
        let obj = self.pop()?;
        let val = match obj {
            UValue::Gc(h, ValueTag::Object) => {
                let mut result = UValue::Nil;
                let (class_fields, proto_chain, found_method_data) = {
                    let obj_gc = self.gc.get(h);
                    match obj_gc {
                        HeapObject::Object { class_handle, fields, proto, .. } => {
                            let cf = match self.gc.get(*class_handle) {
                                HeapObject::Class { fields: cff, .. } => cff.clone(),
                                _ => vec![],
                            };
                            for (field_sid, _) in cf.iter().enumerate() {
                                if field_sid as StringId == target_sid {
                                    result = fields[field_sid].clone();
                                    break;
                                }
                            }
                            (cf.clone(), *proto, {
                                if matches!(result, UValue::Nil) {
                                    match self.gc.get(*class_handle) {
                                        HeapObject::Class { methods, .. } =>
                                            methods.iter().find(|(sid, _)| *sid == target_sid).map(|(_, fr)| *fr),
                                        _ => None,
                                    }
                                } else { None }
                            })
                        }
                        _ => (vec![], None, None),
                    }
                };
                if matches!(result, UValue::Nil) {
                    if let Some(fr) = found_method_data {
                        let mid = self.current_module_id() as ModuleId;
                        result = UValue::Gc(self.gc.alloc(HeapObject::Closure { func: fr, captures: vec![], module_id: mid }), ValueTag::Closure);
                    }
                }
                if matches!(result, UValue::Nil) {
                    let mut current_proto = proto_chain;
                    while let Some(ph) = current_proto {
                        let (pfields, pch, next_proto, proto_method) = {
                            let obj_gc = self.gc.get(ph);
                            match obj_gc {
                                HeapObject::Object { fields, class_handle, proto: pp, .. } => {
                                    for (field_sid, _) in class_fields.iter().enumerate() {
                                        if field_sid as StringId == target_sid {
                                            if field_sid < fields.len() { result = fields[field_sid].clone(); }
                                            break;
                                        }
                                    }
                                    let method = if matches!(result, UValue::Nil) {
                                        match self.gc.get(*class_handle) {
                                            HeapObject::Class { methods, .. } =>
                                                methods.iter().find(|(sid, _)| *sid == target_sid).map(|(_, fr)| *fr),
                                            _ => None,
                                        }
                                    } else { None };
                                    (fields.clone(), *class_handle, *pp, method)
                                }
                                _ => (vec![], 0, None, None),
                            }
                        };
                        if matches!(result, UValue::Nil) {
                            if let Some(fr) = proto_method {
                                let mid = self.current_module_id() as ModuleId;
                                result = UValue::Gc(self.gc.alloc(HeapObject::Closure { func: fr, captures: vec![], module_id: mid }), ValueTag::Closure);
                            }
                        }
                        if matches!(result, UValue::Nil) { current_proto = next_proto; }
                        else { break; }
                    }
                }
                result
            }
            UValue::Gc(h, ValueTag::Namespace) => {
                if let HeapObject::Namespace { members, .. } = self.gc.get(h) {
                    members.iter().find(|(sid, _)| *sid == target_sid).map(|(_, v)| v.clone()).unwrap_or(UValue::Nil)
                } else { UValue::Nil }
            }
            _ => UValue::Nil,
        };
        self.stack.push(val);
        Ok(())
    }

    pub(super) fn op_set_attr(&mut self, target_sid: StringId) -> UtenResult<()> {
        let val = self.pop()?;
        let val_h = if let UValue::Gc(ch, _) = &val { Some(*ch) } else { None };
        let obj = self.pop()?;
        match obj {
            UValue::Gc(h, ValueTag::Object) => {
                let class_fields = {
                    let obj_gc = self.gc.get(h);
                    if let HeapObject::Object { class_handle, .. } = obj_gc {
                        if let HeapObject::Class { fields: cf, .. } = self.gc.get(*class_handle) { cf.clone() }
                        else { vec![] }
                    } else { vec![] }
                };
                if let HeapObject::Object { fields, .. } = self.gc.get_mut(h) {
                    if let Some(pos) = class_fields.iter().position(|fsid| *fsid == target_sid) {
                        if pos < fields.len() { fields[pos] = val; }
                    }
                }
                if let Some(ch) = val_h { self.gc.write_barrier(h, ch); }
            }
            UValue::Gc(h, ValueTag::Namespace) => {
                if let HeapObject::Namespace { ref mut members, .. } = self.gc.get_mut(h) {
                    if let Some((_, ref mut v)) = members.iter_mut().find(|(sid, _)| *sid == target_sid) {
                        *v = val;
                    } else { members.push((target_sid, val)); }
                }
                if let Some(ch) = val_h { self.gc.write_barrier(h, ch); }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn op_class_add_method(&mut self, operand: u32) -> UtenResult<()> {
        let is_constructor = (operand & 0x8000) != 0;
        let name_sid = (operand & 0x7FFF) as u16;
        let func_val = self.pop()?;
        let func_ref = match func_val {
            UValue::Int32(i) => i as FuncRef,
            UValue::Int64(i) => i as FuncRef,
            UValue::Gc(h, tag) if tag == ValueTag::Closure || tag == ValueTag::Lambda => {
                if let HeapObject::Closure { func, .. } = self.gc.get(h) { *func }
                else if let HeapObject::Lambda { func, .. } = self.gc.get(h) { *func }
                else { return Err(UtenError::TypeError { expected: "function".into(), actual: format!("{:?}", tag) }); }
            }
            _ => return Err(UtenError::TypeError { expected: "function ref or closure".into(), actual: format!("{:?}", func_val.tag()) }),
        };
        let class_handle = self.pop_gc(ValueTag::Class)?;
        if let HeapObject::Class { ref mut methods, ref mut constructor, .. } = self.gc.get_mut(class_handle) {
            methods.push((name_sid as StringId, func_ref));
            if is_constructor { *constructor = Some(func_ref); }
        }
        self.stack.push(UValue::Gc(class_handle, ValueTag::Class));
        Ok(())
    }

    pub(super) fn op_instance_of(&mut self) -> UtenResult<()> {
        let class_val = self.pop()?;
        let obj_val = self.pop()?;
        let is_instance = match (&obj_val, &class_val) {
            (UValue::Gc(oh, ValueTag::Object), UValue::Gc(ch, ValueTag::Class)) => {
                let mut current = *oh;
                loop {
                    match self.gc.get(current) {
                        HeapObject::Object { class_handle, proto, .. } => {
                            if *class_handle == *ch { break true; }
                            match proto { Some(p) => current = *p, None => break false, }
                        }
                        _ => break false,
                    }
                }
            }
            _ => false,
        };
        self.stack.push(UValue::Bool(is_instance));
        Ok(())
    }

    pub(super) fn op_has_field(&mut self, name_sid: StringId) -> UtenResult<()> {
        let obj = self.pop()?;
        let found = match obj {
            UValue::StructInline(sid, ref bytes) => {
                self.modules.get(self.current_module_id())
                    .and_then(|m| m.module.get_struct(sid))
                    .map_or(false, |sd| sd.fields.iter().any(|f| f.name == name_sid))
            }
            UValue::Gc(h, ValueTag::Struct) => {
                match self.gc.get(h) { HeapObject::Struct(fields) => fields.iter().any(|(id, _)| *id == name_sid), _ => false }
            }
            UValue::Gc(h, ValueTag::Object) => {
                match self.gc.get(h) {
                    HeapObject::Object { class_handle, fields, .. } => {
                        let class_fields = match self.gc.get(*class_handle) {
                            HeapObject::Class { fields: cf, .. } => cf.clone(), _ => vec![],
                        };
                        class_fields.iter().any(|fsid| *fsid == name_sid) || fields.iter().any(|f| !matches!(f, UValue::Nil))
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        self.stack.push(UValue::Bool(found));
        Ok(())
    }

    // ── Module import/export dispatchers ──

    pub(super) fn op_import(&mut self, name_sid: StringId) -> UtenResult<()> {
        let mid = self.current_module_id();
        let name = self.modules[mid].module.strings.get(name_sid as usize)
            .cloned().unwrap_or_default();
        let resolved = self.resolve_ns_alias(&name);
        if let Some(module_id) = self.import_module_by_name(&resolved) {
            let ns_handle = self.build_module_namespace(module_id, &resolved);
            self.stack.push(UValue::Gc(ns_handle, ValueTag::Namespace));
        } else {
            eprintln!("Warning: module '{name}' not found (resolved: '{resolved}')");
            self.stack.push(UValue::Nil);
        }
        Ok(())
    }

    pub(super) fn op_import_func(&mut self, name_sid: StringId) -> UtenResult<()> {
        let module_id_val = self.pop()?;
        let module_id: Option<usize> = match &module_id_val {
            UValue::Gc(h, ValueTag::Namespace) => {
                match self.gc.get(*h) { HeapObject::Namespace { module_id, .. } => Some(*module_id as usize), _ => None }
            }
            UValue::Int32(id) => Some(*id as usize),
            UValue::Int64(id) => Some(*id as usize),
            _ => None,
        };
        let Some(module_id) = module_id else { self.stack.push(UValue::Nil); return Ok(()); };
        let mid = self.current_module_id();
        let name = self.modules[mid].module.strings.get(name_sid as usize).cloned().unwrap_or_default();
        if module_id < self.modules.len() {
            if let Some(val) = self.modules[module_id].export_values.get(&name) {
                self.stack.push(val.clone());
            } else if let Some(export) = self.modules[module_id].module.exports.get(&name) {
                match export {
                    ExportEntry::Function(fr) => {
                        let closure = HeapObject::Closure { func: *fr, captures: vec![], module_id: module_id as ModuleId };
                        self.stack.push(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                    }
                    ExportEntry::Global(g) => {
                        let v = self.modules[module_id].globals.get(*g as usize).cloned().unwrap_or(UValue::Nil);
                        self.stack.push(v);
                    }
                    ExportEntry::Type(_) => { self.stack.push(UValue::Nil); }
                }
            } else {
                let fi = self.modules[module_id].module.functions.iter().position(|f| f.name == name);
                if let Some(fi) = fi {
                    let closure = HeapObject::Closure { func: fi as FuncRef, captures: vec![], module_id: module_id as ModuleId };
                    self.stack.push(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                } else { self.stack.push(UValue::Nil); }
            }
        } else { self.stack.push(UValue::Nil); }
        Ok(())
    }

    pub(super) fn op_import_value(&mut self, name_sid: StringId) -> UtenResult<()> {
        let module_id_val = self.pop()?;
        let module_id: Option<usize> = match &module_id_val {
            UValue::Gc(h, ValueTag::Namespace) => {
                match self.gc.get(*h) { HeapObject::Namespace { module_id, .. } => Some(*module_id as usize), _ => None }
            }
            UValue::Int32(id) => Some(*id as usize),
            UValue::Int64(id) => Some(*id as usize),
            _ => None,
        };
        let Some(module_id) = module_id else { self.stack.push(UValue::Nil); return Ok(()); };
        let mid = self.current_module_id();
        let name = self.modules[mid].module.strings.get(name_sid as usize).cloned().unwrap_or_default();
        if module_id < self.modules.len() {
            self.stack.push(self.modules[module_id].export_values.get(&name).cloned().unwrap_or(UValue::Nil));
        } else { self.stack.push(UValue::Nil); }
        Ok(())
    }

    pub(super) fn op_export(&mut self, sid: StringId) -> UtenResult<()> {
        let val = self.pop()?;
        let mid = self.current_module_id();
        if let Some(name) = self.modules[mid].module.strings.get(sid as usize).cloned() {
            if !name.is_empty() { self.modules[mid].export_values.insert(name, val); }
        }
        Ok(())
    }

    // ── CIB dispatch ──

    pub(super) fn op_cib_call(&mut self, nargs: i64) -> UtenResult<()> {
        let mut args = Vec::with_capacity(nargs as usize);
        for _ in 0..nargs { args.push(self.pop()?); }
        args.reverse();
        let func_val = self.pop()?;
        match func_val {
            UValue::NativeFn(ref nh) => match self.cib.call(nh, &args) {
                Ok(v) => self.stack.push(v),
                Err(e) => { eprintln!("CIB call error: {e}"); self.stack.push(UValue::Nil); }
            }
            UValue::Gc(h, ValueTag::Opaque) => {
                if let HeapObject::Opaque { data, .. } = self.gc.get(h) {
                    let fn_ptr = usize::from_le_bytes(data[..8].try_into().unwrap_or([0u8; 8]));
                    if fn_ptr == 0 { self.stack.push(UValue::Nil); }
                    else {
                        let ret_type = crate::cib::ffi::ffi_type_for(&crate::cib::marshal::CType::Void);
                        let cif = match crate::cib::ffi::prepare_cif(crate::cib::ffi::FfiAbi::DefaultAbi, ret_type, &[]) {
                            Ok(cif) => cif,
                            Err(e) => { eprintln!("CIB CIF error: {e}"); self.stack.push(UValue::Nil); return Ok(()); }
                        };
                        let mut ret_buf = [0u8; 8];
                        let mut empty_args: [*mut std::ffi::c_void; 0] = [];
                        unsafe { crate::cib::ffi::call(&cif, fn_ptr, ret_buf.as_mut_ptr() as *mut std::ffi::c_void, &mut empty_args); }
                        self.stack.push(UValue::Int64(i64::from_le_bytes(ret_buf)));
                    }
                } else { self.stack.push(UValue::Nil); }
            }
            _ => { eprintln!("CIB: expected function pointer, got {:?}", func_val.tag()); self.stack.push(UValue::Nil); }
        }
        Ok(())
    }

    pub(super) fn op_cib_call_typed(&mut self) -> UtenResult<()> {
        let func_idx = self.pop_int()? as usize;
        let nargs = self.pop_int()? as usize;
        let mut args = Vec::with_capacity(nargs);
        for _ in 0..nargs { args.push(self.pop()?); }
        args.reverse();
        let iface_idx = self.pop_int()? as usize;
        match self.cib.call_typed(iface_idx, func_idx, &args) {
            Ok(val) => self.stack.push(val),
            Err(e) => { eprintln!("CIB typed call error: {e}"); self.stack.push(UValue::Nil); }
        }
        Ok(())
    }

    // ── Object/Class field access ──

    pub(super) fn op_get_field_object(&mut self, h: GcHandle, fid: StringId) -> UtenResult<()> {
        let mid = self.current_module_id();
        let field_name = self.resolve_string_across_modules(fid);
        if field_name.is_empty() { self.stack.push(UValue::Nil); return Ok(()); }
        let (fields_data, methods_data) = {
            let obj = self.gc.get(h);
            match obj {
                HeapObject::Object { class_handle, fields, .. } => {
                    let cf = match self.gc.get(*class_handle) { HeapObject::Class { fields: cf, .. } => cf.clone(), _ => vec![] };
                    let cm = match self.gc.get(*class_handle) { HeapObject::Class { methods, .. } => methods.clone(), _ => vec![] };
                    (fields.clone(), cm)
                }
                _ => (vec![], vec![]),
            }
        };
        if !fields_data.is_empty() {
            let fields_info = {
                let obj = self.gc.get(h);
                match obj {
                    HeapObject::Object { class_handle, .. } => match self.gc.get(*class_handle) {
                        HeapObject::Class { fields: cf, .. } => cf.clone(), _ => vec![]
                    }
                    _ => vec![]
                }
            };
            if let Some(pos) = fields_info.iter().position(|fsid| *fsid == fid as StringId) {
                if pos < fields_data.len() { self.stack.push(fields_data[pos].clone()); return Ok(()); }
            }
        }
        for (msid, fr) in &methods_data {
            if self.resolve_string_across_modules(*msid) == field_name {
                let closure = HeapObject::Closure { func: *fr, captures: vec![], module_id: mid as ModuleId };
                self.stack.push(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                return Ok(());
            }
        }
        self.stack.push(UValue::Nil);
        Ok(())
    }

    pub(super) fn op_get_field_namespace(&mut self, h: GcHandle, fid: StringId) -> UtenResult<()> {
        let (ns_members, ns_mod_id) = match self.gc.get(h) {
            HeapObject::Namespace { members, module_id, .. } => (members.clone(), *module_id),
            _ => (vec![], 0),
        };
        let caller_mid = self.current_module_id();
        let field_name = self.modules.get(caller_mid)
            .and_then(|m| m.module.strings.get(fid as usize).cloned())
            .unwrap_or_default();
        if field_name.is_empty() { self.stack.push(UValue::Nil); }
        else {
            let found = ns_members.iter().find(|(sid, _)| {
                self.modules.get(ns_mod_id as usize)
                    .and_then(|m| m.module.strings.get(*sid as usize))
                    .map(|s| s == &field_name).unwrap_or(false)
            });
            self.stack.push(found.map(|(_, v)| v.clone()).unwrap_or(UValue::Nil));
        }
        Ok(())
    }

    // ── HasAttr with prototype chain walking ──

    pub(super) fn op_has_attr(&mut self, target_sid: StringId) -> UtenResult<()> {
        let obj = self.pop()?;
        let found = match obj {
            UValue::Gc(h, ValueTag::Object) => {
                let mut result = false;
                if let HeapObject::Object { class_handle, fields, proto, .. } = self.gc.get(h) {
                    let class_fields = match self.gc.get(*class_handle) {
                        HeapObject::Class { fields: cf, .. } => cf.clone(), _ => vec![],
                    };
                    for (field_sid, _) in class_fields.iter().enumerate() {
                        if field_sid as StringId == target_sid && field_sid < fields.len() && !matches!(fields[field_sid], UValue::Nil) {
                            result = true; break;
                        }
                    }
                    if !result {
                        if let HeapObject::Class { methods, .. } = self.gc.get(*class_handle) {
                            result = methods.iter().any(|(sid, _)| *sid == target_sid);
                        }
                    }
                    if !result {
                        let mut current_proto = *proto;
                        while let Some(ph) = current_proto {
                            if let HeapObject::Object { fields: pf, class_handle: pch, proto: pp, .. } = self.gc.get(ph) {
                                let pclass_fields = match self.gc.get(*pch) {
                                    HeapObject::Class { fields: cf, .. } => cf.clone(), _ => vec![],
                                };
                                for (field_sid, _) in pclass_fields.iter().enumerate() {
                                    if field_sid as StringId == target_sid && field_sid < pf.len() && !matches!(pf[field_sid], UValue::Nil) {
                                        result = true; break;
                                    }
                                }
                                if !result {
                                    if let HeapObject::Class { methods, .. } = self.gc.get(*pch) {
                                        result = methods.iter().any(|(sid, _)| *sid == target_sid);
                                    }
                                }
                                if !result { current_proto = *pp; } else { break; }
                            } else { break; }
                        }
                    }
                }
                result
            }
            _ => false,
        };
        self.stack.push(UValue::Bool(found));
        Ok(())
    }
}
