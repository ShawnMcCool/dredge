pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// Whole song in memory: interleaved stereo f32 at 48 kHz.
#[derive(Debug, Clone, PartialEq)]
pub struct SongBuffer {
    pub data: Vec<f32>,
}

impl SongBuffer {
    pub fn frames(&self) -> usize {
        self.data.len() / CHANNELS
    }
    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / SAMPLE_RATE as f64
    }
}
