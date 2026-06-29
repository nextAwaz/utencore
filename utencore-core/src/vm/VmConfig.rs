//! VM configuration.

use super::*;

#[derive(Debug, Clone)]
pub struct VmConfig {
    pub stack_size: usize,
    pub frame_size: usize,
    pub jit_enabled: bool,
    pub jit_threshold: u32,
    pub gc_interval: u32,
    pub max_recursion: u32,
}

impl Default for VmConfig {
    fn default() -> Self {
        VmConfig {
            stack_size: 1024 * 1024,
            frame_size: 1024,
            jit_enabled: true,
            jit_threshold: 100,
            gc_interval: 10000,
            max_recursion: 1000,
        }
    }
}
