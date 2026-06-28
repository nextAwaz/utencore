//! UtenCore Garbage Collection system.
//!
//! Provides a pluggable GC trait with multiple implementations:
//! - `GenerationalGc` (default) — generational GC
//! - `MarkSweepGc` — simple mark-and-sweep
//! - `RefCountGc` — reference counting
//!
//! The GC strategy is selectable by the compiler via `.uclib` header.

use std::collections::{HashSet, VecDeque, HashMap};
use utencore_types::*;

/// Sub-modules for alternative GC implementations
pub mod mark_sweep;
pub mod refcount;
pub use mark_sweep::MarkSweepGc;
pub use refcount::RefCountGc;

/// A handle to a GC-managed heap object
/// GC statistics
#[derive(Debug, Clone, Default)]
pub struct GcStats {
    pub total_allocations: u64,
    pub total_collections: u64,
    pub bytes_allocated: u64,
    pub bytes_freed: u64,
    pub heap_size: usize,
    pub gen0_collections: u64,
    pub gen1_collections: u64,
    pub promoted_objects: u64,
}

/// Trait for types that can trace their GC roots.
/// Implemented by Vm to allow GC to find all live references.
pub trait TraceRoots {
    /// Visit all GC handles that are root references.
    fn trace_roots(&mut self, tracer: &mut dyn FnMut(GcHandle));
}

/// The GC strategy identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcStrategy {
    Generational,
    MarkSweep,
    RefCount,
    None,
}

impl GcStrategy {
    pub fn from_str(s: &str) -> Self {
        match s {
            "mark-sweep" => GcStrategy::MarkSweep,
            "refcount" => GcStrategy::RefCount,
            "none" => GcStrategy::None,
            _ => GcStrategy::Generational,
        }
    }
}

/// Trait that all GC engines must implement
pub trait GcEngine: Send {
    /// Allocate a new heap object
    fn alloc(&mut self, obj: HeapObject) -> GcHandle;

    /// Get a reference to a heap object (read-only)
    fn get(&self, handle: GcHandle) -> &HeapObject;

    /// Get a mutable reference to a heap object
    fn get_mut(&mut self, handle: GcHandle) -> &mut HeapObject;

    /// Trigger a garbage collection cycle
    fn collect(&mut self, roots: &mut dyn TraceRoots);

    /// Pin an object (prevent collection)
    fn pin(&mut self, handle: GcHandle);

    /// Unpin an object
    fn unpin(&mut self, handle: GcHandle);

    /// Check if a handle is valid
    fn is_valid(&self, handle: GcHandle) -> bool;

    /// Get GC statistics
    fn stats(&self) -> GcStats;

    /// Get the strategy name (for matching module headers)
    fn strategy_name(&self) -> &str;

    /// Shutdown the GC
    fn shutdown(&mut self);

    /// Write barrier: called after a pointer store at `target` that now references `child`.
    /// The GC uses this to track old→young references for generational collection.
    fn write_barrier(&mut self, target: GcHandle, child: GcHandle);

    /// Returns true if `handle` is in the young generation (gen0).
    fn is_young(&self, handle: GcHandle) -> bool;
}

/// GC root — a value that must be kept alive
#[derive(Clone)]
pub enum GcRoot {
    Stack(u32),      // stack slot index
    Frame(usize, u16), // (frame_index, local_index)
    Global(ModuleId, u16),
    Pinned(GcHandle),
}

// ═══════════════════════════════════════════════════════════
// Generational GC (default)
// ═══════════════════════════════════════════════════════════

/// The default generational GC implementation.
///
/// Two generations:
/// - Gen 0: young objects (nursery). Collected frequently.
/// - Gen 1: old objects (tenured). Collected less frequently.
///
/// Objects surviving N gen0 collections are promoted to gen1.
pub struct GenerationalGc {
    /// All heap objects (both generations)
    objects: Vec<Option<HeapObject>>,
    /// Free slots for reuse
    free_slots: VecDeque<GcHandle>,
    /// Generation of each object (0 or 1)
    generations: Vec<u8>,
    /// Pin count for each object
    pins: Vec<u32>,
    /// Objects promoted to gen1
    promoted: Vec<GcHandle>,
    // Stats
    stats: GcStats,
    // Config
    promotion_threshold: u8,   // survivals before promotion
    gen0_capacity: usize,      // max gen0 objects before collection
    gen0_collections_since_promotion: u32,
    // Survival tracking
    survivals: Vec<u8>,         // how many collections each object survived
    /// Remembered set: gen1 handles that may reference gen0 objects.
    /// Used to ensure gen0 is fully traced during minor collections.
    remembered_set: HashSet<GcHandle>,
    /// Quick check: which gen0 handles are known to be referenced from gen1.
    /// Populated from remembered_set during gen0-only collections.
    cross_gen_refs: HashSet<GcHandle>,
}

