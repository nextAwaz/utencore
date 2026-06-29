//! OOP operation dispatch handlers.

use super::super::*;
use crate::error::{UtenError, UtenResult};
use utencore_bytecode::ExportEntry;
use utencore_types::*;

impl Vm {
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
        // Link class into namespace
        if let HeapObject::Namespace { ref mut members, .. } = self.gc.get_mut(ns_handle) {
            members.push((0, UValue::Gc(class_handle, ValueTag::Class)));
        }
        self.stack.push(UValue::Gc(class_handle, ValueTag::Class));
        Ok(())
    }
}
