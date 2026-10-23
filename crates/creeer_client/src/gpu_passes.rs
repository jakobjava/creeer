//! GPU time per pass over a bench run (G1.3d): the renderer's timestamps gathered frame by
//! frame and reported as medians.

use creeer_render::GpuTime;

/// Each pass's GPU times (ms), in the order the passes first appeared.
#[derive(Debug, Default)]
pub(crate) struct GpuPasses {
    passes: Vec<(&'static str, Vec<f32>)>,
}

impl GpuPasses {
    /// Adds one frame's times.
    pub fn push(&mut self, times: &[GpuTime]) {
        for time in times {
            match self.passes.iter_mut().find(|(name, _)| *name == time.name) {
                Some((_, list)) => list.push(time.ms),
                None => self.passes.push((time.name, vec![time.ms])),
            }
        }
    }

    /// One line per pass with its median, and the frame's total (the sum of the medians).
    pub fn report(&mut self) -> String {
        if self.passes.is_empty() {
            return "  GPU passes: no timestamps on this adapter".to_owned();
        }
        let mut total = 0.0;
        let mut lines = vec!["  GPU passes (p50):".to_owned()];
        for (name, list) in &mut self.passes {
            list.sort_by(f32::total_cmp);
            let median = list[list.len() / 2];
            total += median;
            lines.push(format!("    {name:<12} {median:.3} ms"));
        }
        lines.push(format!("    {:<12} {total:.3} ms", "all"));
        lines.join("\n")
    }
}
