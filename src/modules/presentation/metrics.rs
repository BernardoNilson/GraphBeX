//! Brings the monitoring metrics into Bevy: the hardware samples, and the GPU
//! memory, which only the renderer can read.

use std::time::Duration;

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::time::common_conditions::on_timer;
use crossbeam_channel::Receiver;

use crate::modules::application::monitoring::HardwareSample;

pub struct MetricsPlugin(pub Receiver<HardwareSample>);

#[derive(Resource)]
struct HardwareChannel(Receiver<HardwareSample>);

/// The latest hardware sample, once the first one arrives.
#[derive(Resource, Default)]
pub struct HardwareMetrics(pub Option<HardwareSample>);

/// The GPU memory this application holds, in bytes, as tracked by wgpu.
#[derive(Resource, Default)]
pub struct GpuMemory {
    pub buffers: u64,
    pub textures: u64,
}

impl GpuMemory {
    pub fn total(&self) -> u64 {
        self.buffers + self.textures
    }

    /// A backend without counters never records anything.
    pub fn is_known(&self) -> bool {
        self.total() > 0
    }
}

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HardwareChannel(self.0.clone()))
            .init_resource::<HardwareMetrics>()
            .init_resource::<GpuMemory>()
            .add_systems(
                PreUpdate,
                (receive_hardware_samples, sample_gpu_memory.run_if(on_timer(Duration::from_secs(1)))),
            );
    }
}

fn receive_hardware_samples(channel: Res<HardwareChannel>, mut metrics: ResMut<HardwareMetrics>) {
    if let Some(sample) = channel.0.try_iter().last() {
        metrics.0 = Some(sample);
    }
}

fn sample_gpu_memory(device: Option<Res<RenderDevice>>, mut memory: ResMut<GpuMemory>) {
    if let Some(device) = device {
        let counters = device.wgpu_device().get_internal_counters().hal;
        memory.buffers = counters.buffer_memory.read().max(0) as u64;
        memory.textures = counters.texture_memory.read().max(0) as u64;
    }
}
