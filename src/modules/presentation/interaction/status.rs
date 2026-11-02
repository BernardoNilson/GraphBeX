//! Keeps the status bar, the selection, the performance panel and the window
//! title in sync with the engine and monitoring state.

use std::fmt::Write;
use std::time::Duration;

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::controls::UiSelection;
use super::screen::{EmptyCanvasHint, PerformancePanel, StatusText};
use super::theme::{Ink, Tone};
use super::widgets::shown;
use crate::modules::application::monitoring::{HardwareSample, MINIMUM_CORES};
use crate::modules::presentation::graph_state::{
    LastError, LayoutPositions, LayoutRunInfo, LayoutStatus, LoadedGraph, VertexSelection,
};
use crate::modules::presentation::metrics::{GpuMemory, HardwareMetrics};

pub const WINDOW_TITLE: &str = "GraphBeX - Graph Visualizer";

#[derive(SystemParam)]
pub struct StatusSources<'w> {
    loaded: Res<'w, LoadedGraph>,
    positions: Res<'w, LayoutPositions>,
    selection: Res<'w, VertexSelection>,
    status: Res<'w, LayoutStatus>,
    info: Res<'w, LayoutRunInfo>,
    last_error: Res<'w, LastError>,
    hardware: Res<'w, HardwareMetrics>,
    gpu: Res<'w, GpuMemory>,
    diagnostics: Res<'w, DiagnosticsStore>,
}

pub fn refresh_status_texts(sources: StatusSources, mut texts: Query<(&StatusText, &mut Text, &mut Ink)>) {
    let hardware = sources.hardware.0.as_ref();
    // The metrics are padded with figure spaces, as wide as a digit, so the bar
    // does not shift when a number grows.
    let metric = |name: &str, width: usize, value: Option<String>| {
        format!("{name} {:\u{2007}>width$}", value.as_deref().unwrap_or("--"))
    };

    let performance = performance_report(&sources);
    let (labels, values): (Vec<&str>, Vec<&str>) = performance.iter().map(|(l, v)| (*l, v.as_str())).unzip();

    for (kind, mut text, mut ink) in &mut texts {
        ink.set_if_neq(Ink(match kind {
            StatusText::Selection | StatusText::PerformanceValues => Tone::Text,
            StatusText::Layout if sources.last_error.0.is_some() => Tone::Error,
            _ => Tone::TextMuted,
        }));

        **text = match kind {
            StatusText::Graph => match (&sources.loaded.graph, sources.loaded.file_name()) {
                (Some(graph), Some(name)) => {
                    format!("{name}  {} vertices  {} edges", graph.node_count(), graph.edge_count())
                }
                _ => "No graph".to_string(),
            },
            StatusText::Layout => match (&sources.last_error.0, sources.info.algorithm) {
                (Some(message), _) => format!("Error: {message}"),
                (None, Some(algorithm)) => format!("{algorithm}: {:?}", *sources.status),
                (None, None) => format!("{:?}", *sources.status),
            },
            StatusText::Cpu => metric("CPU", 4, hardware.map(|sample| format!("{:.0}%", sample.cpu_usage))),
            StatusText::Ram => metric("RAM", 7, hardware.map(|sample| bytes(sample.process_memory))),
            StatusText::Vram => metric("VRAM", 7, sources.gpu.is_known().then(|| bytes(sources.gpu.total()))),
            StatusText::Fps => metric("FPS", 3, fps(&sources.diagnostics).map(|fps| format!("{fps:.0}"))),
            StatusText::Iterations => metric("Iter", 6, Some(sources.info.iterations.to_string())),
            StatusText::Selection => selection_report(&sources),
            StatusText::PerformanceLabels => labels.join("\n"),
            StatusText::PerformanceValues => values.join("\n"),
        };
    }
}

/// The identifier, degree and position of the selected vertex (FR6). Its
/// neighbors are highlighted on the canvas.
fn selection_report(sources: &StatusSources) -> String {
    let (Some(vertex), Some(graph)) = (sources.selection.0, &sources.loaded.graph) else {
        return "Click a vertex to select it".to_string();
    };
    let p = sources.positions.positions.get(vertex as usize).copied().unwrap_or_default();

    format!("id: {vertex}\ndegree: {}\nposition: ({:.1}, {:.1}, {:.1})", graph.degree(vertex), p.x, p.y, p.z)
}

