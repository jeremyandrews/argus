use crate::START_TIME;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System}; // Updated imports

pub struct SystemInfo {
    pub memory_usage: u64,
    pub memory_total: u64,
    pub cpu_usage: f32,
    pub uptime: u64,
    pub thread_count: usize,
}

impl SystemInfo {
    pub fn collect() -> Self {
        // Create with specific refresh kinds
        let mut sys = System::new_with_specifics(
            RefreshKind::everything() // Changed from new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );

        // Refresh system info
        sys.refresh_memory();
        sys.refresh_cpu_all(); // Changed from refresh_cpu()

        let memory_total = sys.total_memory();
        let memory_usage = sys.used_memory();
        let cpu_usage = sys.global_cpu_usage(); // Changed from global_cpu_info().cpu_usage()

        let start_time = START_TIME.load(Ordering::SeqCst);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let uptime = now.saturating_sub(start_time);
        let thread_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        SystemInfo {
            memory_usage,
            memory_total,
            cpu_usage,
            uptime,
            thread_count,
        }
    }
}
