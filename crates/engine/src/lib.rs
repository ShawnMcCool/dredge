//! Audio engine: decode, stretch/pitch, sample-accurate looping (stub).

pub mod buffer;
pub mod capture;
#[cfg(not(target_os = "linux"))]
#[path = "capture_cpal.rs"]
mod capture_cpal;
pub mod decode;
pub mod device;
pub mod encode;
pub mod engine;
pub mod error;
pub mod export;
pub mod ffi;
pub mod filter;
pub mod looper;
#[cfg(target_os = "linux")]
pub mod output;
#[cfg(not(target_os = "linux"))]
#[path = "output_cpal.rs"]
pub mod output;
pub mod peaks;
pub mod pipeline;
pub mod pitch;
pub mod render_core;
pub mod ring;
pub mod stretch;

pub use engine::Engine;
