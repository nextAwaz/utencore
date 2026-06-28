// Mark-Sweep GC
// ═══════════════════════════════════════════════════════════

use std::collections::VecDeque;
use utencore_types::{HeapObject, UValue, ValueTag, FuncRef, ModuleId};
use super::{GcEngine, GcHandle, GcStats, GcRoot, GenerationalGc, TraceRoots};

pub struct MarkSweepGc {
    objects: Vec<Option<HeapObject>>,
    marked: Vec<bool>,
    pins: Vec<u32>,
    free_slots: VecDeque<GcHandle>,
    stats: GcStats,
}

impl MarkSweepGc {
    pub fn new() -> Self {
        MarkSweepGc {
            objects: Vec::new(),
            marked: Vec::new(),
            pins: Vec::new(),
            free_slots: VecDeque::new(),
            stats: GcStats::default(),
        }
    }
}

impl GcEngine for MarkSweepGc {
    fn alloc(&mut self, obj: HeapObject) -> GcHandle {
        let handle = if let Some(free) = self.free_slots.pop_front() {
            self.objects[free as usize] = Some(obj);
            free
        } else {
            let handle = self.objects.len() as GcHandle;
            self.objects.push(Some(obj));
            self.marked.push(false);
            self.pins.push(0);
            handle
        };
        self.stats.total_allocations += 1;
        handle
    }

    fn get(&self, handle: GcHandle) -> &HeapObject {
        self.objects[handle as usize].as_ref().unwrap()
    }

    fn get_mut(&mut self, handle: GcHandle) -> &mut HeapObject {
        self.objects[handle as usize].as_mut().unwrap()
    }

    fn collect(&mut self, roots: &mut dyn TraceRoots) {
        self.stats.total_collections += 1;

        // Mark phase
        self.marked.fill(false);

        // Find roots via TraceRoots trait
        let mut worklist: VecDeque<GcHandle> = VecDeque::new();
        roots.trace_roots(&mut |h| worklist.push_back(h));
        // Mark recursively from roots
        while let Some(handle) = worklist.pop_front() {
            if handle as usize >= self.marked.len() || self.marked[handle as usize] {
                continue;
            }
            self.marked[handle as usize] = true;
            if let Some(Some(obj)) = self.objects.get(handle as usize) {
                Self::trace_children(obj, &mut worklist);
            }
        }

        // Sweep phase
        for (i, obj) in self.objects.iter_mut().enumerate() {
            if self.pins[i] > 0 {
                continue;
            }
            if !self.marked[i] {
                if let Some(obj) = obj.take() {
                    self.stats.bytes_freed += GenerationalGc::estimate_size(&obj) as u64;
                    self.free_slots.push_back(i as GcHandle);
                }
            }
        }
    }

    fn pin(&mut self, handle: GcHandle) {
        if let Some(p) = self.pins.get_mut(handle as usize) {
            *p += 1;
        }
    }

    fn unpin(&mut self, handle: GcHandle) {
        if let Some(p) = self.pins.get_mut(handle as usize) {
            *p = p.saturating_sub(1);
        }
    }

    fn is_valid(&self, handle: GcHandle) -> bool {
        (handle as usize) < self.objects.len() && self.objects[handle as usize].is_some()
    }

    fn stats(&self) -> GcStats {
        let mut s = self.stats.clone();
        s.heap_size = self.objects.len();
        s
    }

    fn shutdown(&mut self) {
        self.objects.clear();
        self.free_slots.clear();
    }

    fn strategy_name(&self) -> &str {
        "mark-sweep"
    }

    fn write_barrier(&mut self, _target: GcHandle, _child: GcHandle) {
        // Mark-sweep is not generational — no write barrier needed
    }

    fn is_young(&self, _handle: GcHandle) -> bool {
        false // mark-sweep treats all objects equally
    }
}

impl MarkSweepGc {
    fn trace_children(obj: &HeapObject, worklist: &mut VecDeque<GcHandle>) {
        match obj {
            HeapObject::Array(elems) => {
                for val in elems {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
            }
            HeapObject::Map(entries) => {
                for (k, v) in entries {
                    if let UValue::Gc(h, _) = k { worklist.push_back(*h); }
                    if let UValue::Gc(h, _) = v { worklist.push_back(*h); }
                }
            }
            HeapObject::Closure { captures, module_id: _mid, .. } => {
                for val in captures {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
            }
            HeapObject::Struct(fields) => {
                for (_, val) in fields {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
            }
            HeapObject::Opaque { .. } => {}
            HeapObject::Namespace { members, .. } => {
                for (_, val) in members {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
            }
            HeapObject::Class { .. } => {}
            HeapObject::Object { fields, proto, .. } => {
                for val in fields {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
                if let Some(p) = proto {
                    worklist.push_back(*p);
                }
            }
            HeapObject::Method { object_handle, .. } => {
                worklist.push_back(*object_handle);
            }
            HeapObject::Dynamic(val) => {
                if let UValue::Gc(h, _) = val {
                    worklist.push_back(*h);
                }
            }
            HeapObject::Set(vals) => {
                for val in vals {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
            }
            HeapObject::Thunk { value, captures, .. } => {
                if let UValue::Gc(h, _) = value.as_ref() { worklist.push_back(*h); }
                for val in captures {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
            }
            HeapObject::HeapString(_) | HeapObject::BigInt(_) | HeapObject::Bytes(_) | HeapObject::ByteArray(_) | HeapObject::BoxedStructBytes(_) => {}
            HeapObject::Lambda { captures, .. } => {
                for val in captures {
                    if let UValue::Gc(h, _) = val { worklist.push_back(*h); }
                }
            }
            HeapObject::Iterator { container_handle, .. } => {
                worklist.push_back(*container_handle);
            }
            HeapObject::Pair { .. } | HeapObject::Tuple(_) | HeapObject::Range { .. } | HeapObject::Regex(_, _) | HeapObject::Continuation { .. } => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════
