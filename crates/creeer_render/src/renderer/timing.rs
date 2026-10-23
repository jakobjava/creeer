//! GPU timestamps per pass (G1.3d, ADR-0026): each pass writes a timestamp where it begins
//! and where it ends; the frame's stamps are resolved into a buffer and read back a few
//! frames later, so no frame waits for the GPU. Every GPU budget is measured with them.
//!
//! Timestamp queries are an optional feature: on adapters without it the timer is off and
//! reports nothing.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Passes a frame can time.
const MOST_PASSES: u32 = 32;
/// Readback buffers in flight: a frame's stamps are read up to this many frames later.
const IN_FLIGHT: usize = 3;
/// What a readback buffer is doing (in [`Readback::state`]).
const FREE: u8 = 0;
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;

/// One pass's GPU time in the last frame read back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuTime {
    /// The pass.
    pub name: &'static str,
    /// Its GPU time (ms).
    pub ms: f32,
}

struct Readback {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    /// The passes it holds, in order.
    names: Vec<&'static str>,
    /// The frame they were recorded in.
    frame: u64,
}

/// The frame's query set and the buffers its stamps travel through.
pub(super) struct GpuTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readbacks: Vec<Readback>,
    /// Nanoseconds per timestamp tick.
    period: f32,
    /// The passes timed in the frame being recorded (interior: passes take `&self`).
    names: RefCell<Vec<&'static str>>,
    /// The readback the frame being recorded will be copied into.
    pending: Option<usize>,
    /// Frames recorded, and the frame `latest` comes from.
    frames: u64,
    latest_frame: u64,
    latest: Vec<GpuTime>,
}

impl GpuTimer {
    /// A timer, if the device has timestamp queries.
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let bytes = u64::from(MOST_PASSES) * 2 * wgpu::QUERY_SIZE as u64;
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage,
                mapped_at_creation: false,
            })
        };
        Some(Self {
            set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("gpu timer"),
                ty: wgpu::QueryType::Timestamp,
                count: MOST_PASSES * 2,
            }),
            resolve: buffer(
                "gpu timer resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            readbacks: (0..IN_FLIGHT)
                .map(|_| Readback {
                    buffer: buffer(
                        "gpu timer readback",
                        wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    ),
                    state: Arc::new(AtomicU8::new(FREE)),
                    names: Vec::new(),
                    frame: 0,
                })
                .collect(),
            period: queue.get_timestamp_period(),
            names: RefCell::new(Vec::new()),
            pending: None,
            frames: 0,
            latest_frame: 0,
            latest: Vec::new(),
        })
    }

    /// Starts recording a frame. Frames recorded while every readback is still in flight are
    /// not timed.
    pub(super) fn begin(&mut self) {
        self.frames += 1;
        self.names.get_mut().clear();
        self.pending = self
            .readbacks
            .iter()
            .position(|r| r.state.load(Ordering::Acquire) == FREE);
    }

    /// The next two stamps, for the pass `name` (none when the frame is not timed).
    fn take(&self, name: &'static str) -> Option<(u32, u32)> {
        self.pending?;
        let mut names = self.names.borrow_mut();
        let index = u32::try_from(names.len()).ok()?;
        if index >= MOST_PASSES {
            return None;
        }
        names.push(name);
        Some((2 * index, 2 * index + 1))
    }

    /// Stamps for a render pass named `name`.
    pub(super) fn render(&self, name: &'static str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.take(name)
            .map(|(begin, end)| wgpu::RenderPassTimestampWrites {
                query_set: &self.set,
                beginning_of_pass_write_index: Some(begin),
                end_of_pass_write_index: Some(end),
            })
    }

    /// Stamps for a compute pass named `name`.
    pub(super) fn compute(
        &self,
        name: &'static str,
    ) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        self.take(name)
            .map(|(begin, end)| wgpu::ComputePassTimestampWrites {
                query_set: &self.set,
                beginning_of_pass_write_index: Some(begin),
                end_of_pass_write_index: Some(end),
            })
    }

    /// Resolves the frame's stamps and copies them to its readback (at the frame's end).
    pub(super) fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(slot) = self.pending else {
            return;
        };
        let names = std::mem::take(self.names.get_mut());
        if names.is_empty() {
            self.pending = None;
            return;
        }
        let count = u32::try_from(names.len()).expect("at most MOST_PASSES") * 2;
        encoder.resolve_query_set(&self.set, 0..count, &self.resolve, 0);
        let readback = &mut self.readbacks[slot];
        encoder.copy_buffer_to_buffer(
            &self.resolve,
            0,
            &readback.buffer,
            0,
            u64::from(count) * wgpu::QUERY_SIZE as u64,
        );
        readback.names = names;
        readback.frame = self.frames;
    }

    /// After the frame's submission: asks for its readback to be mapped.
    pub(super) fn submitted(&mut self) {
        let Some(slot) = self.pending.take() else {
            return;
        };
        let readback = &self.readbacks[slot];
        readback.state.store(MAPPING, Ordering::Release);
        let state = Arc::clone(&readback.state);
        readback
            .buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                // A failed map (device lost) frees the slot; the frame is simply not timed.
                state.store(
                    if result.is_ok() { MAPPED } else { FREE },
                    Ordering::Release,
                );
            });
    }

    /// Takes in every readback the GPU has finished (the device must have been polled).
    pub(super) fn collect(&mut self) {
        for readback in &mut self.readbacks {
            if readback.state.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            if readback.frame > self.latest_frame {
                self.latest_frame = readback.frame;
                let bytes = readback
                    .buffer
                    .get_mapped_range(..)
                    .expect("mapped readback");
                let stamps: &[u64] = bytemuck::cast_slice(&bytes);
                self.latest.clear();
                for (i, &name) in readback.names.iter().enumerate() {
                    let (begin, end) = (stamps[2 * i], stamps[2 * i + 1]);
                    // Stamps can wrap; a pass is never negative.
                    #[allow(clippy::cast_precision_loss)] // ticks of a frame: far below 2^24 ms
                    let ms = end.saturating_sub(begin) as f32 * self.period * 1e-6;
                    self.latest.push(GpuTime { name, ms });
                }
            }
            readback.buffer.unmap();
            readback.state.store(FREE, Ordering::Release);
        }
    }

    /// The passes' times in the last frame read back.
    pub(super) fn latest(&self) -> &[GpuTime] {
        &self.latest
    }
}
