//! A GPU buffer that grows to what a frame needs (G1.3e).

/// A buffer that grows to what a frame needs.
pub(super) struct Grown {
    pub(super) buffer: wgpu::Buffer,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl Grown {
    pub(super) fn new(
        device: &wgpu::Device,
        label: &'static str,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: 256,
            usage,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            usage,
            label,
        }
    }

    /// Grows to at least `bytes`; true when it was made anew.
    pub(super) fn reserve(&mut self, device: &wgpu::Device, bytes: u64) -> bool {
        if bytes <= self.buffer.size() {
            return false;
        }
        self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size: bytes.next_power_of_two(),
            usage: self.usage,
            mapped_at_creation: false,
        });
        true
    }

    /// Writes `bytes` from the start, growing first; true when it was made anew.
    pub(super) fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
    ) -> bool {
        let grew = self.reserve(device, bytes.len() as u64);
        queue.write_buffer(&self.buffer, 0, bytes);
        grew
    }
}
