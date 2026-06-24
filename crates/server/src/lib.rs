//! JSON-lines control socket and shared command dispatcher.

pub mod analysis;
pub mod app;
pub mod control;
pub mod logging;
mod profile;
pub mod protocol;
mod sampler;
// Consumed by Task 7 (server commands); allow until then.
#[allow(dead_code)]
mod section_click;
pub mod socket;
pub mod stems;
pub mod tuner;
