use engine::buffer::{CHANNELS, SAMPLE_RATE};
use engine::decode::decode_file;

/// 1 s of 440 Hz mono sine at 44.1 kHz — exercises resample AND mono→stereo.
fn write_test_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..44_100 {
        let t = i as f32 / 44_100.0;
        let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        w.write_sample((v * i16::MAX as f32) as i16).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn decodes_resamples_and_upmixes() {
    let dir = std::env::temp_dir().join("earworm-decode-test");
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("sine.wav");
    write_test_wav(&wav);

    let buf = decode_file(&wav).unwrap();
    // ~1 s at 48 kHz (resampler may trim edges slightly)
    let frames = buf.frames();
    assert!((47_000..=49_000).contains(&frames), "frames = {frames}");
    assert_eq!(buf.data.len() % CHANNELS, 0);
    // stereo channels identical after mono upmix
    assert_eq!(buf.data[1000 * 2], buf.data[1000 * 2 + 1]);
    // energy preserved: RMS of a 0.5-amplitude sine ≈ 0.35
    let rms = (buf.data.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / buf.data.len() as f64)
        .sqrt();
    assert!((0.30..=0.40).contains(&rms), "rms = {rms}");
    let _ = SAMPLE_RATE; // canonical-rate contract referenced above
}

#[test]
fn missing_file_is_an_error() {
    assert!(decode_file(std::path::Path::new("/nope/missing.flac")).is_err());
}
