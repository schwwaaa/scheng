use std::time::{Duration, Instant};

use scheng_runtime::MatrixPreset;

// fn saturate(x: f32) -> f32 {
//     if x < 0.0 {
//         0.0
//     } else if x > 1.0 {
//         1.0
//     } else {
//         x
//     }
// }

pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn smoothstep01(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Smooth crossfade between two matrix presets.
#[derive(Debug, Clone, Copy)]
pub struct PresetTransition {
    pub from: MatrixPreset,
    pub to: MatrixPreset,
    started_at: Instant,
    duration: Duration,
}

impl PresetTransition {
    pub fn new(from: MatrixPreset, to: MatrixPreset, duration: Duration) -> Self {
        Self {
            from,
            to,
            started_at: Instant::now(),
            duration,
        }
    }

    /// Returns (weights, done)
    pub fn weights(&self) -> ([f32; 4], bool) {
        let elapsed = self.started_at.elapsed().as_secs_f32();
        let dur = self.duration.as_secs_f32().max(0.0001);
        let t = smoothstep01(elapsed / dur);

        let a = self.from.params().weights;
        let b = self.to.params().weights;

        let w = [
            lerp(a[0], b[0], t),
            lerp(a[1], b[1], t),
            lerp(a[2], b[2], t),
            lerp(a[3], b[3], t),
        ];

        (w, t >= 1.0)
    }
}
