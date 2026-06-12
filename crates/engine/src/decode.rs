use crate::buffer::{SongBuffer, CHANNELS, SAMPLE_RATE};
use crate::error::{Error, Result};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const RESAMPLE_CHUNK: usize = 1024;

/// Decode any supported file to the canonical in-memory format:
/// interleaved stereo f32 at 48 kHz.
pub fn decode_file(path: &Path) -> Result<SongBuffer> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| Error::Decode("no default audio track".into()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| Error::Decode(e.to_string()))?;

    let mut src_rate: Option<u32> = track.codec_params.sample_rate;
    let mut src_channels: Option<usize> = track.codec_params.channels.map(|c| c.count());
    let mut interleaved: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // Skip over recoverable per-packet decode errors.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        let spec = *decoded.spec();
        src_rate.get_or_insert(spec.rate);
        src_channels.get_or_insert(spec.channels.count());
        let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sb.copy_interleaved_ref(decoded);
        interleaved.extend_from_slice(sb.samples());
    }

    let ch = src_channels.ok_or_else(|| Error::Decode("unknown channel count".into()))?;
    let rate = src_rate.ok_or_else(|| Error::Decode("unknown sample rate".into()))?;
    if ch == 0 {
        return Err(Error::Unsupported("zero channels".into()));
    }

    let (left, right) = to_stereo_planar(&interleaved, ch);
    let (left, right) = if rate == SAMPLE_RATE {
        (left, right)
    } else {
        resample_stereo(&left, &right, rate)?
    };

    let mut data = Vec::with_capacity(left.len() * CHANNELS);
    for (l, r) in left.iter().zip(right.iter()) {
        data.push(*l);
        data.push(*r);
    }
    Ok(SongBuffer { data })
}

/// Mono → duplicate; stereo → split; >2 ch → L = mean(even chans), R = mean(odd chans).
fn to_stereo_planar(interleaved: &[f32], ch: usize) -> (Vec<f32>, Vec<f32>) {
    let frames = interleaved.len() / ch;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    match ch {
        1 => {
            left.extend_from_slice(interleaved);
            right.extend_from_slice(interleaved);
        }
        2 => {
            for fr in interleaved.chunks_exact(2) {
                left.push(fr[0]);
                right.push(fr[1]);
            }
        }
        n => {
            let evens = n.div_ceil(2) as f32;
            let odds = (n / 2) as f32;
            for fr in interleaved.chunks_exact(n) {
                let l: f32 = fr.iter().step_by(2).sum();
                let r: f32 = fr.iter().skip(1).step_by(2).sum();
                left.push(l / evens);
                right.push(r / odds.max(1.0));
            }
        }
    }
    (left, right)
}

fn resample_stereo(left: &[f32], right: &[f32], src_rate: u32) -> Result<(Vec<f32>, Vec<f32>)> {
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let ratio = SAMPLE_RATE as f64 / src_rate as f64;
    let mut resampler = SincFixedIn::<f64>::new(ratio, 2.0, params, RESAMPLE_CHUNK, 2)
        .map_err(|e| Error::Decode(format!("resampler init: {e}")))?;

    let l64: Vec<f64> = left.iter().map(|s| *s as f64).collect();
    let r64: Vec<f64> = right.iter().map(|s| *s as f64).collect();
    let mut out_l: Vec<f32> = Vec::with_capacity((left.len() as f64 * ratio) as usize + 1024);
    let mut out_r: Vec<f32> = Vec::with_capacity(out_l.capacity());

    let mut pos = 0;
    while pos + RESAMPLE_CHUNK <= l64.len() {
        let chunk = [
            &l64[pos..pos + RESAMPLE_CHUNK],
            &r64[pos..pos + RESAMPLE_CHUNK],
        ];
        let out = resampler
            .process(&chunk, None)
            .map_err(|e| Error::Decode(format!("resample: {e}")))?;
        out_l.extend(out[0].iter().map(|s| *s as f32));
        out_r.extend(out[1].iter().map(|s| *s as f32));
        pos += RESAMPLE_CHUNK;
    }
    if pos < l64.len() {
        let chunk = [&l64[pos..], &r64[pos..]];
        let out = resampler
            .process_partial(Some(&chunk), None)
            .map_err(|e| Error::Decode(format!("resample tail: {e}")))?;
        out_l.extend(out[0].iter().map(|s| *s as f32));
        out_r.extend(out[1].iter().map(|s| *s as f32));
    }
    // Drain the resampler's internal delay line.
    let out = resampler
        .process_partial::<&[f64]>(None, None)
        .map_err(|e| Error::Decode(format!("resample flush: {e}")))?;
    out_l.extend(out[0].iter().map(|s| *s as f32));
    out_r.extend(out[1].iter().map(|s| *s as f32));

    // Trim the leading delay and the zero-padded tail so the output length
    // matches the source duration.
    let delay = resampler.output_delay();
    let expected = (left.len() as f64 * ratio).round() as usize;
    let end = (delay + expected).min(out_l.len());
    out_l.drain(..delay.min(out_l.len()));
    out_l.truncate(end - delay.min(end));
    out_r.drain(..delay.min(out_r.len()));
    out_r.truncate(end - delay.min(end));

    Ok((out_l, out_r))
}

/// blake3 hash of the file contents (streaming, 1 MiB chunks), hex string.
pub fn file_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
