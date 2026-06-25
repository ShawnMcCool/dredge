//! Graph-clock ↔ sample-frame mapping for sample-accurate overdub anchoring.
//!
//! A `ClockSnapshot` is one reading of a PipeWire stream's `pw_time`: at graph
//! time `now_ns`, the stream was at sample `ticks`, advancing at `rate_hz`
//! samples/sec. From a pair of snapshots (one for the capture stream, one for
//! playback) taken against the same graph clock, we can map a song-playback
//! frame to the capture ring frame that was being acquired at the same instant.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwapOption;

/// One reading of a stream's position against the shared graph clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockSnapshot {
    /// Graph-clock timestamp of this reading (nanoseconds, monotonic).
    pub now_ns: i64,
    /// Stream sample position at `now_ns`.
    pub ticks: i64,
    /// Sample rate (frames per second), e.g. 48_000.
    pub rate_hz: i64,
}

impl ClockSnapshot {
    /// The stream's sample frame at graph time `t_ns` (linear interpolation
    /// from this snapshot).
    pub fn frame_at_ns(&self, t_ns: i64) -> i64 {
        self.ticks + (t_ns - self.now_ns) * self.rate_hz / 1_000_000_000
    }

    /// The graph time (ns) at which the stream reaches sample `frame`.
    pub fn ns_at_frame(&self, frame: i64) -> i64 {
        self.now_ns + (frame - self.ticks) * 1_000_000_000 / self.rate_hz
    }
}

/// Given the capture stream's clock snapshot AND the capture ring's
/// `total_frames_written` recorded at the same `now_ns` as that snapshot,
/// return the ring's absolute frame number being acquired at graph time `t_ns`.
///
/// `cap` is the capture stream snapshot. `ring_total_at_snapshot` is
/// `RollingRing::total_frames_written()` sampled at `cap.now_ns`. Because the
/// ring and the stream both advance at the device rate, the ring frame at
/// `t_ns` is the ring total shifted by however many stream frames elapsed.
pub fn ring_frame_at_ns(cap: &ClockSnapshot, ring_total_at_snapshot: i64, t_ns: i64) -> i64 {
    let stream_frame = cap.frame_at_ns(t_ns);
    ring_total_at_snapshot + (stream_frame - cap.ticks)
}

/// Publishes the latest capture timing to the control thread. Writes are gated
/// by `armed` so the steady audio path does no allocation; the control thread
/// arms briefly around a recording.
#[derive(Default)]
pub struct StreamClock {
    armed: AtomicBool,
    snapshot: ArcSwapOption<ClockSnapshot>,
    ring_total_at_snapshot: AtomicI64,
    delay_frames: AtomicI64,
}

impl StreamClock {
    /// Arm publishing: clear any stale snapshot, then allow `store` to publish.
    pub fn arm(&self) {
        self.snapshot.store(None);
        self.armed.store(true, Ordering::Release);
    }

