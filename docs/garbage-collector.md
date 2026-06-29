# Garbage Collector

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

The VM supports three pluggable GC strategies, selectable per-module via the module header's `gc_strategy` field.

## GcEngine Trait

```rust
pub trait GcEngine: Send {
    fn alloc(&mut self, obj: HeapObject) -> GcHandle;
    fn get(&self, handle: GcHandle) -> &HeapObject;
    fn get_mut(&mut self, handle: GcHandle) -> &mut HeapObject;
    fn collect(&mut self, roots: &mut dyn TraceRoots);
    fn pin(&mut self, handle: GcHandle);
    fn unpin(&mut self, handle: GcHandle);
    fn is_valid(&self, handle: GcHandle) -> bool;
    fn stats(&self) -> GcStats;
    fn strategy_name(&self) -> &str;
    fn shutdown(&mut self);
    fn write_barrier(&mut self, target: GcHandle, child: GcHandle);
    fn is_young(&self, handle: GcHandle) -> bool;
}
```

Root scanning is provided by `Vm` implementing `TraceRoots`, which traces:
- VM stack values
- Frame locals and captures
- Module globals

## Generational GC (Default)

Two-generation heap with write barrier:

- **Gen0 (nursery)**: ~10,000 object capacity. Collected frequently.
- **Gen1 (tenured)**: Objects promoted after surviving 2 gen0 collections.
- **Major GC**: Triggers when gen0 fills 5 times without promotion cycle.
- **Remembered set**: Tracks gen1→gen0 references for minor collections.
- **Promotion**: Objects surviving N collections move to gen1.

## Mark-Sweep GC

Simple stop-the-world collector:
1. **Mark phase**: Trace from roots, mark reachable objects
2. **Sweep phase**: Free unmarked objects, reclaim slots

## RefCount GC

Reference counting without cycle detection. Every pointer store adjusts reference counts. No cycle collection — use generational or mark-sweep for cyclic workloads.

## Pinning

Pinned objects (`gc_pin`/`gc_unpin`) are never collected or moved. Useful for objects referenced by native code via CIB.

## GC Statistics

```rust
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
```

## Known Limitations

1. **No compacting GC** — The generational collector does not compact the heap. Fragmentation may occur under sustained allocation.
2. **No cycle collection in RefCount** — Use generational or mark-sweep for cyclic data.
3. **Single-threaded** — GC is not thread-safe. All collection happens on the mutator thread.
