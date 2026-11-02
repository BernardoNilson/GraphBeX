//! The monitoring engine: samples CPU and RAM on its own thread, off the
//! rendering loop, and sends each sample through a channel.

use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, unbounded};
use sysinfo::{MINIMUM_CPU_UPDATE_INTERVAL, ProcessRefreshKind, ProcessesToUpdate, System};

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// The minimum requirements the TCC checks at startup.
pub const MINIMUM_CORES: usize = 4;
pub const MINIMUM_MEMORY: u64 = 8 * 1024 * 1024 * 1024;

/// One reading of the hardware. Memory is in bytes, usage in percent.
#[derive(Debug, Clone, Default)]
pub struct HardwareSample {
    pub physical_cores: Option<usize>,
    pub core_usage: Vec<f32>,
    pub cpu_usage: f32,
    pub process_memory: u64,
    pub used_memory: u64,
    pub total_memory: u64,
}

impl HardwareSample {
    pub fn logical_cores(&self) -> usize {
        self.core_usage.len()
    }

    /// Physical cores, falling back to logical ones.
    pub fn cores(&self) -> usize {
        self.physical_cores.unwrap_or(self.logical_cores())
    }

    pub fn meets_core_requirement(&self) -> bool {
        self.cores() >= MINIMUM_CORES
    }

    pub fn meets_memory_requirement(&self) -> bool {
        self.total_memory >= MINIMUM_MEMORY
    }
}

/// Starts sampling. The thread ends once the receiver is dropped.
pub fn start() -> Receiver<HardwareSample> {
    let (sender, receiver) = unbounded();

    thread::spawn(move || {
        let mut system = System::new();
        let process = sysinfo::get_current_pid().ok();
        let physical_cores = System::physical_core_count();

        // CPU usage is the difference between two readings.
        system.refresh_cpu_usage();
        thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);

        for index in 0.. {
            system.refresh_cpu_usage();
            system.refresh_memory();
            if let Some(process) = process {
                let memory = ProcessRefreshKind::nothing().with_memory();
                system.refresh_processes_specifics(ProcessesToUpdate::Some(&[process]), false, memory);
            }

            let sample = HardwareSample {
                physical_cores,
                core_usage: system.cpus().iter().map(|cpu| cpu.cpu_usage()).collect(),
                cpu_usage: system.global_cpu_usage(),
                process_memory: process.and_then(|pid| system.process(pid)).map_or(0, |p| p.memory()),
                used_memory: system.used_memory(),
                total_memory: system.total_memory(),
            };

            if index == 0 {
                log_requirements(&sample);
            }
            if sender.send(sample).is_err() {
                break;
            }

            thread::sleep(SAMPLE_INTERVAL);
        }
    });

    receiver
}

fn log_requirements(sample: &HardwareSample) {
    let cores = sample.cores();
    let memory = sample.total_memory as f64 / (1024.0 * 1024.0 * 1024.0);

    if sample.meets_core_requirement() && sample.meets_memory_requirement() {
        log::info!("Minimum requirements met: {cores} cores, {memory:.1} GiB of RAM.");
    } else {
        log::warn!(
            "Below the minimum requirements of {MINIMUM_CORES} cores and 8 GiB of RAM: \
             {cores} cores, {memory:.1} GiB. Large graphs may be slow."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_compare_against_the_minimum() {
        let sample = HardwareSample {
            physical_cores: Some(4),
            core_usage: vec![0.0; 8],
            total_memory: MINIMUM_MEMORY,
            ..HardwareSample::default()
        };
        assert!(sample.meets_core_requirement() && sample.meets_memory_requirement());

        let small = HardwareSample {
            physical_cores: None,
            core_usage: vec![0.0; 2],
            total_memory: MINIMUM_MEMORY - 1,
            ..HardwareSample::default()
        };
        assert!(!small.meets_core_requirement() && !small.meets_memory_requirement());
    }
}