impl GenerationalGc {
    pub fn new() -> Self {
        GenerationalGc {
            objects: Vec::with_capacity(1024),
            free_slots: VecDeque::new(),
            generations: Vec::new(),
            pins: Vec::new(),
            promoted: Vec::new(),
            stats: GcStats::default(),
            promotion_threshold: 2,
            gen0_capacity: 10000,
            gen0_collections_since_promotion: 0,
            survivals: Vec::new(),
            remembered_set: HashSet::new(),
            cross_gen_refs: HashSet::new(),
        }
    }

    fn alloc_slot(&mut self) -> GcHandle {
        if let Some(free) = self.free_slots.pop_front() {
            free
        } else {
            let handle = self.objects.len() as GcHandle;
            self.objects.push(None);
            self.generations.push(0);
            self.pins.push(0);
            self.survivals.push(0);
            handle
        }
    }

    /// Find roots from VM stack by calling TraceRoots::trace_roots.
    fn find_roots(&self, roots: &mut dyn TraceRoots) -> Vec<GcHandle> {
        let mut root_set = HashSet::new();
        let mut tracer = |h: GcHandle| { root_set.insert(h); };
        roots.trace_roots(&mut tracer);
        root_set.into_iter().collect()
    }

    /// Trace reachable objects from roots
    fn trace(&self, roots: &[GcHandle]) -> HashSet<GcHandle> {
        let mut reachable = HashSet::new();
        let mut worklist: VecDeque<GcHandle> = roots.iter().copied().collect();

        while let Some(handle) = worklist.pop_front() {
            if !reachable.insert(handle) {
                continue;
            }
            if let Some(Some(obj)) = self.objects.get(handle as usize) {
                // Trace references from this object
                match obj {
                    HeapObject::Array(elements) => {
                        for val in elements {
                            if let UValue::Gc(child, _) = val {
                                worklist.push_back(*child);
                            }
                        }
                    }
                    HeapObject::Map(entries) => {
                        for (k, v) in entries.iter() {
                            if let UValue::Gc(child, _) = k {
                                worklist.push_back(*child);
                            }
                            if let UValue::Gc(child, _) = v {
                                worklist.push_back(*child);
                            }
                        }
                    }
                    HeapObject::Closure { captures, module_id: _mid, .. } => {
                        for val in captures {
                            if let UValue::Gc(child, _) = val {
                                worklist.push_back(*child);
                            }
                        }
                    }
                    HeapObject::Struct(fields) => {
                        for (_, val) in fields {
                            if let UValue::Gc(child, _) = val {
                                worklist.push_back(*child);
                            }
                        }
                    }
                    HeapObject::Opaque { .. } => {}
                    HeapObject::Namespace { members, .. } => {
                        for (_, val) in members {
                            if let UValue::Gc(child, _) = val {
                                worklist.push_back(*child);
                            }
                        }
                    }
                    HeapObject::Class { methods, .. } => {
                        // Class handles are tracked via namespace/stack
                    }
                    HeapObject::Object { fields, proto, .. } => {
                        for val in fields {
                            if let UValue::Gc(child, _) = val {
                                worklist.push_back(*child);
                            }
                        }
                        if let Some(p) = proto {
                            worklist.push_back(*p);
                        }
                    }
                    HeapObject::Method { object_handle, .. } => {
                        worklist.push_back(*object_handle);
                    }
                    HeapObject::Dynamic(val) => {
                        if let UValue::Gc(child, _) = val {
                            worklist.push_back(*child);
                        }
                    }
                    HeapObject::Pair { car, cdr } => {
                        if let UValue::Gc(child, _) = car.as_ref() { worklist.push_back(*child); }
                        if let UValue::Gc(child, _) = cdr.as_ref() { worklist.push_back(*child); }
                    }
                    HeapObject::Tuple(elements) => {
                        for val in elements {
                            if let UValue::Gc(child, _) = val { worklist.push_back(*child); }
                        }
                    }
                    HeapObject::Range { start, end, step, .. } => {
                        if let UValue::Gc(c, _) = start.as_ref() { worklist.push_back(*c); }
                        if let UValue::Gc(c, _) = end.as_ref() { worklist.push_back(*c); }
                        if let UValue::Gc(c, _) = step.as_ref() { worklist.push_back(*c); }
                    }
                    HeapObject::Continuation { saved_stack, .. } => {
                        for val in saved_stack {
                            if let UValue::Gc(child, _) = val { worklist.push_back(*child); }
                        }
                    }
                    HeapObject::Set(vals) => {
                        for val in vals {
                            if let UValue::Gc(child, _) = val { worklist.push_back(*child); }
                        }
                    }
                    HeapObject::Thunk { value, captures, .. } => {
                        if let UValue::Gc(child, _) = value.as_ref() { worklist.push_back(*child); }
                        for val in captures {
                            if let UValue::Gc(child, _) = val { worklist.push_back(*child); }
                        }
                    }
                    HeapObject::Regex(_, _) => {}
                    HeapObject::HeapString(_) | HeapObject::BigInt(_) | HeapObject::Bytes(_) | HeapObject::ByteArray(_) | HeapObject::BoxedStructBytes(_) => {}
                    HeapObject::Lambda { captures, .. } => {
                        for val in captures {
                            if let UValue::Gc(child, _) = val { worklist.push_back(*child); }
                        }
                    }
                    HeapObject::Iterator { container_handle, .. } => {
                        worklist.push_back(*container_handle);
                    }
                }
            }
        }

        reachable
    }

