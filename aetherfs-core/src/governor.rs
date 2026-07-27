use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Set the calling thread to background/idle priority based on the platform.
pub fn set_background_priority() {
    #[cfg(target_os = "linux")]
    unsafe {
        // Set nice value to 19 (lowest priority)
        libc::setpriority(libc::PRIO_PROCESS, 0, 19);

        // Attempt to set SCHED_IDLE scheduler
        let param = libc::sched_param { sched_priority: 0 };
        let result = libc::sched_setscheduler(0, libc::SCHED_IDLE, &param);
        if result != 0 {
            eprintln!("AetherFS Governor: Failed to set SCHED_IDLE scheduler, falling back to nice priority");
        } else {
            println!("AetherFS Governor: Thread set to SCHED_IDLE priority");
        }
    }

    #[cfg(target_os = "macos")]
    unsafe {
        // qos_class_utility() fallback via dispatch or pthread
        // In macOS, we can set thread policy or rely on nice value.
        libc::setpriority(libc::PRIO_PROCESS, 0, 19);
        println!("AetherFS Governor: Thread niceness set to 19 (macOS)");
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use winapi::um::processthreadsapi::{GetCurrentThread, SetThreadPriority};
        // THREAD_MODE_BACKGROUND_BEGIN = 0x00010000
        let success = SetThreadPriority(GetCurrentThread(), 0x00010000);
        if success == 0 {
            // Fallback to THREAD_PRIORITY_IDLE = -15
            SetThreadPriority(GetCurrentThread(), -15);
            println!("AetherFS Governor: Set thread to idle priority (Windows)");
        } else {
            println!("AetherFS Governor: Enabled background mode for thread (Windows)");
        }
    }
}

/// A Token-Bucket / Sleep-Ratio based CPU governor.
/// Ensures that a single-core worker thread does not exceed a specified CPU percentage (e.g., 35%).
pub struct CpuGovernor {
    target_cpu_limit: f64, // E.g., 0.35 for 35% CPU limit
    last_tick: Instant,
    accumulated_work_ms: f64,
}

impl CpuGovernor {
    /// Create a new CpuGovernor with a target CPU usage limit (e.g. 0.35 for 35%).
    pub fn new(target_cpu_limit: f64) -> Self {
        Self {
            target_cpu_limit: target_cpu_limit.clamp(0.01, 1.0),
            last_tick: Instant::now(),
            accumulated_work_ms: 0.0,
        }
    }

    /// Call this immediately before starting a chunk of work.
    pub fn start_work(&mut self) {
        self.last_tick = Instant::now();
    }

    /// Call this immediately after completing a chunk of work.
    /// This method will calculate the elapsed work time and sleep
    /// the current async task if needed to maintain the CPU limit.
    pub async fn end_work_and_throttle(&mut self) {
        let work_duration = self.last_tick.elapsed();
        let work_ms = work_duration.as_secs_f64() * 1000.0;
        self.accumulated_work_ms += work_ms;

        // If work took some measurable time, throttle
        if self.accumulated_work_ms > 1.0 {
            // Target ratio: work_ms / (work_ms + sleep_ms) = target_cpu_limit
            // => work_ms = target_cpu_limit * work_ms + target_cpu_limit * sleep_ms
            // => work_ms * (1.0 - target_cpu_limit) = target_cpu_limit * sleep_ms
            // => sleep_ms = work_ms * (1.0 - target_cpu_limit) / target_cpu_limit
            let sleep_ratio = (1.0 - self.target_cpu_limit) / self.target_cpu_limit;
            let sleep_ms = self.accumulated_work_ms * sleep_ratio;

            if sleep_ms > 1.0 {
                sleep(Duration::from_secs_f64(sleep_ms / 1000.0)).await;
            }
            
            // Reset accumulator
            self.accumulated_work_ms = 0.0;
        }
    }
}
