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
    let rms =
        (buf.data.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / buf.data.len() as f64).sqrt();
    assert!((0.30..=0.40).contains(&rms), "rms = {rms}");
    let _ = SAMPLE_RATE; // canonical-rate contract referenced above
}

#[test]
fn decodes_native_48k_stereo_without_resampling() {
    // 48 kHz stereo exercises the no-resample fast path (to_stereo_interleaved):
    // left ramps up, right is constant, so channels must stay distinct.
    let dir = std::env::temp_dir().join("earworm-decode-48k");
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("stereo48.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&wav, spec).unwrap();
    for i in 0..1000 {
        w.write_sample(((i as f32 / 1000.0) * i16::MAX as f32) as i16)
            .unwrap(); // L
        w.write_sample(i16::MAX / 2).unwrap(); // R constant
    }
    w.finalize().unwrap();

    let buf = decode_file(&wav).unwrap();
    assert_eq!(buf.frames(), 1000, "no resampling at native 48k");
    // right channel is constant ~0.5, left is a rising ramp distinct from right
    assert!((buf.data[500 * 2 + 1] - 0.5).abs() < 0.01);
    assert!(buf.data[900 * 2] > buf.data[100 * 2], "left ramps up");
}

#[test]
fn missing_file_is_an_error() {
    assert!(decode_file(std::path::Path::new("/nope/missing.flac")).is_err());
}

#[test]
fn decode_to_wav_writes_a_canonical_48k_stereo_file() {
    // A 44.1k mono source must come out as a readable 48k stereo WAV — the
    // canonical PCM external tools (analyze, demucs) consume.
    let dir = std::env::temp_dir().join("earworm-decode-to-wav");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src.wav");
    write_test_wav(&src);
    let dst = dir.join("canonical.wav");

    engine::decode::decode_to_wav(&src, &dst).unwrap();

    assert_eq!(engine::capture::wav_header_rate(&dst).unwrap(), SAMPLE_RATE);
    let reader = hound::WavReader::open(&dst).unwrap();
    assert_eq!(reader.spec().channels as usize, CHANNELS);
    assert!(reader.len() > 0, "wrote no samples");
}