    /// Stop publishing; subsequent `store` calls are no-ops.
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
    }

    pub fn is_armed(&self) -> bool {
        self.armed.load(Ordering::Acquire)
    }

    /// Called from the RT callback. No-op (and no allocation) unless armed.
    pub fn store(&self, snap: ClockSnapshot, ring_total: i64, delay_frames: i64) {
        if self.armed.load(Ordering::Acquire) {
            self.ring_total_at_snapshot.store(ring_total, Ordering::Release);
            self.delay_frames.store(delay_frames, Ordering::Release);
            self.snapshot.store(Some(Arc::new(snap))); // publish last
        }
    }

    pub fn load(&self) -> Option<ClockSnapshot> {
        self.snapshot.load_full().map(|a| *a)
    }

    pub fn ring_total_at_snapshot(&self) -> i64 {
        self.ring_total_at_snapshot.load(Ordering::Acquire)
    }

    pub fn delay_frames(&self) -> i64 {
        self.delay_frames.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_graph_time_to_stream_frame() {
        let s = ClockSnapshot { now_ns: 1_000_000_000, ticks: 48_000, rate_hz: 48_000 };
        assert_eq!(s.frame_at_ns(1_000_000_000), 48_000); // at the snapshot instant
        assert_eq!(s.frame_at_ns(1_500_000_000), 48_000 + 24_000); // +0.5s
        assert_eq!(s.frame_at_ns(500_000_000), 48_000 - 24_000); // 0.5s before
    }

    #[test]
    fn maps_stream_frame_to_graph_time() {
        let s = ClockSnapshot { now_ns: 1_000_000_000, ticks: 48_000, rate_hz: 48_000 };
        assert_eq!(s.ns_at_frame(48_000), 1_000_000_000);
        assert_eq!(s.ns_at_frame(72_000), 1_500_000_000);
    }

    #[test]
    fn frame_ns_round_trip() {
        let s = ClockSnapshot { now_ns: 5_000_000_000, ticks: 123_456, rate_hz: 48_000 };
        // round-trip on multiples of the rate to avoid integer-division remainder
        for f in [123_456_i64, 123_456 + 48_000, 123_456 + 96_000] {
            assert_eq!(s.frame_at_ns(s.ns_at_frame(f)), f);
        }
    }

    #[test]
    fn ring_frame_maps_through_graph_time() {
        // capture stream: at now=0 it's at tick 1000; ring had written 1000 frames then.
        let cap = ClockSnapshot { now_ns: 0, ticks: 1000, rate_hz: 48_000 };
        let ring_total_at_snapshot = 1000;
        // playback song clock: song frame `start` was output at graph time T.
        let song = ClockSnapshot { now_ns: 0, ticks: 0, rate_hz: 48_000 };
        let start_song_frame = 48_000; // 1.0s into the song
        let t = song.ns_at_frame(start_song_frame);
        // the ring frame acquired at that same instant:
        let ring_frame = ring_frame_at_ns(&cap, ring_total_at_snapshot, t);
        // 1.0s after the snapshot, ring advanced 48000 frames from 1000
        assert_eq!(ring_frame, 1000 + 48_000);
    }

    #[test]
    fn ring_frame_with_offset_clocks() {
        // capture and song clocks taken at different `now_ns` but same graph clock
        let cap = ClockSnapshot { now_ns: 2_000_000_000, ticks: 500_000, rate_hz: 48_000 };
        let ring_total_at_snapshot = 500_000;
        let song = ClockSnapshot { now_ns: 1_900_000_000, ticks: 90_000, rate_hz: 48_000 };
        let start_song_frame = 96_000; // 2.0s in song
        let t = song.ns_at_frame(start_song_frame); // graph time the song reaches frame 96000
        let ring_frame = ring_frame_at_ns(&cap, ring_total_at_snapshot, t);
        // sanity: ring_frame is finite and equals ring_total + (cap.frame_at_ns(t) - cap.ticks)
        let expect = ring_total_at_snapshot + (cap.frame_at_ns(t) - cap.ticks);
        assert_eq!(ring_frame, expect);
    }

    fn sample_snapshot() -> ClockSnapshot {
        ClockSnapshot { now_ns: 7_000_000_000, ticks: 333_000, rate_hz: 48_000 }
    }

    #[test]
    fn clock_load_is_none_before_arming() {
        let clock = StreamClock::default();
        assert!(!clock.is_armed());
        assert_eq!(clock.load(), None);
    }

    #[test]
    fn clock_store_while_disarmed_is_noop() {
        let clock = StreamClock::default();
        clock.store(sample_snapshot(), 1234, 56);
        assert_eq!(clock.load(), None);
    }

    #[test]
    fn clock_publishes_after_arm() {
        let clock = StreamClock::default();
        clock.arm();
        assert!(clock.is_armed());
        let snap = sample_snapshot();
        clock.store(snap, 1234, 56);
        assert_eq!(clock.load(), Some(snap));
        assert_eq!(clock.ring_total_at_snapshot(), 1234);
        assert_eq!(clock.delay_frames(), 56);
    }

    #[test]
    fn clock_disarm_then_store_is_noop() {
        let clock = StreamClock::default();
        clock.arm();
        clock.disarm();
        assert!(!clock.is_armed());
        clock.store(sample_snapshot(), 1234, 56);
        assert_eq!(clock.load(), None);
    }
}