    fn collect_gen0(&mut self, roots: &mut dyn TraceRoots) {
        self.stats.gen0_collections += 1;
        self.gen0_collections_since_promotion += 1;

        // Roots = VM roots + remembered set (gen1→gen0 cross-references)
        let mut root_set: HashSet<GcHandle> = self.find_roots(roots).into_iter().collect();
        // Add remembered set as additional roots for minor GC
        for &h in &self.remembered_set {
            root_set.insert(h);
        }
        let roots: Vec<GcHandle> = root_set.into_iter().collect();
        let reachable = self.trace(&roots);

        // Sweep gen0: collect unreachable objects
        for (i, obj) in self.objects.iter_mut().enumerate() {
            let handle = i as GcHandle;
            if self.generations[i] != 0 {
                continue; // skip gen1
            }
            if self.pins[i] > 0 {
                continue; // pinned objects are never collected
            }

            if !reachable.contains(&handle) {
                // Collect this object
                if let Some(obj) = obj.take() {
                    self.stats.bytes_freed += Self::estimate_size(&obj) as u64;
                    self.free_slots.push_back(handle);
                }
            } else {
                // Survived -> track survival
                self.survivals[i] += 1;
                if self.survivals[i] >= self.promotion_threshold {
                    // Promote to gen1
                    self.generations[i] = 1;
                    self.promoted.push(handle);
                    // Remove from cross_gen_refs since both are now gen1
                    self.cross_gen_refs.remove(&handle);
                }
            }
        }

        // Clear remembered set entries whose target is still valid gen1
        // (the set is repopulated by write_barrier for future stores)
        self.remembered_set.retain(|&h| {
            self.generations.get(h as usize).copied() == Some(1) && self.objects[h as usize].is_some()
        });
    }

    fn collect_gen1(&mut self, roots: &mut dyn TraceRoots) {
        self.stats.gen1_collections += 1;

        let roots = self.find_roots(roots);
        let reachable = self.trace(&roots);

        // Sweep both generations
        for (i, obj) in self.objects.iter_mut().enumerate() {
            let handle = i as GcHandle;
            if self.pins[i] > 0 {
                continue;
            }
            if !reachable.contains(&handle) {
                if let Some(obj) = obj.take() {
                    self.stats.bytes_freed += Self::estimate_size(&obj) as u64;
                    self.free_slots.push_back(handle);
                }
            }
        }
    }

