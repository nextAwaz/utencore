// Reference Counting GC
// ═══════════════════════════════════════════════════════════

use std::collections::VecDeque;
use utencore_types::{HeapObject, UValue, ValueTag, FuncRef, ModuleId};
use super::{GcEngine, GcHandle, GcStats, GcRoot, GenerationalGc, TraceRoots};

pub struct RefCountGc {
    objects: Vec<Option<HeapObject>>,
    refcounts: Vec<u32>,
    free_slots: VecDeque<GcHandle>,
    stats: GcStats,
}

impl RefCountGc {
    pub fn new() -> Self {
        RefCountGc {
            objects: Vec::new(),
            refcounts: Vec::new(),
            free_slots: VecDeque::new(),
            stats: GcStats::default(),
        }
    }

    fn retain(&mut self, handle: GcHandle) {
        if let Some(rc) = self.refcounts.get_mut(handle as usize) {
            *rc += 1;
        }
    }

    fn release(&mut self, handle: GcHandle) {
        if let Some(rc) = self.refcounts.get_mut(handle as usize) {
            *rc = rc.saturating_sub(1);
            if *rc == 0 {
                // Free the object
                if let Some(obj) = self.objects[handle as usize].take() {
                    // Release children first
                    Self::release_children(&obj, &mut |child| {
                        // Would need access to self; simplified for now
                    });
                    self.stats.bytes_freed += GenerationalGc::estimate_size(&obj) as u64;
                    self.free_slots.push_back(handle);
                }
            }
        }
    }

    fn release_children<F>(obj: &HeapObject, f: &mut F)
    where F: FnMut(GcHandle)
    {
        match obj {
            HeapObject::Array(elems) => {
                for val in elems {
                    if let UValue::Gc(h, _) = val { f(*h); }
                }
            }
            HeapObject::Map(entries) => {
                for (k, v) in entries.iter() {
                    if let UValue::Gc(h, _) = k { f(*h); }
                    if let UValue::Gc(h, _) = v { f(*h); }
                }
            }
            HeapObject::Closure { captures, .. } => {
                for val in captures { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Struct(fields) => {
                for (_, val) in fields { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Namespace { members, .. } => {
                for (_, val) in members { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Object { fields, proto, .. } => {
                for val in fields { if let UValue::Gc(h, _) = val { f(*h); } }
                if let Some(p) = proto { f(*p); }
            }
            HeapObject::Method { object_handle, .. } => f(*object_handle),
            HeapObject::Dynamic(val) => { if let UValue::Gc(h, _) = val { f(*h); } }
            HeapObject::Pair { car, cdr } => {
                if let UValue::Gc(h, _) = car.as_ref() { f(*h); }
                if let UValue::Gc(h, _) = cdr.as_ref() { f(*h); }
            }
            HeapObject::Tuple(elems) => {
                for val in elems { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Range { start, end, step, .. } => {
                if let UValue::Gc(h, _) = start.as_ref() { f(*h); }
                if let UValue::Gc(h, _) = end.as_ref() { f(*h); }
                if let UValue::Gc(h, _) = step.as_ref() { f(*h); }
            }
            HeapObject::Continuation { saved_frames, saved_stack, .. } => {
                for val in saved_frames {
                    for local in &val.locals { if let UValue::Gc(h, _) = local { f(*h); } }
                    for cap in &val.captures { if let UValue::Gc(h, _) = cap { f(*h); } }
                }
                for val in saved_stack { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Set(vals) => {
                for val in vals { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Thunk { value, captures, .. } => {
                if let UValue::Gc(h, _) = value.as_ref() { f(*h); }
                for val in captures { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Array(elems) => {
                for val in elems { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Lambda { captures, .. } => {
                for val in captures { if let UValue::Gc(h, _) = val { f(*h); } }
            }
            HeapObject::Iterator { container_handle, .. } => f(*container_handle),
            HeapObject::Opaque { .. } | HeapObject::Class { .. } | HeapObject::Regex(_, _) | HeapObject::HeapString(_) | HeapObject::BigInt(_) | HeapObject::Bytes(_) | HeapObject::ByteArray(_) | HeapObject::BoxedStructBytes(_) => {}
        }
    }
}

impl GcEngine for RefCountGc {
    fn alloc(&mut self, obj: HeapObject) -> GcHandle {
        let handle = if let Some(free) = self.free_slots.pop_front() {
            self.objects[free as usize] = Some(obj);
            self.refcounts[free as usize] = 1;
            free
        } else {
            let handle = self.objects.len() as GcHandle;
            self.objects.push(Some(obj));
            self.refcounts.push(1);
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

    fn collect(&mut self, _roots: &mut dyn TraceRoots) {
        // Ref counting is incremental; collect cycles would need
        // a cycle collector (simplified: no cycle collection for now)
        self.stats.total_collections += 1;
    }

    fn pin(&mut self, handle: GcHandle) {
        self.refcounts[handle as usize] += 10000; // effectively pin
    }

    fn unpin(&mut self, handle: GcHandle) {
        self.refcounts[handle as usize] = self.refcounts[handle as usize].saturating_sub(10000);
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
    }

    fn strategy_name(&self) -> &str {
        "refcount"
    }

    fn write_barrier(&mut self, _target: GcHandle, _child: GcHandle) {
        // RefCount is not generational — no write barrier needed
    }

    fn is_young(&self, _handle: GcHandle) -> bool {
        false
    }
}