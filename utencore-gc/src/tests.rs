//! Garbage collector tests.

pub mod tests {
    use crate::memory::{GenerationalGc, MarkSweepGc, RefCountGc, GcEngine};
    use utencore_types::{HeapObject, UValue};

    struct NoopRoots;
    impl crate::TraceRoots for NoopRoots {
        fn trace_roots(&mut self, _tracer: &mut dyn FnMut(utencore_types::GcHandle)) {}
    }

    #[test]
    fn test_generational_gc_alloc() {
        let mut gc = GenerationalGc::new();
        let h = gc.alloc(HeapObject::Array(vec![]));
        match gc.get(h) { HeapObject::Array(_) => {}, _ => panic!("expected Array"), }
        assert!(gc.is_valid(h));
    }

    #[test]
    fn test_mark_sweep_gc_alloc() {
        let mut gc = MarkSweepGc::new();
        let h = gc.alloc(HeapObject::Map(std::collections::HashMap::new()));
        assert!(gc.is_valid(h));
    }

    #[test]
    fn test_refcount_gc_alloc() {
        let mut gc = RefCountGc::new();
        let h = gc.alloc(HeapObject::Array(vec![UValue::Int32(42)]));
        match gc.get(h) { HeapObject::Array(arr) => assert_eq!(arr.len(), 1), _ => panic!("expected Array"), }
    }

    #[test]
    fn test_gc_collect() {
        let mut gc = GenerationalGc::new();
        let h = gc.alloc(HeapObject::Array(vec![]));
        assert!(gc.is_valid(h));
        let mut roots = NoopRoots;
        gc.collect(&mut roots);
        // h is no longer reachable, collection may free it
    }

    #[test]
    fn test_gc_strategy_names() {
        assert_eq!(GenerationalGc::new().strategy_name(), "generational");
        assert_eq!(MarkSweepGc::new().strategy_name(), "mark-sweep");
        assert_eq!(RefCountGc::new().strategy_name(), "refcount");
    }

    #[test]
    fn test_many_allocations() {
        let mut gc = GenerationalGc::new();
        let mut handles = Vec::new();
        for i in 0..50 {
            let h = gc.alloc(HeapObject::Array(vec![UValue::Int32(i)]));
            handles.push(h);
        }
        for (i, h) in handles.iter().enumerate() {
            assert!(gc.is_valid(*h));
            match gc.get(*h) {
                HeapObject::Array(arr) => assert_eq!(arr.len(), 1),
                _ => panic!("expected Array"),
            }
        }
    }
}