    fn estimate_size(obj: &HeapObject) -> usize {
        match obj {
            HeapObject::Array(elems) => std::mem::size_of::<UValue>() * elems.len() + 32,
            HeapObject::Map(entries) => std::mem::size_of::<(UValue, UValue)>() * entries.len() + 32,
            HeapObject::Struct(fields) => std::mem::size_of::<(StringId, UValue)>() * fields.len() + 32,
            HeapObject::Closure { captures, module_id: _mid, .. } => std::mem::size_of::<UValue>() * captures.len() + 32,
            HeapObject::Opaque { data, .. } => data.len() + 32,
            HeapObject::Namespace { members, .. } => std::mem::size_of::<(StringId, UValue)>() * members.len() + 32,
            HeapObject::Class { methods, fields, .. } => {
                std::mem::size_of::<(StringId, FuncRef)>() * methods.len()
                + std::mem::size_of::<StringId>() * fields.len() + 32
            }
            HeapObject::Object { fields, .. } => std::mem::size_of::<UValue>() * fields.len() + 32,
            HeapObject::Method { .. } => 32,
            HeapObject::Dynamic(val) => std::mem::size_of::<UValue>() + 32,
            HeapObject::Pair { .. } => 64,
            HeapObject::Tuple(elems) => std::mem::size_of::<UValue>() * elems.len() + 32,
            HeapObject::Range { .. } => 64,
            HeapObject::Regex(pattern, compiled) => pattern.len() + compiled.len() + 32,
            HeapObject::Continuation { saved_frames, saved_stack, .. } => {
                std::mem::size_of::<SavedFrame>() * saved_frames.len()
                + std::mem::size_of::<UValue>() * saved_stack.len() + 32
            }
            HeapObject::Set(vals) => std::mem::size_of::<UValue>() * vals.len() + 32,
            HeapObject::Thunk { captures, .. } => std::mem::size_of::<UValue>() * captures.len() + 32,
            HeapObject::HeapString(s) => s.len() + 32,
            HeapObject::BigInt(bi) => (bi.bits() / 8) as usize + 32,
            HeapObject::Bytes(b) | HeapObject::ByteArray(b) => b.len() + 32,
            HeapObject::BoxedStructBytes(b) => b.len() + 32,
            HeapObject::Lambda { captures, .. } => std::mem::size_of::<UValue>() * captures.len() + 32,
            HeapObject::Iterator { .. } => 32,
        }
    }
}

impl GcEngine for GenerationalGc {
    fn alloc(&mut self, obj: HeapObject) -> GcHandle {
        let handle = self.alloc_slot();
        self.objects[handle as usize] = Some(obj);
        self.generations[handle as usize] = 0; // always born in gen0
        self.stats.total_allocations += 1;
        handle
    }

    fn get(&self, handle: GcHandle) -> &HeapObject {
        self.objects[handle as usize].as_ref()
            .expect("use-after-free in GC")
    }

    fn get_mut(&mut self, handle: GcHandle) -> &mut HeapObject {
        self.objects[handle as usize].as_mut()
            .expect("use-after-free in GC")
    }

    fn collect(&mut self, roots: &mut dyn TraceRoots) {
        self.stats.total_collections += 1;

        // Count gen0 objects
        let gen0_count = self.generations.iter().filter(|&&g| g == 0).count();

        if gen0_count >= self.gen0_capacity || self.gen0_collections_since_promotion >= 5 {
            // Full collection (major)
            self.collect_gen1(roots);
            self.gen0_collections_since_promotion = 0;
        } else {
            // Minor collection (gen0 only)
            self.collect_gen0(roots);
        }
    }

    fn pin(&mut self, handle: GcHandle) {
        if let Some(pin) = self.pins.get_mut(handle as usize) {
            *pin += 1;
        }
    }

    fn unpin(&mut self, handle: GcHandle) {
        if let Some(pin) = self.pins.get_mut(handle as usize) {
            *pin = pin.saturating_sub(1);
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
        "generational"
    }

    fn write_barrier(&mut self, target: GcHandle, child: GcHandle) {
        // Only care about old→young stores
        let t_gen = self.generations.get(target as usize).copied().unwrap_or(0);
        let c_gen = self.generations.get(child as usize).copied().unwrap_or(0);
        if t_gen >= 1 && c_gen == 0 {
            self.remembered_set.insert(target);
            self.cross_gen_refs.insert(child);
        }
    }

    fn is_young(&self, handle: GcHandle) -> bool {
        self.generations.get(handle as usize).copied() == Some(0)
    }
}

// ═══════════════════════════════════════════════════════════