/// The detailed figures of the monitoring section of the TCC, as rows of a
/// label and a value.
fn performance_report(sources: &StatusSources) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();

    if let Some(sample) = sources.hardware.0.as_ref() {
        let cores: Vec<String> = sample.core_usage.iter().map(|usage| format!("{usage:.0}")).collect();
        let mark = |met: bool| if met { "met" } else { "not met" };
        let (core, memory) = (mark(sample.meets_core_requirement()), mark(sample.meets_memory_requirement()));

        rows.extend([
            ("CPU", format!("{:.0}%", sample.cpu_usage)),
            ("Per core %", cores.join(" ")),
            ("Cores", core_count(sample)),
            ("RAM process", bytes(sample.process_memory)),
            ("RAM system", format!("{} / {}", bytes(sample.used_memory), bytes(sample.total_memory))),
            ("Requirements", format!("{MINIMUM_CORES}+ cores {core}, 8 GB RAM {memory}")),
        ]);
    }

    let gpu = &sources.gpu;
    if gpu.is_known() {
        let (total, buffers, textures) = (bytes(gpu.total()), bytes(gpu.buffers), bytes(gpu.textures));
        rows.push(("VRAM", format!("{total} (buffers {buffers}, textures {textures})")));
    }

    let frame_time = sources.diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed());
    if let Some(frame_time) = frame_time {
        rows.push(("Frame time", format!("{frame_time:.1} ms")));
    }

    if let Some(load_time) = sources.loaded.load_time {
        rows.push(("Graph load", duration(load_time)));
    }

    let info = &sources.info;
    if let Some(algorithm) = info.algorithm {
        let mut layout = format!("{algorithm}, {} iterations", info.iterations);
        if let Some(elapsed) = info.elapsed {
            let each = elapsed / info.iterations.max(1) as u32;
            let _ = write!(layout, ", {} ({} each)", duration(elapsed), duration(each));
        }
        rows.push(("Layout", layout));
    }

    rows
}

fn core_count(sample: &HardwareSample) -> String {
    match sample.physical_cores {
        Some(physical) => format!("{physical} physical, {} logical", sample.logical_cores()),
        None => format!("{} logical", sample.logical_cores()),
    }
}

fn fps(diagnostics: &DiagnosticsStore) -> Option<f64> {
    diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|diagnostic| diagnostic.smoothed())
}

fn bytes(value: u64) -> String {
    let megabytes = value as f64 / (1024.0 * 1024.0);
    if megabytes >= 1024.0 { format!("{:.1} GB", megabytes / 1024.0) } else { format!("{megabytes:.0} MB") }
}

fn duration(value: Duration) -> String {
    let seconds = value.as_secs_f64();
    if seconds >= 1.0 { format!("{seconds:.2} s") } else { format!("{:.1} ms", seconds * 1000.0) }
}

/// Shows the loaded file in the title bar, as in the mockup.
pub fn update_window_title(loaded: Res<LoadedGraph>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    let title = match loaded.file_name() {
        Some(name) => format!("{name} - {WINDOW_TITLE}"),
        None => WINDOW_TITLE.to_string(),
    };
    windows.iter_mut().for_each(|mut window| window.title = title.clone());
}

pub fn update_canvas_overlays(
    loaded: Res<LoadedGraph>,
    selection: Res<UiSelection>,
    mut hints: Query<&mut Visibility, (With<EmptyCanvasHint>, Without<PerformancePanel>)>,
    mut panels: Query<&mut Visibility, With<PerformancePanel>>,
) {
    hints.iter_mut().for_each(|mut visibility| *visibility = shown(loaded.graph.is_none()));
    panels.iter_mut().for_each(|mut visibility| *visibility = shown(selection.performance_panel));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_durations_read_naturally() {
        assert_eq!(bytes(300 * 1024 * 1024), "300 MB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(duration(Duration::from_millis(12)), "12.0 ms");
        assert_eq!(duration(Duration::from_millis(2500)), "2.50 s");
    }
}
