//! Timing harness for `song.open` (plan 13). Ignored by default — run with:
//!
//! ```sh
//! cargo test -p server --test open_timing -- --ignored --nocapture
//! ```
//!
//! Seeds deterministic data (4-minute 44.1 kHz WAV, 4 scaled 44.1 kHz stem
//! copies — the pre-plan-13 cache format) and times `song.open` over the
//! control socket: no stems, first open with stems, second open with stems.

use practice::store::Store;
use serde_json::{json, Value};
use server::app::App;
use server::control::MockEngine;
use server::socket::serve;
use server::stems::{FakeSeparator, STEM_NAMES};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SONG_SECS: u32 = 240;
const SEED_RATE: u32 = 44_100;
const STEM_SCALES: [f32; 4] = [0.4, 0.3, 0.2, 0.1];

/// Deterministic 4-minute 44.1 kHz stereo sine, scaled so distinct songs
/// (and stems) get distinct content hashes.
fn write_44k_wav(path: &std::path::Path, scale: f32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SEED_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(SEED_RATE * SONG_SECS) {
        let t = i as f32 / SEED_RATE as f32;
        let v = (t * 220.0 * std::f32::consts::TAU).sin() * 0.5 * scale;
        let s = (v * i16::MAX as f32) as i16;
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

fn roundtrip(stream: &mut UnixStream, reader: &mut BufReader<UnixStream>, req: &Value) -> Value {
    stream.write_all(req.to_string().as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(resp["ok"], true, "request failed: {line}");
    resp["data"].clone()
}

fn timed_open(
    stream: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    song_id: i64,
) -> Duration {
    let start = Instant::now();
    roundtrip(
        stream,
        reader,
        &json!({"id": 1, "cmd": "song.open", "params": {"song_id": song_id}}),
    );
    start.elapsed()
}

#[test]
#[ignore = "timing harness, run explicitly with --ignored --nocapture"]
fn time_song_open() {
    let dir = tempfile::tempdir().unwrap();
    // keep the peaks cache (~/.cache/dredge/peaks) inside the tempdir
    std::env::set_var("XDG_CACHE_HOME", dir.path().join("cache"));

    let plain = dir.path().join("plain.wav");
    let stemmed = dir.path().join("stemmed.wav");
    write_44k_wav(&plain, 1.0);
    write_44k_wav(&stemmed, 0.9);

    let mut app = App::new(
        Store::open(&dir.path().join("dredge.db")).unwrap(),
        Box::new(MockEngine::default()),
        Arc::new(FakeSeparator),
    );
    let stems_dir = dir.path().join("stems");
    app.set_stems_dir(stems_dir.clone());

    let socket = dir.path().join("dredge.sock");
    let _handle = serve(Arc::new(Mutex::new(app)), &socket, |_| {}).unwrap();
    let mut stream = UnixStream::connect(&socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    let import = |stream: &mut UnixStream, reader: &mut BufReader<UnixStream>, path: &str| {
        roundtrip(
            stream,
            reader,
            &json!({"id": 1, "cmd": "song.import", "params": {"path": path}}),
        )
    };
    let plain_song = import(&mut stream, &mut reader, plain.to_str().unwrap());
    let stemmed_song = import(&mut stream, &mut reader, stemmed.to_str().unwrap());
    let plain_id = plain_song["id"].as_i64().unwrap();
    let stemmed_id = stemmed_song["id"].as_i64().unwrap();

    // seed a pre-plan-13 stems cache: 44.1 kHz WAVs straight into the dir
    let cache = stems_dir.join(stemmed_song["file_hash"].as_str().unwrap());
    std::fs::create_dir_all(&cache).unwrap();
    for (name, scale) in STEM_NAMES.iter().zip(STEM_SCALES) {
        write_44k_wav(&cache.join(format!("{name}.wav")), scale);
    }

    let no_stems_1 = timed_open(&mut stream, &mut reader, plain_id);
    let no_stems_2 = timed_open(&mut stream, &mut reader, plain_id);
    let stems_1 = timed_open(&mut stream, &mut reader, stemmed_id);
    let stems_2 = timed_open(&mut stream, &mut reader, stemmed_id);
    let stems_3 = timed_open(&mut stream, &mut reader, stemmed_id);

    println!("song.open timings ({SONG_SECS} s file seeded at {SEED_RATE} Hz):");
    println!("  no stems, open #1 (peaks computed): {no_stems_1:>8.3?}");
    println!("  no stems, open #2 (peaks cached):   {no_stems_2:>8.3?}");
    println!("  4 stems,  open #1 (peaks computed): {stems_1:>8.3?}");
    println!("  4 stems,  open #2 (peaks cached):   {stems_2:>8.3?}");
    println!("  4 stems,  open #3 (steady state):   {stems_3:>8.3?}");
}
