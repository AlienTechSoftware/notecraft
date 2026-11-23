//! Live CPU and memory of this process while the lab runs, drawn as small charts. Sampled about
//! twice a second; the charts keep the last two minutes.

use eframe::egui;
use egui::{Color32, Pos2, Stroke, Vec2};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessesToUpdate, System};

const SAMPLE_EVERY: Duration = Duration::from_millis(500);
const KEEP: usize = 240;

#[derive(Clone, Copy)]
struct Sample {
    cpu_percent: f32,
    memory_mb: f32,
}

pub struct Metrics {
    system: Option<System>,
    pid: Pid,
    samples: VecDeque<Sample>,
    last: Option<Instant>,
}

impl Metrics {
    pub fn new() -> Metrics {
        Metrics { system: System::new().ok(), pid: Pid::from_u32(std::process::id()), samples: VecDeque::new(), last: None }
    }

    /// Takes a sample when the last one is old enough.
    pub fn tick(&mut self) {
        if self.last.is_some_and(|t| t.elapsed() < SAMPLE_EVERY) {
            return;
        }
        self.last = Some(Instant::now());
        let Some(system) = self.system.as_mut() else { return };
        system.refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);
        if let Some(process) = system.process(self.pid) {
            if self.samples.len() == KEEP {
                self.samples.pop_front();
            }
            self.samples.push_back(Sample { cpu_percent: process.cpu_usage(), memory_mb: process.memory() as f32 / (1024.0 * 1024.0) });
        }
    }

    /// Draws the two charts and their current values.
    pub fn show(&self, ui: &mut egui::Ui) {
        if self.system.is_none() {
            ui.label("Metrics unavailable on this system.");
            return;
        }
        let Some(latest) = self.samples.back().copied() else {
            ui.label("Sampling…");
            return;
        };
        let cpu_max = self.samples.iter().map(|s| s.cpu_percent).fold(100.0_f32, f32::max);
        let mem_max = self.samples.iter().map(|s| s.memory_mb).fold(1.0_f32, f32::max);
        ui.label(format!("CPU {:.0}% (peak {:.0}%)", latest.cpu_percent, cpu_max));
        chart(ui, self.samples.iter().map(|s| s.cpu_percent), cpu_max, Color32::from_rgb(90, 170, 255));
        ui.label(format!("Memory {:.0} MB (peak {:.0} MB)", latest.memory_mb, mem_max));
        chart(ui, self.samples.iter().map(|s| s.memory_mb), mem_max, Color32::from_rgb(160, 120, 230));
    }
}

fn chart(ui: &mut egui::Ui, values: impl Iterator<Item = f32>, top: f32, colour: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let values: Vec<f32> = values.collect();
    if values.len() < 2 {
        return;
    }
    let step = rect.width() / (KEEP as f32 - 1.0);
    let x0 = rect.right() - step * (values.len() as f32 - 1.0);
    let points: Vec<Pos2> =
        values.iter().enumerate().map(|(i, v)| Pos2::new(x0 + step * i as f32, rect.bottom() - (v / top).clamp(0.0, 1.0) * rect.height())).collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, colour)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_twice_quickly_takes_one_sample_and_keeps_the_limit() {
        let mut metrics = Metrics::new();
        metrics.tick();
        metrics.tick();
        assert!(metrics.samples.len() <= 1, "the second tick is too soon to sample");
        for _ in 0..(KEEP + 5) {
            metrics.last = None;
            metrics.tick();
        }
        assert!(metrics.samples.len() <= KEEP, "history stays within {KEEP} samples");
    }
}
