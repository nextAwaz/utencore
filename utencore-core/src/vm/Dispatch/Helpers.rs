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
}
