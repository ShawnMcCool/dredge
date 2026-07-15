use crate::analysis::Analyzer;
use crate::control::AudioControl;
use crate::protocol::{Event, Request, Response};
use crate::sampler::{SharedWork, WorkReporter, WorkSample};
use crate::stems::{StemSeparator, STEM_NAMES};
use crate::tuner::{RealTuner, TunerControl, TunerReading};
use engine::metronome::{Cadence, Kit};
use engine::pipeline::{EngineCmd, EngineEvent};
use practice::library::{LoopRename, NewLoop, NewSection};
use practice::model::{
    Analysis, AnalysisSection, Block, CountIn, CountInMode, LoopId, LoopKind, Mix, ProfileRun,
    Recording, RecordingId, Routine, RoutineId, Section, SectionId, Song, SongId,
};
use practice::notes::NotesDoc;
use practice::store::Store;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

/// Map any displayable error onto the protocol's String error channel.
trait ErrStr<T> {
    fn err_str(self) -> Result<T, String>;
}
impl<T, E: std::fmt::Display> ErrStr<T> for Result<T, E> {
    fn err_str(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

fn from_params<T: serde::de::DeserializeOwned>(p: Value) -> Result<T, String> {
    serde_json::from_value(p).map_err(|e| format!("bad params: {e}"))
}

/// Semitones + cents (+ optional octave) → the frequency multiplier the engine
/// stretcher wants. Shared by the live `pitch` command and offline export so
/// both pitch the same way.
fn pitch_scale_factor(semitones: f64, cents: f64, octave_up: bool) -> f64 {
    2f64.powf((semitones + cents / 100.0) / 12.0) * if octave_up { 2.0 } else { 1.0 }
}

/// Count-in 1x beat interval from BPM (seconds per beat).
fn beat_secs_from_bpm(bpm: f64) -> f64 {
    60.0 / bpm
}

/// Current UTC time as an RFC-3339 string for recording timestamps. Mirrors the
/// formatting used by `logging.rs`; an unexpected format error yields "" rather
/// than failing the command.
fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

// --- shared dispatch (lock-phased heavy commands) ---------------------------

/// Dispatch that holds the App lock only for state work — known-heavy
/// commands (`song.open`, `song.import`) run their decode/hash/IO phase
/// outside the lock, so the tick pump and other clients never wait behind a
/// multi-second decode. Everything else takes the lock once and delegates to
/// `App::dispatch` (which inlines the same phases).
pub fn dispatch_shared(app: &Arc<Mutex<App>>, req: Request) -> Response {
    let id = req.id;
    let phased = match req.cmd.as_str() {
        "song.open" => open_phased(app, req.params),
        "song.import" => import_phased(app, req.params),
        "song.update" => update_phased(app, req.params),
        "recording.calibrate" => calibrate_phased(app, req.params),
        _ => return app.lock().unwrap().dispatch(req),
    };
    match phased {
        Ok(data) => Response::ok(id, data),
        Err(e) => Response::err(id, e),
    }
}

fn open_phased(app: &Arc<Mutex<App>>, p: Value) -> Result<Value, String> {
    let p: OpenParams = from_params(p)?;
    let (song, stems_cache) = app.lock().unwrap().open_lookup(p.song_id)?;
    let decoded = open_decode(&song, &stems_cache)?;
    app.lock().unwrap().finish_open(song, decoded)
}

fn update_phased(app: &Arc<Mutex<App>>, p: Value) -> Result<Value, String> {
    let (song, reopen) = app.lock().unwrap().update_apply(p)?;
    if let Some(song_id) = reopen {
        // Reopen the renamed song with the heavy decode off-lock, exactly like
        // `open_phased`, so the pump never waits behind it.
        let (s, stems_cache) = app.lock().unwrap().open_lookup(song_id)?;
        let decoded = open_decode(&s, &stems_cache)?;
        app.lock().unwrap().finish_open(s, decoded)?;
    }
    serde_json::to_value(song).err_str()
}

fn import_phased(app: &Arc<Mutex<App>>, p: Value) -> Result<Value, String> {
    let p: ImportParams = from_params(p)?;
    let hash = engine::decode::file_hash(Path::new(&p.path)).err_str()?;
    if let Some(existing) = app.lock().unwrap().import_lookup(&hash)? {
        return serde_json::to_value(existing).err_str();
    }
    let prep = import_decode(p.path, p.title, hash)?;
    app.lock().unwrap().import_prepared(prep)
}

/// Loopback round-trip-latency (RTL) calibration, off the App lock. With a
/// physical loopback cable (an output patched to an input), this emits a short
/// impulse out the output and measures the sample delay until it returns on the
/// capture ring; that delay IS the RTL.
///
/// Flow (reuses the verified overdub-sync clock machinery): start + arm a
/// capture on the input, emit the impulse (the output RT callback stamps its
/// graph-clock emit time), wait ~1s for the cable round-trip, then map `emit_ns`
/// to the capture ring frame `f_emit` via `ring_frame_at_ns` and read ~1s from
/// there. `detect_click_onset` on that window — which starts exactly at the emit
/// instant — yields the RTL in frames. The recorder lives behind its own mutex
/// so the ~1s wait runs with neither the App lock nor the recorder lock held in
/// a way that stalls the tick pump; we re-lock only to emit, read back, and
/// persist.
fn calibrate_phased(app: &Arc<Mutex<App>>, p: Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct P {
        device_id: String,
    }
    let p: P = from_params(p)?;
    let recorder = app.lock().unwrap().recorder.clone();

    // Start + arm the capture on the loopback input before emitting, so the ring
    // is recording when the impulse returns.
    recorder.lock().unwrap().calibrate_session(&p.device_id)?;
    // Emit the impulse (just sets an atomic; the RT callback does the rest).
    app.lock().unwrap().audio.emit_impulse();
    // Wait for the impulse to traverse the cable and be captured.
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let emit_ns = app.lock().unwrap().audio.impulse_emit_ns();
    if emit_ns == 0 {
        let _ = recorder.lock().unwrap().stop();
        return Err("calibration: impulse never emitted (no output stream?)".into());
    }

    // Map the emit instant to the capture ring frame, then read from there up to
    // what's actually been captured (the onset is only tens of ms out, so a
    // full second is unnecessary — and reading past the captured end was the
    // "window unavailable" failure).
    let rate = engine::buffer::SAMPLE_RATE as i64;
    let debug = std::env::var("DREDGE_DEBUG").is_ok();
    let measured = {
        let rec = recorder.lock().unwrap();
        match rec.capture_snapshot() {
            None => {
                if debug {
                    eprintln!("dredge calibrate[loopback]: no capture snapshot (clock not armed?)");
                }
                None
            }
            Some((cap_snap, ring_total)) => {
                let f_emit =
                    engine::stream_clock::ring_frame_at_ns(&cap_snap, ring_total, emit_ns).max(0);
                // window = up to 1s, clamped to frames captured since f_emit.
                let win = (ring_total - f_emit).clamp(0, rate);
                if debug {
                    eprintln!(
                        "dredge calibrate[loopback]: emit_ns={emit_ns} ring_total={ring_total} \
                         f_emit={f_emit} win={win}"
                    );
                }
                rec.extract_range(f_emit, win).map(|slice| (f_emit, slice))
            }
        }
    };
    let _ = recorder.lock().unwrap().stop(); // tear the capture session down

    let (f_emit, slice) = measured.ok_or("calibration: capture timing/window unavailable")?;
    let rtl = crate::recording::detect_click_onset(&slice, CALIBRATION_CLICK_THRESHOLD)
        .ok_or("calibration: no click detected in loopback")? as i64;
    // Sanity-clamp: reject anything outside 0..=2s as not a real cable round-trip.
    if !(0..=2 * rate).contains(&rtl) {
        return Err(format!(
            "calibration: implausible round-trip latency ({rtl} frames)"
        ));
    }
    if std::env::var("DREDGE_DEBUG").is_ok() {
        eprintln!(
            "dredge calibrate[loopback]: emit_ns={emit_ns} f_emit={f_emit} rtl={rtl} ({:.1} ms)",
            rtl as f64 / rate as f64 * 1000.0
        );
    }

    // Envelope for the UI: a peak-amplitude trace of the ~150 ms window starting
    // at the emit instant, so the UI can draw the click's return and mark the
    // emit (left edge, index 0) and onset positions, labeling the gap as the
    // latency. The window starts exactly at `f_emit`, so emit_index is 0.
    let frames_per_bucket = CALIBRATION_ENVELOPE_WINDOW_FRAMES / CALIBRATION_ENVELOPE_POINTS as i64;
    let envelope = crate::recording::peak_envelope(
        &slice,
        CALIBRATION_ENVELOPE_WINDOW_FRAMES as usize,
        CALIBRATION_ENVELOPE_POINTS,
    );
    let onset_index = rtl / frames_per_bucket;
    let window_ms = CALIBRATION_ENVELOPE_WINDOW_FRAMES as f64 / rate as f64 * 1000.0;

    let mut guard = app.lock().unwrap();
    guard
        .store
        .set_setting(INPUT_LATENCY_LOOPBACK_KEY, &json!(rtl))
        .err_str()?;
    guard
        .store
        .set_setting(LATENCY_SOURCE_KEY, &json!("loopback"))
        .err_str()?;
    guard.refresh_layers();
    Ok(json!({
        "latency_frames": rtl,
        "latency_ms": rtl as f64 / rate as f64 * 1000.0,
        "source": "loopback",
        "envelope": envelope,
        "emit_index": 0,
        "onset_index": onset_index,
        "window_ms": window_ms,
    }))
}

#[derive(Deserialize)]
struct OpenParams {
    song_id: SongId,
}

#[derive(Deserialize)]
struct ImportParams {
    path: String,
    title: Option<String>,
}

/// Lock-free product of `song.open`'s slow phase: engine-ready audio,
/// whether it's a 4-stem set, and the waveform peaks.
struct OpenDecoded {
    set: engine::buffer::StemSet,
    stems: bool,
    peaks: engine::peaks::Peaks,
}

/// Slow phase of `song.open` (pure, no lock): decode the mix, load/compute
/// peaks, and decode all cached stem WAVs when present. With stems the five
/// decodes run concurrently (`std::thread::scope`): one thread per stem WAV
/// while the calling thread decodes the original and computes peaks.
fn open_decode(song: &Song, stems_cache: &Path) -> Result<OpenDecoded, String> {
    // No cached stems: the plain mix alone.
    if !App::stems_cached(stems_cache) {
        let buf = engine::decode::decode_file(Path::new(&song.path)).err_str()?;
        let peaks = engine::peaks::load_or_compute(&buf, &song.file_hash).err_str()?;
        return Ok(OpenDecoded {
            set: engine::buffer::StemSet::single(buf),
            stems: false,
            peaks,
        });
    }
    std::thread::scope(|scope| {
        let stem_threads: Vec<_> = STEM_NAMES
            .iter()
            .map(|name| {
                let path = stems_cache.join(format!("{name}.wav"));
                scope.spawn(move || open_stem(&path))
            })
            .collect();
        let buf = engine::decode::decode_file(Path::new(&song.path)).err_str()?;
        let peaks = engine::peaks::load_or_compute(&buf, &song.file_hash).err_str()?;
        let mut bufs = Vec::with_capacity(STEM_NAMES.len());
        for t in stem_threads {
            bufs.push(t.join().map_err(|_| "stem decode thread panicked")??);
        }
        Ok(OpenDecoded {
            set: engine::buffer::StemSet::new(bufs),
            stems: true,
            peaks,
        })
    })
}

/// Decode one cached stem WAV for the open path. Legacy caches (written at
/// 44.1 kHz before stems were normalized at separation time) are rewritten
/// at 48 kHz from the buffer just decoded, so the *next* open of this song
/// skips the sinc resample entirely. The rewrite is best-effort — this open
/// already has its audio either way.
fn open_stem(path: &Path) -> Result<engine::buffer::SongBuffer, String> {
    let header_rate = engine::capture::wav_header_rate(path).err_str()?;
    let buf = engine::decode::decode_file(path).err_str()?;
    if header_rate != engine::buffer::SAMPLE_RATE {
        if let Err(e) = crate::stems::rewrite_wav_48k(path, &buf.data) {
            eprintln!(
                "dredge: stem cache upgrade failed for {}: {e}",
                path.display()
            );
        }
    }
    Ok(buf)
}

/// Decode the song to an engine `StemSet` for export — the 4-stem set when a
/// stem cache exists, else the plain mix. Skips peak computation (export
/// doesn't need a waveform). No lock held; runs on the export thread.
fn export_decode(song: &Song, stems_cache: &Path) -> Result<engine::buffer::StemSet, String> {
    if !App::stems_cached(stems_cache) {
        let buf = engine::decode::decode_file(Path::new(&song.path)).err_str()?;
        return Ok(engine::buffer::StemSet::single(buf));
    }
    let mut bufs = Vec::with_capacity(STEM_NAMES.len());
    for name in STEM_NAMES {
        bufs.push(open_stem(&stems_cache.join(format!("{name}.wav")))?);
    }
    Ok(engine::buffer::StemSet::new(bufs))
}

/// Resolve a user-typed export folder into an absolute path: expand a leading
/// `~`/`~/` to the home directory, then require the result be absolute. A typed
/// relative path must never resolve against the daemon's working directory —
/// that's how typing `~/downloads/` once spawned a literal `~` dir in the repo.
fn resolve_export_dir(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("choose an export folder".into());
    }
    let path = if trimmed == "~" {
        dirs::home_dir().ok_or("can't find your home directory")?
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or("can't find your home directory")?
            .join(rest)
    } else {
        PathBuf::from(trimmed)
    };
    if !path.is_absolute() {
        return Err("enter an absolute folder path (e.g. /home/you/Music or ~/Music)".into());
    }
    Ok(path)
}

/// Reject an export whose destination isn't usable *before* any work starts:
/// the folder must already exist (the picker only returns real dirs; a typed
/// path might not), and the file name must be a plain name — non-empty and
/// free of path separators so it can't escape the chosen folder.
fn validate_export_target(dir: &Path, filename: &str) -> Result<(), String> {
    let name = filename.trim();
    if name.is_empty() {
        return Err("enter a file name".into());
    }
    if name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err("file name can't contain slashes or path parts".into());
    }
    if !dir.is_dir() {
        return Err("export folder doesn't exist".into());
    }
    Ok(())
}

/// `dir/stem.ext`, or `dir/stem (n).ext` if that exists — never silently
/// clobbers a previous export.
fn unique_export_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let base = dir.join(format!("{stem}.{ext}"));
    if !base.exists() {
        return base;
    }
    for n in 1..10_000 {
        let cand = dir.join(format!("{stem} ({n}).{ext}"));
        if !cand.exists() {
            return cand;
        }
    }
    base
}

/// Lock-free product of `song.import`'s slow phase.
struct ImportPrepared {
    path: String,
    title: String,
    hash: String,
    duration_secs: f64,
}

/// Slow phase of `song.import` (pure, no lock): decode for the duration. The
/// hash is computed by the caller — it gates the dedupe lookup that decides
/// whether this phase runs at all.
fn import_decode(
    path: String,
    title: Option<String>,
    hash: String,
) -> Result<ImportPrepared, String> {
    let p = Path::new(&path);
    let buf = engine::decode::decode_file(p).err_str()?;
    // explicit title wins over the file stem
    let title = title.unwrap_or_else(|| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_owned()
    });
    Ok(ImportPrepared {
        duration_secs: buf.duration_secs(),
        title,
        hash,
        path,
    })
}

/// Last broadcast playhead snapshot: secs, rate, playing, count-in `(beat, of)`.
/// Compared field-wise to throttle no-op position broadcasts.
type PositionSnapshot = (f64, f64, bool, Option<(u32, u32)>);

pub struct App {
    store: Store,
    library: practice::library::Library,
    audio: Box<dyn AudioControl>,
    separator: Arc<dyn StemSeparator>,
    open_song: Option<OpenSong>,
    /// Canonical live mix — the resolved isolation state (stem gains +
    /// bass-focus). Every isolation control mutates this; the routine scheduler
    /// applies whole snapshots of it. Reset to `Mix::default()` on song open.
    current_mix: Mix,
    /// The running practice routine, if any. Advances on engine loop-wrap events
    /// (`tick`), driving the loop region / mix / rate / count-in per block.
    active_routine: Option<crate::routine::RoutineRunner>,
    last_position: Option<PositionSnapshot>,
    /// Background-job events (stem separation); drained by `tick()`.
    job_tx: mpsc::Sender<Event>,
    job_rx: mpsc::Receiver<Event>,
    /// Song ids with a separation thread in flight.
    separating: Arc<Mutex<HashSet<i64>>>,
    /// Set true to ask the in-flight export render to stop. One export at a
    /// time: a new `export.start` replaces (and cancels) any prior one.
    export_cancel: Arc<AtomicBool>,
    analyzer: Arc<dyn Analyzer>,
    /// Finished analyses; drained by `tick()`, which persists them (the
    /// store lives on this thread) and emits `analysis_progress`.
    analysis_tx: mpsc::Sender<(SongId, Result<Analysis, String>)>,
    analysis_rx: mpsc::Receiver<(SongId, Result<Analysis, String>)>,
    /// Song ids with an analysis thread in flight (main thread only).
    analyzing: HashSet<i64>,
    /// Finished profiling runs; drained by `tick()`, persisted, emitted as
    /// `profile_run`.
    profile_tx: mpsc::Sender<ProfileRun>,
    profile_rx: mpsc::Receiver<ProfileRun>,
    /// Shared "what's running now" slot, read by the sampler thread.
    work_state: SharedWork,
    /// Live work samples from the sampler thread; drained by `tick()`.
    work_sample_tx: mpsc::Sender<WorkSample>,
    work_sample_rx: mpsc::Receiver<WorkSample>,
    /// Live pitch readings from the tuner sampler thread; drained by `tick()`.
    tuner: Box<dyn TunerControl>,
    tuner_tx: mpsc::Sender<TunerReading>,
    tuner_rx: mpsc::Receiver<TunerReading>,
    /// Live input level readings for the record-arming meter; drained by `tick()`.
    input_monitor: Box<dyn crate::input_monitor::InputMonitorControl>,
    monitor_tx: mpsc::Sender<crate::input_monitor::InputLevel>,
    monitor_rx: mpsc::Receiver<crate::input_monitor::InputLevel>,
    /// Input capture backend for overdub recording. Behind its own mutex so the
    /// blocking `calibrate_capture` can run via `dispatch_shared` without
    /// holding the App lock (which would stall the tick pump).
    recorder: Arc<Mutex<Box<dyn crate::recording::RecordingControl>>>,
    /// The take being captured between `recording.start` and `recording.stop`.
    pending_recording: Option<PendingRecording>,
    /// Decoded recording audio, keyed by id. A take's WAV is immutable, so a
    /// cached buffer is reused across edits — only a brand-new take decodes.
    layer_cache: std::collections::HashMap<RecordingId, Arc<engine::buffer::SongBuffer>>,
    /// Per-take waveform peaks, keyed by id. Computed once from the decoded
    /// buffer (a recomputable cache, never persisted — like the song's peaks)
    /// and shipped to the frontend so each take draws as a real waveform lane.
    layer_peaks: std::collections::HashMap<RecordingId, engine::peaks::Peaks>,
}

struct OpenSong {
    song: Song,
    /// True when the engine got a 4-stem StemSet for this song.
    stems: bool,
}

/// In-flight recording state: what span we're capturing and for which song.
struct PendingRecording {
    song_id: SongId,
    anchor_frame: i64,
    /// Span length in source frames; the take ends when playback reaches
    /// `(anchor_frame + len_frames) / SAMPLE_RATE`.
    len_frames: i64,
    /// Capture-ring frame mapping to the song's `anchor_frame`, frozen at the
    /// first real-playback tick (count-in done). `None` until pinned. Pinning at
    /// playback start keeps the graph-time extrapolation short and excludes the
    /// count-in cleanly, instead of extrapolating from span-end at finalize.
    ring_start: Option<i64>,
}

/// Auto-detected RTL baseline in frames: output+input PipeWire delays,
/// re-measured on every take (kept current even while loopback is active).
const INPUT_LATENCY_AUTO_KEY: &str = "input_latency_auto";
/// Loopback-calibrated RTL in frames, pinned by AS-7 calibration. Persisted but
/// inactive while `latency_source` is `"auto"`.
const INPUT_LATENCY_LOOPBACK_KEY: &str = "input_latency_loopback";
/// Which measurement is active: `"auto"` (default — auto baseline) or
/// `"loopback"` (the calibrated value). The active value is DERIVED from this by
/// `input_latency_frames`; there is no single stored "current" value.
const LATENCY_SOURCE_KEY: &str = "latency_source";
/// Absolute-sample threshold for the latency-calibration click onset detector.
const CALIBRATION_CLICK_THRESHOLD: f32 = 0.3;
/// Window summarized into the calibration envelope: ~150 ms at 48 kHz, starting
/// at the impulse emit instant — long enough to show the click's return.
const CALIBRATION_ENVELOPE_WINDOW_FRAMES: i64 = 7_200;
/// Envelope resolution (peak-amplitude buckets) the UI draws across that window.
const CALIBRATION_ENVELOPE_POINTS: usize = 240;

impl App {
    pub fn new(
        store: Store,
        audio: Box<dyn AudioControl>,
        separator: Arc<dyn StemSeparator>,
    ) -> Self {
        let (job_tx, job_rx) = mpsc::channel();
        let (analysis_tx, analysis_rx) = mpsc::channel();
        let (profile_tx, profile_rx) = mpsc::channel();
        let (work_sample_tx, work_sample_rx) = mpsc::channel();
        let (tuner_tx, tuner_rx) = mpsc::channel();
        let (monitor_tx, monitor_rx) = mpsc::channel();
        let root = Self::library_root(&store);
        let library = practice::library::Library::load(root.clone()).unwrap_or_else(|e| {
            eprintln!("dredge: library load failed at {}: {e}", root.display());
            practice::library::Library::empty(root)
        });
        // Read the saved output device before moving `audio` and `store` into
        // the struct, so we can apply it right after construction.
        let saved_output_device = store
            .get_setting("output_device")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|s| !s.is_empty());
        let mut app = Self {
            store,
            library,
            audio,
            separator,
            open_song: None,
            current_mix: Mix::default(),
            active_routine: None,
            last_position: None,
            job_tx,
            job_rx,
            separating: Arc::new(Mutex::new(HashSet::new())),
            export_cancel: Arc::new(AtomicBool::new(false)),
            analyzer: Arc::new(crate::analysis::ScriptAnalyzer::default()),
            analysis_tx,
            analysis_rx,
            analyzing: HashSet::new(),
            profile_tx,
            profile_rx,
            work_state: std::sync::Arc::new(std::sync::Mutex::new(None)),
            work_sample_tx,
            work_sample_rx,
            tuner: Box::new(RealTuner::default()),
            tuner_tx,
            tuner_rx,
            input_monitor: Box::new(crate::input_monitor::RealInputMonitor::default()),
            monitor_tx,
            monitor_rx,
            recorder: Arc::new(Mutex::new(Box::new(
                crate::recording::RealRecorder::default(),
            ))),
            pending_recording: None,
            layer_cache: std::collections::HashMap::new(),
            layer_peaks: std::collections::HashMap::new(),
        };
        // Apply the saved output device if one was persisted. The Engine's
        // fallback handles a currently-absent device by reverting to default,
        // so we don't delete the setting on failure.
        if let Some(id) = saved_output_device {
            app.audio.set_output_device(Some(id));
        }
        app
    }

    /// Swap the analyzer (tests use `FakeAnalyzer`).
    pub fn set_analyzer(&mut self, analyzer: Arc<dyn Analyzer>) {
        self.analyzer = analyzer;
    }

    /// Swap the tuner (tests use `MockTuner`).
    pub fn set_tuner(&mut self, tuner: Box<dyn TunerControl>) {
        self.tuner = tuner;
    }

    /// Swap the input monitor (tests use `MockInputMonitor`).
    #[cfg(test)]
    pub fn set_input_monitor(
        &mut self,
        monitor: Box<dyn crate::input_monitor::InputMonitorControl>,
    ) {
        self.input_monitor = monitor;
    }

    /// Swap the recording backend (tests use `FakeRecorder`).
    #[cfg(test)]
    pub fn set_recorder(&mut self, recorder: Box<dyn crate::recording::RecordingControl>) {
        self.recorder = Arc::new(Mutex::new(recorder));
    }

    /// Handles the sampler thread needs (work-state slot + sample sender).
    /// Cloned out once by `serve` before it spawns the sampler.
    pub fn sampler_handles(&self) -> (SharedWork, mpsc::Sender<WorkSample>) {
        (self.work_state.clone(), self.work_sample_tx.clone())
    }

    /// A reporter the heavy workers use to publish their stage.
    fn work_reporter(&self) -> WorkReporter {
        WorkReporter::new(self.work_state.clone())
    }

    /// Library root: the `library_root` setting if set, else the OS default.
    fn library_root(store: &Store) -> PathBuf {
        if let Ok(Some(v)) = store.get_setting("library_root") {
            if let Some(s) = v.as_str() {
                if !s.trim().is_empty() {
                    return PathBuf::from(s);
                }
            }
        }
        practice::bundle::default_library_root().unwrap_or_else(|| PathBuf::from("dredge-library"))
    }

    /// Point the library at `root` (tests use a tempdir; also used if the
    /// library_root setting changes and the app reloads).
    pub fn set_library_root(&mut self, root: std::path::PathBuf) {
        self.library = practice::library::Library::load(root.clone())
            .unwrap_or_else(|_| practice::library::Library::empty(root));
    }

    /// Bundle directory for a song — test/diagnostic helper.
    pub fn song_bundle_dir(&self, song_id: SongId) -> Option<std::path::PathBuf> {
        self.library.bundle_dir(song_id)
    }

    /// Remove any Demucs staging dirs left by a separation that was killed
    /// mid-run (e.g. the app quit). The committed stem cache uses atomic rename,
    /// so only the hidden `.demucs-tmp` staging can survive — sweep it at start.
    pub fn sweep_stem_staging(&self) {
        for bundle in self.library.bundle_dirs() {
            let stems = bundle.join("stems");
            let Ok(entries) = std::fs::read_dir(&stems) else {
                continue;
            };
            for entry in entries.flatten() {
                let tmp = entry.path().join(".demucs-tmp");
                if tmp.is_dir() {
                    let _ = std::fs::remove_dir_all(&tmp);
                }
            }
        }
    }

    pub fn dispatch(&mut self, req: Request) -> Response {
        let id = req.id;
        match self.dispatch_inner(&req.cmd, req.params) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::err(id, e),
        }
    }

    fn dispatch_inner(&mut self, cmd: &str, p: Value) -> Result<Value, String> {
        match cmd {
            "song.import" => self.song_import(p),
            "song.list" => serde_json::to_value(self.library.list_songs()).err_str(),
            "song.update" => self.song_update(p),
            "song.delete" => self.song_delete(p),
            "song.open" => self.song_open(p),
            "section.replace" => self.section_replace(p),
            "section.notes.set" => self.section_notes_set(p),
            "isolation.set" => self.isolation_set(p),
            "section.click.set" => self.section_click_set(p),
            "sectionclick.set" => self.section_click_enable(p),
            "loop.create" => self.loop_create(p),
            "loop.update" => self.loop_update(p),
            "loop.delete" => self.loop_delete(p),
            "loop.fit" => self.loop_fit(p),
            "loop.list" => self.loop_list(p),
            "routine.list" => self.routine_list(p),
            "routine.save" => self.routine_save(p),
            "routine.delete" => self.routine_delete(p),
            "routine.start" => self.routine_start(p),
            "routine.stop" => self.routine_stop(),
            "play" => self.send_ok(EngineCmd::Play),
            "pause" => self.send_ok(EngineCmd::Pause),
            "seek" => self.seek(p),
            "rate" => self.rate(p),
            "volume" => self.volume(p),
            "loop.set" => self.loop_set(p),
            "loop.clear" => self.send_ok(EngineCmd::ClearLoop),
            "bass_focus" => self.bass_focus(p),
            "mix.get" => serde_json::to_value(self.current_mix()).err_str(),
            "mix.set" => self.mix_set(p),
            "mute" => self.mute(p),
            "pitch" => self.pitch(p),
            "countin.set" => self.countin_set(p),
            "metronome.set" => self.metronome_set(p),
            "status" => self.status(),
            // device::list_* returns engine::error::Error (not String), so an
            // extra map_err converts it to String before err_str() takes over.
            "device.outputs" => serde_json::to_value(
                engine::device::list_output_devices().map_err(|e| e.to_string())?,
            )
            .err_str(),
            "device.inputs" => serde_json::to_value(
                engine::device::list_input_devices().map_err(|e| e.to_string())?,
            )
            .err_str(),
            "device.setOutput" => self.device_set_output(p),
            "device.setInput" => self.device_set_input(p),
            "tuner.start" => self.tuner_start(p),
            "tuner.stop" => {
                self.tuner.stop();
                Ok(Value::Null)
            }
            "input.monitorStart" => self.input_monitor_start(p),
            "input.monitorStop" => {
                self.input_monitor.stop();
                Ok(Value::Null)
            }
            "stems.separate" => self.stems_separate(p),
            "stems.status" => self.stems_status(p),
            "stems.gains" => self.stems_gains(p),
            "export.caps" => Ok(json!({ "mp3": engine::encode::ffmpeg_available() })),
            "caps" => Ok(json!({
                "mp3": engine::encode::ffmpeg_available(),
                "stems": self.separator.is_available(),
                "analysis": self.analyzer.is_available(),
            })),
            "export.start" => self.export_start(p),
            "export.cancel" => self.export_cancel(),
            "analysis.run" => self.analysis_run(p),
            "analysis.status" => self.analysis_status(p),
            "analysis.get" => self.analysis_get(p),
            "settings.get_all" => self.settings_get_all(),
            "settings.set" => self.settings_set(p),
            "profiles.list" => self.profiles_list(p),
            "recording.start" => self.recording_start(p),
            "recording.stop" => self.recording_stop(p),
            "recording.list" => Ok(json!(self
                .open_song
                .as_ref()
                .map(|o| self.recordings_view(o.song.id))
                .unwrap_or_default())),
            "recording.rename" => self.recording_rename(p),
            "recording.delete" => self.recording_delete(p),
            "recording.setGain" => self.recording_set_gain(p),
            "recording.setMute" => self.recording_set_mute(p),
            "recording.setNudge" => self.recording_set_nudge(p),
            "recording.calibrate.reset" => self.calibrate_reset(),
            "recording.latency" => self.recording_latency(),
            _ => Err(format!("unknown command: {cmd}")),
        }
    }

    // --- settings -----------------------------------------------------------

    fn settings_get_all(&mut self) -> Result<Value, String> {
        let mut map = serde_json::Map::new();
        for (key, value) in self.store.all_settings().err_str()? {
            map.insert(key, value);
        }
        Ok(Value::Object(map))
    }

    fn settings_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            key: String,
            value: Value,
        }
        let p: P = from_params(p)?;
        self.store.set_setting(&p.key, &p.value).err_str()?;
        Ok(Value::Null)
    }

    // --- device -------------------------------------------------------------

    fn device_set_output(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            id: Option<String>,
        }
        let p: P = from_params(p)?;
        // Normalise empty string to null (= follow system default).
        let id = p.id.filter(|s| !s.is_empty());
        let val = id.clone().map(Value::String).unwrap_or(Value::Null);
        self.store.set_setting("output_device", &val).err_str()?;
        self.audio.set_output_device(id);
        Ok(Value::Null)
    }

    fn device_set_input(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            id: Option<String>,
        }
        let p: P = from_params(p)?;
        // Normalise empty string to null (= follow system default). Input has no
        // live engine stream; the tuner reads this setting on its next start.
        let id = p.id.filter(|s| !s.is_empty());
        let val = id.map(Value::String).unwrap_or(Value::Null);
        self.store.set_setting("input_device", &val).err_str()?;
        Ok(Value::Null)
    }

    fn profiles_list(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default = "default_limit")]
            limit: i64,
        }
        fn default_limit() -> i64 {
            50
        }
        let p: P = from_params(p).unwrap_or(P { limit: 50 });
        serde_json::to_value(self.store.list_profiles(p.limit).err_str()?).err_str()
    }

    // --- transport ---------------------------------------------------------

    fn send_ok(&mut self, cmd: EngineCmd) -> Result<Value, String> {
        self.audio.send(cmd);
        Ok(Value::Null)
    }

    fn seek(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            secs: f64,
        }
        let p: P = from_params(p)?;
        self.send_ok(EngineCmd::SeekSecs(p.secs))
    }

    fn rate(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            value: f64,
        }
        let p: P = from_params(p)?;
        self.send_ok(EngineCmd::SetRate(p.value))
    }

    /// User playback volume → engine multiplier (engine clamps 0.0..=1.5).
    /// Persistence lives in the `playback_volume` setting, written by the UI.
    fn volume(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            value: f32,
        }
        let p: P = from_params(p)?;
        self.send_ok(EngineCmd::SetVolume(p.value))
    }

    fn loop_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            start: f64,
            end: f64,
        }
        let p: P = from_params(p)?;
        self.send_ok(EngineCmd::SetLoopSecs {
            start: p.start,
            end: p.end,
        })
    }

    fn bass_focus(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            on: bool,
        }
        let p: P = from_params(p)?;
        self.current_mix.bass_focus = p.on;
        self.send_ok(EngineCmd::BassFocus(p.on))
    }

    /// The canonical live mix (resolved isolation state).
    pub(crate) fn current_mix(&self) -> Mix {
        self.current_mix
    }

    /// Apply a whole mix to the engine and record it as the live mix. Bass-focus
    /// always applies; stem gains only when the open song actually has stems
    /// (matching the `stems.gains` guard). The single path the routine scheduler
    /// drives when a block becomes active.
    fn apply_mix(&mut self, mix: Mix) {
        self.current_mix = mix;
        self.audio.send(EngineCmd::BassFocus(mix.bass_focus));
        if self.open_song.as_ref().is_some_and(|o| o.stems) {
            for (idx, gain) in mix.stems.into_iter().enumerate() {
                self.audio.send(EngineCmd::SetStemGain { idx, gain });
            }
        }
    }

    fn mix_set(&mut self, p: Value) -> Result<Value, String> {
        let mix: Mix = from_params(p)?;
        self.apply_mix(mix);
        Ok(Value::Null)
    }

    fn mute(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            on: bool,
        }
        let p: P = from_params(p)?;
        self.send_ok(EngineCmd::Mute(p.on))
    }

    fn pitch(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        #[serde(default)]
        struct P {
            semitones: f64,
            cents: f64,
            octave_up: bool,
        }
        impl Default for P {
            fn default() -> Self {
                Self {
                    semitones: 0.0,
                    cents: 0.0,
                    octave_up: false,
                }
            }
        }
        let p: P = from_params(p)?;
        self.send_ok(EngineCmd::SetPitchScale(pitch_scale_factor(
            p.semitones,
            p.cents,
            p.octave_up,
        )))
    }

    fn countin_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            enabled: bool,
            beats: u32,
            loop_mode: String,
        }
        let p: P = from_params(p)?;
        let val = json!({ "enabled": p.enabled, "beats": p.beats, "loop_mode": p.loop_mode });
        self.store.set_setting("count_in", &val).err_str()?;
        self.push_count_in();
        Ok(Value::Null)
    }

    /// Recompute and send the count-in config to the engine from the persisted
    /// setting and the open song's analyzed BPM. The count-in is forced off
    /// (`beats: 0`) when disabled, with no open song, or with no BPM.
    fn push_count_in(&mut self) {
        let cfg = self
            .store
            .get_setting("count_in")
            .ok()
            .flatten()
            .unwrap_or(Value::Null);
        let beats = cfg.get("beats").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        // Migrate the old shape where beats 0 meant off (no `enabled` key).
        let enabled = cfg
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(beats > 0);
        let every_loop = cfg.get("loop_mode").and_then(|v| v.as_str()) == Some("every");
        let bpm = self
            .open_song
            .as_ref()
            .and_then(|o| self.library.get_analysis(o.song.id))
            .and_then(|a| a.bpm);
        let (beats, beat_secs) = match bpm {
            Some(b) if b > 0.0 && enabled && beats > 0 => (beats, beat_secs_from_bpm(b)),
            _ => (0, 0.5),
        };
        self.audio.send(EngineCmd::SetCountIn {
            beats,
            beat_secs,
            every_loop,
        });
    }

    fn metronome_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            running: bool,
            bpm: f64,
            beats_per_bar: u32,
            strong_mask: u32,
            cadence: String,
            kit: String,
        }
        let p: P = from_params(p)?;
        self.store
            .set_setting(
                "metronome",
                &json!({
                    "bpm": p.bpm.clamp(30.0, 300.0),
                    "beats_per_bar": p.beats_per_bar.max(1),
                    "strong_mask": p.strong_mask,
                    "cadence": p.cadence,
                    "kit": p.kit,
                }),
            )
            .err_str()?;
        self.push_metronome(p.running);
        Ok(Value::Null)
    }

    /// Send the persisted metronome config to the engine. `running` is carried
    /// separately (transient) so launch never auto-starts the click.
    fn push_metronome(&mut self, running: bool) {
        let cfg = self
            .store
            .get_setting("metronome")
            .ok()
            .flatten()
            .unwrap_or(Value::Null);
        let bpm = cfg
            .get("bpm")
            .and_then(|v| v.as_f64())
            .unwrap_or(120.0)
            .clamp(30.0, 300.0);
        let beats_per_bar = cfg
            .get("beats_per_bar")
            .and_then(|v| v.as_u64())
            .unwrap_or(4) as u32;
        let strong_mask = cfg.get("strong_mask").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        let cadence = match cfg.get("cadence").and_then(|v| v.as_str()) {
            Some("bar") => Cadence::EveryBar,
            Some("half") => Cadence::HalfBar,
            _ => Cadence::EveryBeat,
        };
        let kit = match cfg.get("kit").and_then(|v| v.as_str()) {
            Some("kick_snare") => Kit::KickSnare,
            Some("cowbell") => Kit::Cowbell,
            _ => Kit::Click,
        };
        self.audio.set_metronome(EngineCmd::SetMetronome {
            running,
            beat_secs: 60.0 / bpm,
            beats_per_bar: beats_per_bar.max(1),
            strong_mask,
            cadence,
            kit,
        });
    }

    /// Recompute the section-click schedule from the persisted master switch,
    /// the open song's sections, and its analyzed beat grid; push it to the
    /// engine. Empty schedule when off, no song open, or no analysis.
    fn push_section_click(&mut self) {
        let enabled = self
            .store
            .get_setting("section_click")
            .ok()
            .flatten()
            .and_then(|v| v.get("enabled").and_then(|e| e.as_bool()))
            .unwrap_or(false);
        let song_id = self.open_song.as_ref().map(|o| o.song.id);
        let marks = match song_id {
            Some(id) if enabled => match self.library.get_analysis(id) {
                Some(a) => {
                    let sections = self.library.list_sections(id);
                    crate::section_click::build_schedule(&a, &sections)
                }
                None => Vec::new(),
            },
            _ => Vec::new(),
        };
        self.audio.set_click_schedule(marks);
    }

    fn section_click_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            section_id: SectionId,
            on: bool,
        }
        let p: P = from_params(p)?;
        let song_id = self
            .open_song
            .as_ref()
            .map(|o| o.song.id)
            .ok_or_else(|| "no song open".to_string())?;
        self.library
            .set_section_click_guide(song_id, p.section_id, p.on)
            .err_str()?;
        self.push_section_click();
        let (sections, orphan_notes) = self.sections_payload(song_id)?;
        Ok(json!({ "sections": sections, "orphan_notes": orphan_notes }))
    }

    fn section_click_enable(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            enabled: bool,
        }
        let p: P = from_params(p)?;
        self.store
            .set_setting("section_click", &json!({ "enabled": p.enabled }))
            .err_str()?;
        self.push_section_click();
        Ok(Value::Null)
    }

    fn status(&self) -> Result<Value, String> {
        let (secs, rate, playing, _count_in) =
            self.last_position.unwrap_or((0.0, 1.0, false, None));
        Ok(json!({
            "position_secs": secs,
            "rate": rate,
            "playing": playing,
            "song_id": self.open_song.as_ref().map(|o| o.song.id),
        }))
    }

    /// Drain engine events and return them for broadcast. Call ~every 50 ms.
    pub fn tick(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        // background-job reports (stem separation done/failed)
        while let Ok(ev) = self.job_rx.try_recv() {
            events.push(ev);
        }
        // finished analyses: persist here (the store is not Sync) and report
        while let Ok((song_id, result)) = self.analysis_rx.try_recv() {
            self.analyzing.remove(&song_id.0);
            let data = match result {
                Ok(a) => match self.library.save_analysis(song_id, &a) {
                    Ok(()) => {
                        // commit the model's section layout as real sections so
                        // structure + loop names are correct with no manual save.
                        // `sections` rides the event so clients refresh without a
                        // round-trip.
                        let sections = match self.commit_analysis_sections(song_id, &a.sections) {
                            Ok(s) => serde_json::to_value(s).unwrap_or(Value::Null),
                            Err(e) => {
                                eprintln!("dredge: auto-save sections after analysis: {e}");
                                Value::Null
                            }
                        };
                        // analysis just gave this song a BPM; if it's the open
                        // song, refresh the engine's count-in (it was forced off
                        // at open time when no BPM existed yet).
                        if self.open_song.as_ref().map(|o| o.song.id) == Some(song_id) {
                            self.push_count_in();
                            self.push_section_click();
                        }
                        json!({"song_id": song_id, "state": "done", "sections": sections})
                    }
                    Err(e) => {
                        json!({"song_id": song_id, "state": "failed", "error": e.to_string()})
                    }
                },
                Err(e) => json!({"song_id": song_id, "state": "failed", "error": e}),
            };
            events.push(Event {
                event: "analysis_progress".into(),
                data,
            });
        }
        // finished profiling runs: persist (store on this thread) then emit
        while let Ok(mut run) = self.profile_rx.try_recv() {
            match self.store.save_profile(&run) {
                Ok(started) => run.started_at = started,
                Err(e) => eprintln!("dredge: profile save failed: {e}"),
            }
            if let Ok(data) = serde_json::to_value(&run) {
                events.push(Event {
                    event: "profile_run".into(),
                    data,
                });
            }
        }
        // live work samples from the sampler thread
        while let Ok(sample) = self.work_sample_rx.try_recv() {
            if let Ok(data) = serde_json::to_value(&sample) {
                events.push(Event {
                    event: "work_sample".into(),
                    data,
                });
            }
        }
        // live tuner readings from the sampler thread
        while let Ok(reading) = self.tuner_rx.try_recv() {
            if let Ok(data) = serde_json::to_value(reading) {
                events.push(Event {
                    event: "tuner_pitch".into(),
                    data,
                });
            }
        }
        // live input levels from the record-arming monitor. Coalesce to the most
        // recent reading — a UI meter only needs the latest, not the backlog.
        let mut level = None;
        while let Ok(l) = self.monitor_rx.try_recv() {
            level = Some(l);
        }
        if let Some(l) = level {
            if let Ok(data) = serde_json::to_value(l) {
                events.push(Event {
                    event: "input_level".into(),
                    data,
                });
            }
        }
        let mut last_pos = None;
        for ev in self.audio.poll_events() {
            match ev {
                EngineEvent::Position {
                    secs,
                    rate,
                    playing,
                    count_in,
                } => last_pos = Some((secs, rate, playing, count_in)),
                EngineEvent::LoopWrapped => {
                    events.push(Event {
                        event: "loop_wrapped".into(),
                        data: Value::Null,
                    });
                    // A running routine advances one pass per wrap; when the
                    // block changes it's applied here and broadcast so clients
                    // animate the new mix/rate/indicator.
                    if let Some(routine_ev) = self.advance_routine_on_wrap() {
                        events.push(routine_ev);
                    }
                }
                EngineEvent::Finished => events.push(Event {
                    event: "song_finished".into(),
                    data: Value::Null,
                }),
                EngineEvent::MetronomeBeat { beat, of, sounded } => {
                    events.push(Event {
                        event: "metronome_beat".into(),
                        data: json!({ "beat": beat, "of": of, "sounded": sounded }),
                    });
                }
            }
        }
        // Only the final Position per tick is broadcast (throttling), and only
        // when it actually changed since the last broadcast — a paused song
        // keeps emitting an identical Position every callback, so without this
        // the pump would serialize+broadcast a no-op ~20x/sec while idle.
        if let Some(next) = last_pos {
            if self.last_position != Some(next) {
                self.last_position = Some(next);
                let (secs, rate, playing, count_in) = next;
                let count_in = count_in.map(|(beat, of)| json!({ "beat": beat, "of": of }));
                events.push(Event {
                    event: "position".into(),
                    data: json!({
                        "secs": secs,
                        "rate": rate,
                        "playing": playing,
                        "count_in": count_in,
                    }),
                });
            }
        }
        // Pin the take's anchor at the first real-playback tick (count-in done):
        // freeze the capture-ring frame that maps to the song's `anchor_frame`
        // via the stream clocks. Snapshots are self-timestamping, so reading them
        // a tick late is fine — and extrapolating to the anchor from *here*
        // (playback start) is a few ms, versus extrapolating back from span end
        // across the held-during-count-in audible frame (which delayed the take
        // by the count-in length).
        if let Some((_secs, _rate, playing, count_in)) = last_pos {
            let anchor = self
                .pending_recording
                .as_ref()
                .filter(|p| p.ring_start.is_none())
                .map(|p| p.anchor_frame);
            if playing && count_in.is_none() {
                if let Some(anchor) = anchor {
                    let play_snap = self.audio.playback_clock_snapshot();
                    let cap = self.recorder.lock().unwrap().capture_snapshot();
                    if let (Some(play), Some((cap_snap, ring_total))) = (play_snap, cap) {
                        let t = play.ns_at_frame(anchor);
                        let ring_start =
                            engine::stream_clock::ring_frame_at_ns(&cap_snap, ring_total, t);
                        if std::env::var("DREDGE_DEBUG").is_ok() {
                            eprintln!(
                                "dredge anchor[count-in-fix]: pinned ring_start={ring_start} \
                                 anchor={anchor} play(now={} ticks={} rate={}) \
                                 cap(now={} ticks={} rate={}) ring_total={ring_total}",
                                play.now_ns,
                                play.ticks,
                                play.rate_hz,
                                cap_snap.now_ns,
                                cap_snap.ticks,
                                cap_snap.rate_hz
                            );
                        }
                        if let Some(p) = self.pending_recording.as_mut() {
                            p.ring_start = Some(ring_start);
                        }
                    }
                }
            }
        }

        // Auto-finalize at span end: once playback reaches the take's end frame,
        // finalize regardless of whether the user has clicked stop yet — this is
        // what locks the take to the span instead of to the stop click. `take()`
        // in `finalize_recording` makes this fire exactly once.
        if let Some((secs, ..)) = last_pos {
            let reached_end = self.pending_recording.as_ref().is_some_and(|p| {
                let end_secs =
                    (p.anchor_frame + p.len_frames) as f64 / engine::buffer::SAMPLE_RATE as f64;
                secs >= end_secs
            });
            if reached_end {
                match self.finalize_recording() {
                    Ok(rec) => {
                        if let Ok(data) = serde_json::to_value(&rec) {
                            events.push(Event {
                                event: "recording.finished".into(),
                                data,
                            });
                        }
                    }
                    Err(e) => eprintln!("dredge: auto-finalize at span end failed: {e}"),
                }
            }
        }
        events
    }

    // --- stems -------------------------------------------------------------

    /// The stems cache dir for a song: `<bundle>/stems`.
    fn stems_cache_dir(&self, song_id: SongId) -> Option<PathBuf> {
        self.library.bundle_dir(song_id).map(|d| d.join("stems"))
    }

    /// All four stem WAVs present in the song's cache dir?
    fn stems_cached(dir: &Path) -> bool {
        STEM_NAMES
            .iter()
            .all(|name| dir.join(format!("{name}.wav")).is_file())
    }

    fn stems_separate(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            /// Re-separate even when stems are cached: the old set is cleared
            /// completely before the run starts, so the song holds either the
            /// fresh separation or (after a failure) no stems at all — never a
            /// stale mix of the two.
            #[serde(default)]
            force: bool,
        }
        let p: P = from_params(p)?;
        let song = self.song_row(p.song_id)?;
        let cache = self
            .stems_cache_dir(p.song_id)
            .ok_or("song not in library")?;
        if p.force {
            for name in STEM_NAMES {
                let _ = std::fs::remove_file(cache.join(format!("{name}.wav")));
            }
        } else if Self::stems_cached(&cache) {
            return Ok(json!({"state": "cached"}));
        }
        {
            let mut running = self.separating.lock().unwrap();
            if running.contains(&p.song_id.0) {
                // refuse double-start: the existing job keeps running
                return Ok(json!({"state": "running"}));
            }
            if !self.separator.is_available() {
                return Err(
                    "demucs not installed — install with: uv tool install demucs --with torchcodec \
                     (CUDA torch pulls ~2.5 GB; torchcodec is required by torchaudio 2.9+)"
                        .into(),
                );
            }
            running.insert(p.song_id.0);
        }
        let separator = self.separator.clone();
        let tx = self.job_tx.clone();
        let profile_tx = self.profile_tx.clone();
        let separating = self.separating.clone();
        let audio_path = PathBuf::from(&song.path);
        let song_id = p.song_id;
        let force_cpu = self
            .store
            .get_setting("analysis_device")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_owned))
            .map(|s| s == "cpu")
            .unwrap_or(false);
        let device = if force_cpu { "cpu" } else { "auto" }.to_string();
        let reporter = self.work_reporter();
        std::thread::spawn(move || {
            reporter.begin("stems", "separating stems");
            let mut timer = crate::profile::Timer::new("stems", Some(song_id));
            // Decode to a canonical WAV first so Demucs reads pure PCM (no
            // ffmpeg) and video sources work; the temp dir lives until the job
            // ends.
            let prepared = timer.stage("decode", || canonical_wav_for_tools(&audio_path));
            let result = match &prepared {
                Ok((_dir, wav)) => {
                    timer.stage("demucs", || separator.separate(wav, &cache, force_cpu))
                }
                Err(e) => Err(e.clone()),
            };
            let m = reporter.maxes();
            reporter.end();
            separating.lock().unwrap().remove(&song_id.0);
            let err = result.as_ref().err().cloned();
            let mut run = timer.finish(result.is_ok(), err.clone(), Some(device), None);
            if let Some((cpu, gpu, vram_used, vram_total)) = m {
                run.max_cpu_pct = Some(cpu);
                run.max_gpu_util = gpu;
                run.max_vram_used_mb = vram_used;
                run.vram_total_mb = vram_total;
            }
            let data = match result {
                Ok(_) => json!({"song_id": song_id, "state": "done"}),
                Err(e) => json!({"song_id": song_id, "state": "failed", "error": e}),
            };
            let _ = tx.send(Event {
                event: "stems_progress".into(),
                data,
            });
            let _ = profile_tx.send(run);
        });
        Ok(json!({"state": "running"}))
    }

    fn stems_status(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        let cache = self
            .stems_cache_dir(p.song_id)
            .ok_or("song not in library")?;
        let state = if self.separating.lock().unwrap().contains(&p.song_id.0) {
            "running"
        } else if Self::stems_cached(&cache) {
            "cached"
        } else {
            "none"
        };
        Ok(json!({"state": state}))
    }

    fn stems_gains(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            gains: [f32; practice::model::STEM_COUNT],
        }
        let p: P = from_params(p)?;
        let open = self.open_song.as_ref().ok_or("no song open")?;
        if !open.stems {
            return Err("no stems loaded for the open song".into());
        }
        self.current_mix.stems = p.gains;
        for (idx, gain) in p.gains.into_iter().enumerate() {
            self.audio.send(EngineCmd::SetStemGain { idx, gain });
        }
        Ok(Value::Null)
    }

    // --- export ------------------------------------------------------------

    /// Render the song to disk on a background thread, baking the supplied mix
    /// (stem gains, rate, pitch, bass focus) into the requested span. The mix
    /// comes from the caller (the UI mirrors the live engine), so the file
    /// matches what's heard — master volume excluded by construction. Progress
    /// and the terminal result arrive as `export_progress` events.
    fn export_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            dir: String,
            filename: String,
            format: String,
            #[serde(default)]
            start_secs: Option<f64>,
            #[serde(default)]
            end_secs: Option<f64>,
            #[serde(default = "one")]
            rate: f64,
            #[serde(default)]
            semitones: f64,
            #[serde(default)]
            cents: f64,
            #[serde(default)]
            octave_up: bool,
            #[serde(default)]
            bass_focus: bool,
            #[serde(default)]
            gains: Vec<f32>,
        }
        fn one() -> f64 {
            1.0
        }
        let p: P = from_params(p)?;
        if p.format != "wav" && p.format != "mp3" {
            return Err(format!("unknown export format: {}", p.format));
        }
        if p.format == "mp3" && !engine::encode::ffmpeg_available() {
            return Err("MP3 export needs ffmpeg, which isn't installed".into());
        }
        let dir = resolve_export_dir(&p.dir)?;
        validate_export_target(&dir, &p.filename)?;
        let filename = p.filename.trim().to_string();
        let song = self.song_row(p.song_id)?;
        let stems_cache = self
            .stems_cache_dir(p.song_id)
            .ok_or("song not in library")?;
        let cfg = engine::export::RenderConfig {
            start_secs: p.start_secs.unwrap_or(0.0),
            end_secs: p.end_secs,
            rate: p.rate,
            pitch_scale: pitch_scale_factor(p.semitones, p.cents, p.octave_up),
            bass_focus: p.bass_focus,
            gains: p.gains,
        };

        // One export at a time: signal any prior render to stop, arm a fresh
        // flag this thread owns.
        self.export_cancel.store(true, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        self.export_cancel = cancel.clone();

        let tx = self.job_tx.clone();
        let format = p.format;

        std::thread::spawn(move || {
            let emit = |data: Value| {
                let _ = tx.send(Event {
                    event: "export_progress".into(),
                    data,
                });
            };
            emit(json!({ "state": "decoding" }));
            let set = match export_decode(&song, &stems_cache) {
                Ok(set) => set,
                Err(e) => return emit(json!({ "state": "failed", "error": e })),
            };
            if cancel.load(Ordering::SeqCst) {
                return emit(json!({ "state": "cancelled" }));
            }

            let mut last_pct = -2i32;
            let samples = engine::export::render_with_progress(&set, &cfg, &mut |frac| {
                if cancel.load(Ordering::SeqCst) {
                    return false;
                }
                let pct = (frac * 100.0) as i32;
                if pct >= last_pct + 2 {
                    last_pct = pct;
                    emit(json!({ "state": "rendering", "percent": pct }));
                }
                true
            });
            if cancel.load(Ordering::SeqCst) {
                return emit(json!({ "state": "cancelled" }));
            }

            let ext = if format == "mp3" { "mp3" } else { "wav" };
            let out_path = unique_export_path(&dir, &filename, ext);
            let write = if format == "mp3" {
                emit(json!({ "state": "encoding" }));
                let tmp = dir.join(format!(".{filename}.export.wav"));
                let r = engine::capture::write_wav(&tmp, &samples)
                    .err_str()
                    .and_then(|()| engine::encode::encode_mp3(&tmp, &out_path, 320).err_str());
                let _ = std::fs::remove_file(&tmp);
                r
            } else {
                engine::capture::write_wav(&out_path, &samples).err_str()
            };
            match write {
                Ok(()) => {
                    let bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                    emit(json!({
                        "state": "done",
                        "path": out_path.to_string_lossy(),
                        "bytes": bytes,
                    }));
                }
                Err(e) => emit(json!({ "state": "failed", "error": e })),
            }
        });
        Ok(json!({ "state": "started" }))
    }

    /// Ask the in-flight export (if any) to stop. The render checks this
    /// between blocks and emits a `cancelled` event; no file is written.
    fn export_cancel(&mut self) -> Result<Value, String> {
        self.export_cancel.store(true, Ordering::SeqCst);
        Ok(Value::Null)
    }

    // --- analysis ----------------------------------------------------------

    /// Kick off background analysis (beat grid + suggested sections).
    /// Results are cached per song; a second run reports `cached`.
    fn analysis_run(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            #[serde(default)]
            force: bool,
        }
        let p: P = from_params(p)?;
        let song = self.song_row(p.song_id)?;
        if !p.force && self.library.has_analysis(p.song_id) {
            return Ok(json!({"state": "cached"}));
        }
        if self.analyzing.contains(&p.song_id.0) {
            // refuse double-start: the existing job keeps running
            return Ok(json!({"state": "running"}));
        }
        if !self.analyzer.is_available() {
            return Err(
                "analysis script not found — expected <repo>/scripts/analyze (or set $DREDGE_ANALYZE)"
                    .into(),
            );
        }
        self.analyzing.insert(p.song_id.0);
        let analyzer = self.analyzer.clone();
        let tx = self.analysis_tx.clone();
        let profile_tx = self.profile_tx.clone();
        let audio_path = PathBuf::from(&song.path);
        let song_id = p.song_id;
        let device_setting = self
            .store
            .get_setting("analysis_device")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "auto".into());
        let reporter = self.work_reporter();
        std::thread::spawn(move || {
            let first_stage = if device_setting == "cpu" {
                "analyzing structure"
            } else {
                "GPU attempt"
            };
            reporter.begin("analysis", first_stage);
            let mut timer = crate::profile::Timer::new("analysis", Some(song_id));
            // Decode to a canonical WAV first so the Python analyzer reads pure
            // PCM (no ffmpeg) and video sources work; the temp dir lives until
            // the job ends.
            let prepared = timer.stage("decode", || canonical_wav_for_tools(&audio_path));
            let (result, device) = match &prepared {
                Ok((_dir, wav)) => crate::analysis::analyze_with_recovery(
                    analyzer.as_ref(),
                    wav,
                    &device_setting,
                    &mut timer,
                    &reporter,
                ),
                Err(e) => (Err(e.clone()), None),
            };
            let m = reporter.maxes();
            reporter.end();
            let engine = result.as_ref().ok().map(|a| a.engine.clone());
            let err = result.as_ref().err().cloned();
            let mut run = timer.finish(result.is_ok(), err, device, engine);
            if let Some((cpu, gpu, vram_used, vram_total)) = m {
                run.max_cpu_pct = Some(cpu);
                run.max_gpu_util = gpu;
                run.max_vram_used_mb = vram_used;
                run.vram_total_mb = vram_total;
            }
            let _ = tx.send((song_id, result));
            let _ = profile_tx.send(run);
        });
        Ok(json!({"state": "running"}))
    }

    fn analysis_status(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        let state = if self.analyzing.contains(&p.song_id.0) {
            "running"
        } else if self.library.has_analysis(p.song_id) {
            "cached"
        } else {
            "none"
        };
        Ok(json!({"state": state}))
    }

    fn analysis_get(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        serde_json::to_value(self.library.get_analysis(p.song_id)).err_str()
    }

    // --- tuner -------------------------------------------------------------

    fn tuner_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            device_id: String,
        }
        let p: P = from_params(p)?;
        self.tuner.start(&p.device_id, self.tuner_tx.clone())?;
        Ok(Value::Null)
    }

    // --- input monitor -----------------------------------------------------

    fn input_monitor_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            device_id: String,
        }
        let p: P = from_params(p)?;
        self.input_monitor
            .start(&p.device_id, self.monitor_tx.clone())?;
        Ok(Value::Null)
    }

    // --- recording ---------------------------------------------------------

    /// Read a stored frame count, or `None` when the setting is unset.
    fn latency_setting(&self, key: &str) -> Option<i64> {
        self.store
            .get_setting(key)
            .ok()
            .flatten()
            .and_then(|v| v.as_i64())
    }

    /// The current latency source: `"loopback"` if a calibration is active, else
    /// `"auto"` (the default).
    fn latency_source(&self) -> String {
        self.store
            .get_setting(LATENCY_SOURCE_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "auto".to_string())
    }

    /// Active round-trip input latency in frames, DERIVED from the source: the
    /// loopback value when calibrated, else the auto baseline (0 when unset).
    fn input_latency_frames(&self) -> i64 {
        let key = if self.latency_source() == "loopback" {
            INPUT_LATENCY_LOOPBACK_KEY
        } else {
            INPUT_LATENCY_AUTO_KEY
        };
        self.latency_setting(key).unwrap_or(0)
    }

    /// Latency status for the devices readout: both stored measurements (null
    /// when unset) and which one is in use.
    fn recording_latency(&self) -> Result<Value, String> {
        Ok(json!({
            "auto_frames": self.latency_setting(INPUT_LATENCY_AUTO_KEY),
            "loopback_frames": self.latency_setting(INPUT_LATENCY_LOOPBACK_KEY),
            "source": self.latency_source(),
        }))
    }

    /// Drop a loopback calibration: reset `latency_source` to `"auto"` so the
    /// auto baseline becomes active again. The calibrated value stays stored
    /// (inactive) so the devices readout still shows it.
    fn calibrate_reset(&mut self) -> Result<Value, String> {
        self.store
            .set_setting(LATENCY_SOURCE_KEY, &json!("auto"))
            .err_str()?;
        self.refresh_layers();
        Ok(json!({ "source": "auto" }))
    }

    /// A take's wire form: the persisted `Recording` flattened with its
    /// (non-persisted) waveform peaks, so the frontend draws a real waveform
    /// lane. `peaks` is null when the take's audio failed to decode.
    fn recording_view(&self, r: &Recording) -> Value {
        let mut v = serde_json::to_value(r).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut v {
            let peaks = self
                .layer_peaks
                .get(&r.id)
                .and_then(|p| serde_json::to_value(p).ok())
                .unwrap_or(Value::Null);
            map.insert("peaks".into(), peaks);
        }
        v
    }

    /// All takes for `song_id` as wire views (Recording + peaks).
    fn recordings_view(&self, song_id: SongId) -> Vec<Value> {
        self.library
            .recordings(song_id)
            .iter()
            .map(|r| self.recording_view(r))
            .collect()
    }

    /// Rebuild the engine's layer set from the open song's recordings. A take
    /// whose WAV fails to decode is logged and skipped — never fails the caller.
    fn refresh_layers(&mut self) {
        let Some(open) = self.open_song.as_ref() else {
            self.audio.set_layers(Vec::new());
            return;
        };
        let song_id = open.song.id;
        let Some(dir) = self.library.bundle_dir(song_id) else {
            return;
        };
        let latency = self.input_latency_frames();
        let mut layers = Vec::new();
        for r in self.library.recordings(song_id) {
            // Reuse the decoded buffer when cached (a take's audio never
            // changes); otherwise decode once and cache. A bad file is logged
            // and skipped, never aborting the rest.
            let samples = if let Some(buf) = self.layer_cache.get(&r.id) {
                buf.clone()
            } else {
                let path = dir.join(&r.file);
                match engine::decode::decode_file(&path) {
                    Ok(buf) => {
                        let buf = Arc::new(buf);
                        self.layer_cache.insert(r.id, buf.clone());
                        buf
                    }
                    Err(e) => {
                        eprintln!(
                            "dredge: skipping recording layer {} ({}): {e}",
                            r.id.0,
                            path.display()
                        );
                        continue;
                    }
                }
            };
            // Compute this take's waveform peaks once from the decoded buffer
            // (a recomputable cache, never persisted); the frontend draws them
            // as the take's waveform lane.
            self.layer_peaks
                .entry(r.id)
                .or_insert_with(|| engine::peaks::compute_peaks(&samples));
            layers.push(engine::layers::Layer {
                samples,
                start_frame: r.anchor_frame - latency - r.nudge_frames,
                gain: r.gain,
                muted: r.muted,
            });
        }
        self.audio.set_layers(layers);
    }

    fn recording_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            span: String,
            #[serde(default)]
            start: Option<f64>,
            #[serde(default)]
            end: Option<f64>,
            device_id: String,
        }
        if self.pending_recording.is_some() {
            return Err("already recording".into());
        }
        // The recorder is about to open its own capture on this device; stop the
        // arming meter first so two capture sessions don't fight for the input.
        self.input_monitor.stop();
        let p: P = from_params(p)?;
        let open = self.open_song.as_ref().ok_or("no song open")?;
        let song_id = open.song.id;
        let song_frames =
            (open.song.duration_secs * engine::buffer::SAMPLE_RATE as f64).round() as i64;
        let span = match p.span.as_str() {
            "song" => crate::recording::Span::Song,
            "selection" => crate::recording::Span::Selection {
                start: p.start.ok_or("selection span needs start/end")?,
                end: p.end.ok_or("selection span needs start/end")?,
            },
            "loop" => crate::recording::Span::Loop {
                start: p.start.ok_or("loop span needs start/end")?,
                end: p.end.ok_or("loop span needs start/end")?,
            },
            other => return Err(format!("unknown span: {other}")),
        };
        let (start, end) = crate::recording::resolve_span(span, song_frames).ok_or("empty span")?;
        let len_frames = end - start;
        {
            let mut rec = self.recorder.lock().unwrap();
            rec.start(&p.device_id, len_frames)?;
            // Arm both stream clocks so the capture and output RT threads begin
            // publishing timing snapshots; the take is anchored against them at
            // finalize.
            rec.arm_clock();
        }
        self.audio.arm_playback_clock();
        self.pending_recording = Some(PendingRecording {
            song_id,
            anchor_frame: start,
            len_frames,
            ring_start: None,
        });
        self.audio.send(EngineCmd::SeekSecs(
            start as f64 / engine::buffer::SAMPLE_RATE as f64,
        ));
        self.audio.send(EngineCmd::Play);
        Ok(Value::Null)
    }

    /// Finalize the in-flight take: extract the transport-locked range from the
    /// capture ring (anchored to the song timeline via the capture/playback
    /// clocks), write the WAV, append the manifest entry, rebuild layers, and
    /// disarm both clocks. Shared by the user early-stop path
    /// (`recording_stop`) and the auto-finalize-at-span-end path (`tick`).
    fn finalize_recording(&mut self) -> Result<Recording, String> {
        let pending = self.pending_recording.take().ok_or("not recording")?;
        if std::env::var("DREDGE_DEBUG").is_ok() {
            eprintln!(
                "dredge finalize[count-in-fix]: ring_start={:?} anchor={} len={}",
                pending.ring_start, pending.anchor_frame, pending.len_frames
            );
        }
        // Read PipeWire's input-stream delay while the capture session is still
        // alive (before `rec.stop()` tears it down) and carry it out of the
        // block; it pairs with the output delay below to form the auto RTL
        // baseline.
        let (samples, input_delay, effective_anchor) = {
            let mut rec = self.recorder.lock().unwrap();
            let input_delay = rec.input_delay_frames();
            // `ring_start` was pinned at the first real-playback tick (see `tick`).
            // Ring-frame-0 maps to song frame (anchor − ring_start). A NEGATIVE
            // `ring_start` means capture began AFTER the anchor — recording from a
            // mid-song playhead, where playback is already at the anchor when
            // recording starts. In that case extract from the earliest captured
            // frame (0) and shift the take's anchor later by the missing amount so
            // it stays aligned, instead of failing the read and re-including the
            // pre-anchor tail via snapshot_last. `None` means the clocks never
            // published — fall back to snapshot_last.
            let mut effective_anchor = pending.anchor_frame;
            let extracted = pending.ring_start.and_then(|ring_start| {
                let extract_start = ring_start.max(0);
                effective_anchor = pending.anchor_frame + (extract_start - ring_start);
                // Clamp to what was actually captured: an early stop (or a
                // "full song" span cut short) has fewer than `len_frames` frames;
                // overrunning fails the read and falls back to snapshot_last
                // (which re-includes the count-in).
                let available = rec
                    .capture_snapshot()
                    .map(|(_, total)| (total - extract_start).max(0))
                    .unwrap_or(pending.len_frames);
                let take_len = pending.len_frames.min(available);
                if std::env::var("DREDGE_DEBUG").is_ok() {
                    eprintln!(
                        "dredge finalize[count-in-fix]: ring_start={ring_start} \
                         extract_start={extract_start} effective_anchor={effective_anchor} \
                         len_frames={} available={available} take_len={take_len}",
                        pending.len_frames
                    );
                }
                let got = rec.extract_range(extract_start, take_len);
                if got.is_none() {
                    eprintln!(
                        "dredge: recording range evicted (extract_start={extract_start}, \
                         take_len={take_len}); falling back to snapshot_last"
                    );
                }
                got
            });
            rec.disarm_clock();
            // `stop()` tears down the capture session (joins its thread) in all
            // paths; its snapshot_last result is the fallback when the
            // transport-locked range was unavailable.
            let samples = match extracted {
                Some(s) => {
                    let _ = rec.stop();
                    s
                }
                None => rec.stop()?,
            };
            (samples, input_delay, effective_anchor)
        };
        self.audio.disarm_playback_clock();
        self.audio.send(EngineCmd::Pause);
        // Auto RTL baseline: the round-trip latency PipeWire reports (output
        // buffering + input buffering). Always recorded so the estimate stays
        // current — even while a loopback calibration is the active source; which
        // value `refresh_layers` actually applies is decided by `latency_source`.
        let output_delay = self.audio.output_delay_frames();
        let rtl = (output_delay + input_delay).max(0);
        if std::env::var("DREDGE_DEBUG").is_ok() {
            eprintln!(
                "dredge rtl[auto]: output_delay={output_delay} input_delay={input_delay} rtl={rtl}"
            );
        }
        self.store
            .set_setting(INPUT_LATENCY_AUTO_KEY, &json!(rtl))
            .err_str()?;
        let dir = self
            .library
            .bundle_dir(pending.song_id)
            .ok_or("song not in library")?;
        let mut recordings = self.library.recordings(pending.song_id);
        let next_id = recordings.iter().map(|r| r.id.0).max().unwrap_or(0) + 1;
        let file = format!("recordings/{next_id}.wav");
        let abs = dir.join(&file);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).err_str()?;
        }
        engine::capture::write_wav(&abs, &samples).err_str()?;
        let len_frames = (samples.len() / engine::buffer::CHANNELS) as i64;
        let rec = Recording {
            id: RecordingId(next_id),
            name: format!("take {next_id}"),
            file,
            anchor_frame: effective_anchor,
            len_frames,
            nudge_frames: 0,
            gain: 1.0,
            muted: false,
            created_at: now_rfc3339(),
        };
        recordings.push(rec.clone());
        self.library
            .set_recordings(pending.song_id, recordings)
            .err_str()?;
        self.refresh_layers();
        Ok(rec)
    }

    fn recording_stop(&mut self, _p: Value) -> Result<Value, String> {
        let rec = self.finalize_recording()?;
        // `finalize_recording` ran `refresh_layers`, so the take's peaks are
        // cached — ship them with the finished take so its lane draws at once.
        let data = self.recording_view(&rec);
        let _ = self.job_tx.send(Event {
            event: "recording.finished".into(),
            data: data.clone(),
        });
        Ok(data)
    }

    /// Load this song's recordings, mutate the one matching `id`, persist, and
    /// rebuild layers. Shared by rename/gain/mute/nudge. Requires an open song.
    fn recording_mutate(
        &mut self,
        id: RecordingId,
        f: impl FnOnce(&mut Recording),
    ) -> Result<Value, String> {
        let song_id = self.open_song.as_ref().ok_or("no song open")?.song.id;
        let mut recordings = self.library.recordings(song_id);
        let rec = recordings
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("recording not found: {}", id.0))?;
        f(rec);
        self.library.set_recordings(song_id, recordings).err_str()?;
        self.refresh_layers();
        Ok(Value::Null)
    }

    fn recording_rename(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            id: RecordingId,
            name: String,
        }
        let p: P = from_params(p)?;
        self.recording_mutate(p.id, |r| r.name = p.name)
    }

    fn recording_set_gain(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            id: RecordingId,
            gain: f32,
        }
        let p: P = from_params(p)?;
        self.recording_mutate(p.id, |r| r.gain = p.gain)
    }

    fn recording_set_mute(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            id: RecordingId,
            muted: bool,
        }
        let p: P = from_params(p)?;
        self.recording_mutate(p.id, |r| r.muted = p.muted)
    }

    fn recording_set_nudge(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            id: RecordingId,
            nudge_ms: f64,
        }
        let p: P = from_params(p)?;
        let frames = (p.nudge_ms / 1000.0 * engine::buffer::SAMPLE_RATE as f64).round() as i64;
        self.recording_mutate(p.id, |r| r.nudge_frames = frames)
    }

    fn recording_delete(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            id: RecordingId,
        }
        let p: P = from_params(p)?;
        let song_id = self.open_song.as_ref().ok_or("no song open")?.song.id;
        let dir = self
            .library
            .bundle_dir(song_id)
            .ok_or("song not in library")?;
        let mut recordings = self.library.recordings(song_id);
        let pos = recordings
            .iter()
            .position(|r| r.id == p.id)
            .ok_or_else(|| format!("recording not found: {}", p.id.0))?;
        let removed = recordings.remove(pos);
        self.layer_cache.remove(&removed.id);
        self.layer_peaks.remove(&removed.id);
        // Best-effort WAV removal: a missing file shouldn't fail the command.
        if let Err(e) = std::fs::remove_file(dir.join(&removed.file)) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("dredge: recording WAV cleanup failed: {e}");
            }
        }
        self.library.set_recordings(song_id, recordings).err_str()?;
        self.refresh_layers();
        Ok(Value::Null)
    }

    // --- library ---------------------------------------------------------

    fn song_import(&mut self, p: Value) -> Result<Value, String> {
        let p: ImportParams = from_params(p)?;
        let hash = engine::decode::file_hash(Path::new(&p.path)).err_str()?;
        if let Some(existing) = self.import_lookup(&hash)? {
            return serde_json::to_value(existing).err_str();
        }
        let prep = import_decode(p.path, p.title, hash)?;
        self.import_prepared(prep)
    }

    /// State phase of `song.update`: refuse while a job for this song is
    /// running, rename the bundle (via the library) so the folder tracks the
    /// new name, and report whether the open song needs reopening. A reopen is
    /// only needed when the folder actually moved — a metadata-only edit that
    /// slugs to the same dir leaves playback untouched.
    fn update_apply(&mut self, p: Value) -> Result<(Song, Option<SongId>), String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            title: String,
            // omitted artist clears it — socket/script clients can send {title} alone
            #[serde(default)]
            artist: Option<String>,
        }
        let p: P = from_params(p)?;

        // A rename moves the bundle dir; a stems/analysis job for this song
        // captured the old path up front and writes into it from another
        // thread. Moving the dir under it would silently lose its output, so
        // refuse.
        if self.analyzing.contains(&p.song_id.0)
            || self.separating.lock().unwrap().contains(&p.song_id.0)
        {
            return Err("can't rename while stems or analysis are running for this song".into());
        }

        let before = self.library.bundle_dir(p.song_id);
        let song = self
            .library
            .update_song(p.song_id, &p.title, p.artist.as_deref())
            .err_str()?;
        let moved = self.library.bundle_dir(p.song_id) != before;
        let _ = self.job_tx.send(Event {
            event: "library_changed".into(),
            data: Value::Null,
        });

        // Reopen only when the folder actually moved, and only for the open song.
        let is_open = self.open_song.as_ref().map(|o| o.song.id) == Some(p.song_id);
        let reopen = (moved && is_open).then_some(p.song_id);
        // Metadata-only edit to the open song (no move, so no reopen): sync its
        // header in place, since nothing else will.
        if is_open && !moved {
            if let Some(o) = self.open_song.as_mut() {
                o.song = song.clone();
            }
        }
        Ok((song, reopen))
    }

    fn song_update(&mut self, p: Value) -> Result<Value, String> {
        // Inline fallback (direct `App::dispatch`): decode under the lock, the
        // same accepted tradeoff as the inline `song_open`. The pump path uses
        // the phased `update_phased` instead.
        let (song, reopen) = self.update_apply(p)?;
        if let Some(song_id) = reopen {
            let (s, stems_cache) = self.open_lookup(song_id)?;
            let decoded = open_decode(&s, &stems_cache)?;
            self.finish_open(s, decoded)?;
        }
        serde_json::to_value(song).err_str()
    }

    fn song_delete(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        // capture path + hash before the row is gone — cleanup needs them
        let song = self.song_row(p.song_id)?;

        // stop playback and drop the handle if we're deleting the open song
        if self.open_song.as_ref().map(|o| o.song.id) == Some(p.song_id) {
            self.audio.send(EngineCmd::Pause);
            self.open_song = None;
        }

        // remove the whole bundle dir (audio + stems + manifest)
        self.library.delete_song(p.song_id).err_str()?;

        // peaks live outside the bundle as a recomputable cache; best-effort
        // cleanup so a failed removal logs but does not fail the command
        if let Err(e) = engine::peaks::remove_cache(&song.file_hash) {
            eprintln!("dredge: peaks cleanup failed for {}: {e}", song.file_hash);
        }

        let _ = self.job_tx.send(Event {
            event: "library_changed".into(),
            data: Value::Null,
        });
        Ok(Value::Null)
    }

    /// `song.import` dedupe check (needs the lock): a song with this content
    /// hash already exists → return it instead of re-importing.
    fn import_lookup(&self, hash: &str) -> Result<Option<Song>, String> {
        Ok(self.library.song_by_hash(hash))
    }

    /// `song.import` final phase (needs the lock): create the bundle and
    /// announce the library change.
    fn import_prepared(&mut self, prep: ImportPrepared) -> Result<Value, String> {
        // re-check the hash: under `dispatch_shared` another client may have
        // imported the same file between the lookup and this phase
        if let Some(existing) = self.library.song_by_hash(&prep.hash) {
            return serde_json::to_value(existing).err_str();
        }
        let song = self
            .library
            .create_song(
                Path::new(&prep.path),
                &prep.title,
                None,
                &prep.hash,
                prep.duration_secs,
            )
            .err_str()?;
        // socket-driven imports refresh every client's library on the next tick
        let _ = self.job_tx.send(Event {
            event: "library_changed".into(),
            data: Value::Null,
        });
        serde_json::to_value(song).err_str()
    }

    fn song_open(&mut self, p: Value) -> Result<Value, String> {
        let p: OpenParams = from_params(p)?;
        let (song, stems_cache) = self.open_lookup(p.song_id)?;
        let decoded = open_decode(&song, &stems_cache)?;
        self.finish_open(song, decoded)
    }

    /// `song.open` phase 1 (needs the lock): resolve the song row and the
    /// stems cache dir for it.
    fn open_lookup(&self, song_id: SongId) -> Result<(Song, PathBuf), String> {
        let song = self.song_row(song_id)?;
        let cache = self.stems_cache_dir(song_id).ok_or("song not in library")?;
        Ok((song, cache))
    }

    /// `song.open` final phase (needs the lock): load the engine, build the
    /// response, set the open song.
    fn finish_open(&mut self, song: Song, decoded: OpenDecoded) -> Result<Value, String> {
        let song_id = song.id;
        self.audio.load(decoded.set);
        let (sections, orphan_notes) = self.sections_payload(song_id)?;
        let mut out = json!({
            "song": song,
            "sections": sections,
            "loops": self.library.list_loops(song_id),
            "routines": self.library.list_routines(song_id),
            "peaks": decoded.peaks,
            "stems": decoded.stems,
            "analysis": self.library.get_analysis(song_id),
            "isolation": self.library.get_isolation(song_id),
            "orphan_notes": orphan_notes,
            // `recordings` is attached below, after `refresh_layers` computes
            // this song's take peaks (the views need them).
        });
        self.open_song = Some(OpenSong {
            song,
            stems: decoded.stems,
        });
        // A freshly opened song starts at full band, no listening aid — the
        // engine loads stems at unity and the UI resets its isolation stores to
        // match, so the canonical mix tracks that. Any prior routine run ends.
        self.current_mix = Mix::default();
        self.active_routine = None;
        // The layer cache only ever holds the open song's takes, and recording
        // ids are assigned per-song (each song's takes start at 1). Clearing on
        // every open makes cross-song id collisions impossible — a take from the
        // previous song can't be served as this song's same-id layer. The new
        // song re-decodes its own takes once via `refresh_layers`.
        self.layer_cache.clear();
        self.layer_peaks.clear();
        // Attach this song's overdub layers (a reopened song restores its takes).
        // This also computes each take's peaks into `layer_peaks`, so build the
        // recordings views only after it runs.
        self.refresh_layers();
        out["recordings"] = json!(self.recordings_view(song_id));
        self.push_count_in();
        self.push_section_click();
        Ok(out)
    }

    /// Build the open-song `sections` array (each section enriched with its
    /// occurrence `label` and stored `notes`) plus the `orphan_notes` list
    /// (stored notes whose label matches no current section). Shared by
    /// `song.open`, `section.replace`, and `section.notes.set`.
    fn sections_payload(&self, song_id: SongId) -> Result<(Value, Value), String> {
        let sections = self.library.list_sections(song_id);
        let notes: std::collections::HashMap<String, NotesDoc> = self
            .library
            .list_section_notes(song_id)
            .into_iter()
            .collect();
        let mut used: HashSet<String> = HashSet::new();
        let enriched: Vec<Value> = sections
            .iter()
            .map(|s| {
                let label = practice::naming::occurrence_label(s, &sections);
                used.insert(label.clone());
                let mut v = serde_json::to_value(s).expect("section serializes");
                v["label"] = json!(label);
                v["notes"] = serde_json::to_value(notes.get(&label)).expect("doc serializes");
                v
            })
            .collect();
        let orphans: Vec<Value> = notes
            .iter()
            .filter(|(label, _)| !used.contains(label.as_str()))
            .map(|(label, doc)| json!({ "label": label, "doc": doc }))
            .collect();
        Ok((json!(enriched), json!(orphans)))
    }

    fn section_replace(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct SecIn {
            name: String,
            start: f64,
            end: f64,
            position: i32,
            #[serde(default)]
            click_guide: bool,
        }
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            sections: Vec<SecIn>,
        }
        let p: P = from_params(p)?;
        let news: Vec<NewSection> = p
            .sections
            .iter()
            .map(|s| NewSection {
                name: &s.name,
                start: s.start,
                end: s.end,
                position: s.position,
                click_guide: s.click_guide,
            })
            .collect();
        self.commit_sections(p.song_id, &news)?;
        // Section spans changed, so the engine's beat-click schedule is stale
        // (it may still be clicking deleted/moved spans). Rebuild it now. The
        // post-analysis auto-commit path calls push_section_click separately, so
        // this stays in section_replace to avoid a double-call there.
        self.push_section_click();
        let (sections, orphan_notes) = self.sections_payload(p.song_id)?;
        Ok(json!({ "sections": sections, "orphan_notes": orphan_notes }))
    }

    fn section_notes_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            label: String,
            doc: NotesDoc,
        }
        let p: P = from_params(p)?;
        let song_id = self
            .open_song
            .as_ref()
            .map(|o| o.song.id)
            .ok_or_else(|| "no song open".to_string())?;
        p.doc.validate()?;
        self.library
            .set_section_notes(song_id, &p.label, &p.doc)
            .err_str()?;
        let (sections, orphan_notes) = self.sections_payload(song_id)?;
        Ok(json!({ "sections": sections, "orphan_notes": orphan_notes }))
    }

    /// Persist the open song's isolation-box state (bass focus + per-stem
    /// levels/mutes/solos) into its bundle manifest. Pure persistence — the live
    /// engine gains are already applied by `bass_focus` / `stems.gains`. The
    /// `song_id` is passed explicitly so a debounced save that lands after a
    /// song switch still writes the song it was captured for.
    fn isolation_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            #[serde(default)]
            bass_focus: bool,
            #[serde(default)]
            levels: Vec<u8>,
            #[serde(default)]
            mutes: Vec<bool>,
            #[serde(default)]
            solos: Vec<bool>,
        }
        let p: P = from_params(p)?;
        self.library
            .set_isolation(
                p.song_id,
                practice::model::Isolation {
                    bass_focus: p.bass_focus,
                    levels: p.levels,
                    mutes: p.mutes,
                    solos: p.solos,
                },
            )
            .err_str()?;
        Ok(json!({ "ok": true }))
    }

    /// Persist a section layout and rename the dynamic loops. Shared by the
    /// `section.replace` command and the post-analysis auto-commit. Also clears
    /// any auto-derived transition loops left over from
    /// when those were created automatically (the app no longer makes them).
    fn commit_sections(
        &mut self,
        song_id: SongId,
        sections: &[NewSection],
    ) -> Result<Vec<Section>, String> {
        let saved = self.library.replace_sections(song_id, sections).err_str()?;
        let stale_junctions: Vec<LoopId> = self
            .library
            .list_loops(song_id)
            .into_iter()
            .filter(|l| matches!(l.kind, LoopKind::Junction { .. }))
            .map(|l| l.id)
            .collect();
        self.library.delete_loops(&stale_junctions).err_str()?;
        self.recompute_loop_names(song_id)?;
        Ok(saved)
    }

    /// Save the analyzer's suggested sections as the song's real sections. A
    /// run that found no sections leaves any existing layout untouched (so a
    /// failed/empty pass never wipes hand-tuned structure).
    fn commit_analysis_sections(
        &mut self,
        song_id: SongId,
        suggestions: &[AnalysisSection],
    ) -> Result<Vec<Section>, String> {
        if suggestions.is_empty() {
            return Ok(self.library.list_sections(song_id));
        }
        let news: Vec<NewSection> = suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| NewSection {
                name: &s.label,
                start: s.start,
                end: s.end,
                position: i as i32,
                // analysis suggestions start unmarked; the user opts in later.
                click_guide: false,
            })
            .collect();
        let sections = self.commit_sections(song_id, &news)?;
        Ok(sections)
    }

    /// Sections to name loops against. Prefer the user's saved sections; when a
    /// song has none yet, fall back to the analysis *suggestions* — the same
    /// dashed spans the waveform's structure lane shows. Without this, looping a
    /// suggested-but-unsaved section names every loop `riff m:ss–m:ss`.
    /// Suggestions get synthetic ids/positions in their natural order, which is
    /// all `naming::loop_name` needs.
    fn naming_sections(&self, song_id: SongId) -> Result<Vec<Section>, String> {
        let saved = self.library.list_sections(song_id);
        if !saved.is_empty() {
            return Ok(saved);
        }
        let Some(analysis) = self.library.get_analysis(song_id) else {
            return Ok(saved); // empty — namer falls back to the timestamp form
        };
        Ok(analysis
            .sections
            .into_iter()
            .enumerate()
            .map(|(i, s)| Section {
                id: SectionId(i as i64),
                song_id,
                name: s.label,
                start: s.start,
                end: s.end,
                position: i as i32,
                click_guide: false,
            })
            .collect())
    }

    /// Effective dynamic name for a loop on this song, disambiguated against
    /// every *other* loop's name. `exclude` is the loop being (re)named.
    fn auto_name_loop(
        &self,
        song_id: SongId,
        start: f64,
        end: f64,
        exclude: Option<LoopId>,
    ) -> Result<String, String> {
        let sections = self.naming_sections(song_id)?;
        let existing: Vec<String> = self
            .library
            .list_loops(song_id)
            .into_iter()
            .filter(|l| Some(l.id) != exclude)
            .map(|l| l.name)
            .collect();
        Ok(practice::naming::loop_name(
            start, end, &sections, &existing,
        ))
    }

    /// Recompute the dynamic name of every non-overridden manual loop on the
    /// song (called when sections change). Overridden and junction loops are
    /// left untouched.
    fn recompute_loop_names(&mut self, song_id: SongId) -> Result<(), String> {
        let loops = self.library.list_loops(song_id);
        let sections = self.naming_sections(song_id)?;
        // Compute every rename against the original snapshot (whose names don't
        // change as we go), then apply them in one transaction.
        let mut renames = Vec::new();
        for l in &loops {
            if l.name_override.is_some() || !matches!(l.kind, LoopKind::Manual) {
                continue;
            }
            let existing: Vec<String> = loops
                .iter()
                .filter(|o| o.id != l.id)
                .map(|o| o.name.clone())
                .collect();
            let name = practice::naming::loop_name(l.start, l.end, &sections, &existing);
            if name != l.name {
                renames.push(LoopRename {
                    id: l.id,
                    name,
                    start: l.start,
                    end: l.end,
                });
            }
        }
        self.library.rename_loops(&renames).err_str()?;
        Ok(())
    }

    fn loop_create(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            start: f64,
            end: f64,
        }
        let p: P = from_params(p)?;
        let name = self.auto_name_loop(p.song_id, p.start, p.end, None)?;
        let l = self
            .library
            .insert_loop(
                p.song_id,
                NewLoop {
                    name: &name,
                    name_override: None,
                    start: p.start,
                    end: p.end,
                    kind: LoopKind::Manual,
                },
            )
            .err_str()?;
        serde_json::to_value(l).err_str()
    }

    fn loop_update(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            loop_id: LoopId,
            name: Option<String>,
            start: Option<f64>,
            end: Option<f64>,
        }
        let p: P = from_params(p)?;
        let old = self
            .library
            .loop_by_id(p.loop_id)
            .ok_or_else(|| format!("loop not found: {}", p.loop_id.0))?;
        let start = p.start.unwrap_or(old.start);
        let end = p.end.unwrap_or(old.end);

        // Decide the override after this update:
        // - explicit non-empty name -> pin it
        // - explicit empty name      -> clear (revert to dynamic)
        // - no name field            -> keep whatever was pinned
        let override_after: Option<String> = match p.name {
            Some(ref n) if !n.trim().is_empty() => Some(n.trim().to_string()),
            Some(_) => None,
            None => old.name_override.clone(),
        };

        let name = match &override_after {
            Some(n) => n.clone(),
            None => self.auto_name_loop(old.song_id, start, end, Some(p.loop_id))?,
        };

        let updated = self
            .library
            .update_loop(p.loop_id, &name, override_after.as_deref(), start, end)
            .err_str()?;
        serde_json::to_value(updated).err_str()
    }

    /// Snap each edge of a loop to the nearest section boundary, then recompute
    /// its dynamic name (a no-op on its name if it carries an override).
    fn loop_fit(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            loop_id: LoopId,
        }
        let p: P = from_params(p)?;
        let old = self
            .library
            .loop_by_id(p.loop_id)
            .ok_or_else(|| format!("loop not found: {}", p.loop_id.0))?;
        let sections = self.library.list_sections(old.song_id);
        // gather every section boundary, snap each edge to the nearest one
        let mut bounds: Vec<f64> = Vec::new();
        for s in &sections {
            bounds.push(s.start);
            bounds.push(s.end);
        }
        let snap = |t: f64| -> f64 {
            bounds
                .iter()
                .copied()
                .min_by(|a, b| (a - t).abs().partial_cmp(&(b - t).abs()).unwrap())
                .unwrap_or(t)
        };
        let (mut start, mut end) = if bounds.is_empty() {
            (old.start, old.end)
        } else {
            (snap(old.start), snap(old.end))
        };
        if end <= start {
            // degenerate snap (both edges to the same boundary) — leave as-was
            start = old.start;
            end = old.end;
        }
        let name = match &old.name_override {
            Some(n) => n.clone(),
            None => self.auto_name_loop(old.song_id, start, end, Some(p.loop_id))?,
        };
        let updated = self
            .library
            .update_loop(p.loop_id, &name, old.name_override.as_deref(), start, end)
            .err_str()?;
        serde_json::to_value(updated).err_str()
    }

    fn loop_delete(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            loop_id: LoopId,
        }
        let p: P = from_params(p)?;
        self.library
            .loop_by_id(p.loop_id)
            .ok_or_else(|| format!("loop not found: {}", p.loop_id.0))?;
        self.library.delete_loop(p.loop_id).err_str()?;
        Ok(Value::Null)
    }

    fn loop_list(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        serde_json::to_value(self.library.list_loops(p.song_id)).err_str()
    }

    // --- routines ---------------------------------------------------------

    fn routine_list(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        serde_json::to_value(self.library.list_routines(p.song_id)).err_str()
    }

    /// Upsert a routine (id 0 = new). Returns the stored routine with its id.
    fn routine_save(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            routine: Routine,
        }
        let p: P = from_params(p)?;
        let saved = self.library.save_routine(p.song_id, p.routine).err_str()?;
        serde_json::to_value(saved).err_str()
    }

    fn routine_delete(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            id: RoutineId,
        }
        let p: P = from_params(p)?;
        self.library.delete_routine(p.song_id, p.id).err_str()?;
        // Stop a run of the routine being deleted.
        if self
            .active_routine
            .as_ref()
            .is_some_and(|r| r.routine_id() == p.id)
        {
            self.active_routine = None;
        }
        Ok(Value::Null)
    }

    /// Launch a routine: load it, apply its first block, and start playback. The
    /// running routine then owns the transport until stopped — `tick` advances
    /// it on each loop wrap.
    fn routine_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            id: RoutineId,
            /// Block to start from (default 0) — lets a click jump into a block.
            #[serde(default)]
            block_index: usize,
        }
        let p: P = from_params(p)?;
        let routine = self
            .library
            .list_routines(p.song_id)
            .into_iter()
            .find(|r| r.id == p.id)
            .ok_or("routine not found")?;
        let runner = crate::routine::RoutineRunner::new_from(p.song_id, routine, p.block_index)
            .ok_or("routine has no blocks")?;
        let block = runner.current_block().clone();
        self.active_routine = Some(runner);
        self.apply_block(p.song_id, &block);
        self.audio.send(EngineCmd::Play);
        serde_json::to_value(self.active_routine.as_ref().unwrap().status()).err_str()
    }

    /// Stop advancing. The current block's loop / mix / rate / count-in stay in
    /// place — you keep practicing where it landed; only the auto-advance ends.
    fn routine_stop(&mut self) -> Result<Value, String> {
        self.active_routine = None;
        Ok(json!({ "running": false }))
    }

    /// Drive the loop region, rate, mix, and count-in for a block. The block is
    /// passed by value (cloned out of the runner) to keep the borrow clean.
    fn apply_block(&mut self, song_id: SongId, block: &Block) {
        let start = self.lead_in_start(song_id, block);
        self.audio.send(EngineCmd::SetLoopSecs {
            start,
            end: block.span.end,
        });
        self.audio.send(EngineCmd::SetRate(block.speed));
        self.apply_mix(block.mix);
        self.push_block_count_in(song_id, &block.count_in);
    }

    /// The block's loop start, pushed back by `lead_in_beats` and snapped to the
    /// analyzed beat grid (the persisted run-up). Falls back to BPM spacing, then
    /// to the bare span start when no analysis exists.
    fn lead_in_start(&self, song_id: SongId, block: &Block) -> f64 {
        if block.lead_in_beats == 0 {
            return block.span.start;
        }
        let Some(a) = self.library.get_analysis(song_id) else {
            return block.span.start;
        };
        if !a.beats.is_empty() {
            let i = a.beats.partition_point(|&b| b < block.span.start);
            let target = i.saturating_sub(block.lead_in_beats as usize);
            return a.beats[target.min(a.beats.len() - 1)];
        }
        match a.bpm {
            Some(bpm) if bpm > 0.0 => {
                (block.span.start - block.lead_in_beats as f64 * 60.0 / bpm).max(0.0)
            }
            _ => block.span.start,
        }
    }

    /// Push a block's count-in to the engine, rate-tracking via the analyzed BPM
    /// (mirrors `push_count_in`, but sourced from the block rather than the
    /// global setting). Forced off with no BPM or zero beats.
    fn push_block_count_in(&mut self, song_id: SongId, ci: &CountIn) {
        let bpm = self.library.get_analysis(song_id).and_then(|a| a.bpm);
        let (beats, beat_secs) = match bpm {
            Some(b) if b > 0.0 && ci.beats > 0 => (ci.beats, beat_secs_from_bpm(b)),
            _ => (0, 0.5),
        };
        self.audio.send(EngineCmd::SetCountIn {
            beats,
            beat_secs,
            every_loop: matches!(ci.loop_mode, CountInMode::Every),
        });
    }

    /// On an engine loop wrap, advance the active routine. Returns a `routine`
    /// event when the block changed (the new block has been applied), so `tick`
    /// can broadcast it. `None` when no routine runs or the block is held.
    fn advance_routine_on_wrap(&mut self) -> Option<Event> {
        let runner = self.active_routine.as_mut()?;
        if !runner.on_wrap() {
            return None;
        }
        let song_id = runner.song_id;
        let block = runner.current_block().clone();
        self.apply_block(song_id, &block);
        let status = self.active_routine.as_ref().unwrap().status();
        Some(Event {
            event: "routine".into(),
            data: serde_json::to_value(status).ok()?,
        })
    }

    // --- shared helpers ---------------------------------------------------

    fn song_row(&self, id: SongId) -> Result<Song, String> {
        self.library
            .song_by_id(id)
            .ok_or_else(|| format!("song not found: {}", id.0))
    }
}

/// Decode `src` to a canonical 48k stereo WAV in a fresh temp dir, returning
/// the dir (kept alive by the caller; auto-removes on drop) and the WAV path.
/// External tools (analysis, Demucs) read this instead of the original file, so
/// symphonia is the single decode authority — they never need ffmpeg, and video
/// containers (mp4/mov) work because only the decoded audio reaches them. The
/// fixed `audio.wav` stem keeps Demucs's file-stem-derived output dir stable.
fn canonical_wav_for_tools(src: &Path) -> Result<(tempfile::TempDir, PathBuf), String> {
    let dir = tempfile::Builder::new()
        .prefix("dredge-decode-")
        .tempdir()
        .map_err(|e| format!("cannot create decode temp dir: {e}"))?;
    let wav = dir.path().join("audio.wav");
    engine::decode::decode_to_wav(src, &wav).err_str()?;
    Ok((dir, wav))
}

#[cfg(test)]
mod export_dir_tests {
    use super::resolve_export_dir;

    #[test]
    fn expands_leading_tilde_to_home() {
        let home = dirs::home_dir().expect("home dir");
        assert_eq!(resolve_export_dir("~").unwrap(), home);
        assert_eq!(resolve_export_dir("~/Music").unwrap(), home.join("Music"));
        // trailing slash and surrounding whitespace are tolerated
        assert_eq!(
            resolve_export_dir("  ~/Music/  ").unwrap(),
            home.join("Music/")
        );
    }

    #[test]
    fn passes_absolute_paths_through() {
        assert_eq!(
            resolve_export_dir("/tmp/dredge-out").unwrap(),
            std::path::PathBuf::from("/tmp/dredge-out")
        );
    }

    #[test]
    fn rejects_relative_paths_so_they_never_resolve_against_cwd() {
        // This is the bug being guarded: a relative/untilded path must error
        // rather than be created relative to the daemon's working directory.
        assert!(resolve_export_dir("downloads").is_err());
        assert!(resolve_export_dir("./out").is_err());
        assert!(resolve_export_dir("").is_err());
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;
    use crate::control::MockEngine;
    use crate::stems::FakeSeparator;
    use practice::store::Store;

    #[test]
    fn rename_rejected_while_analysis_running() {
        let lib_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let audio = src.path().join("a.flac");
        std::fs::write(&audio, b"AUDIO").unwrap();

        let mut app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(MockEngine::default()),
            Arc::new(FakeSeparator),
        );
        app.set_library_root(lib_dir.path().to_path_buf());
        let song = app
            .library
            .create_song(&audio, "Title", Some("Band"), "hash", 1.0)
            .unwrap();

        // A stems/analysis job for this song captured the old path; renaming
        // would move the dir under it. The guard must refuse.
        app.analyzing.insert(song.id.0);
        let err = app
            .update_apply(json!({ "song_id": song.id, "title": "New", "artist": "X" }))
            .unwrap_err();
        assert!(err.contains("running"), "got: {err}");

        // Nothing moved: the bundle dir keeps its original slug.
        let dir = app.song_bundle_dir(song.id).unwrap();
        assert_eq!(
            dir.file_name().unwrap().to_str().unwrap(),
            "Title \u{2014} Band"
        );
    }

    #[test]
    fn reopen_only_when_folder_moves() {
        let lib_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let audio = src.path().join("a.flac");
        std::fs::write(&audio, b"AUDIO").unwrap();

        let mut app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(MockEngine::default()),
            Arc::new(FakeSeparator),
        );
        app.set_library_root(lib_dir.path().to_path_buf());
        let song = app
            .library
            .create_song(&audio, "Title", Some("Band"), "hash", 1.0)
            .unwrap();
        // Pretend it's the open song without paying a real decode.
        app.open_song = Some(OpenSong {
            song: song.clone(),
            stems: false,
        });

        // Same slug → no move → no reopen, but the open header still syncs.
        let (_, reopen) = app
            .update_apply(json!({ "song_id": song.id, "title": "Title", "artist": "Band" }))
            .unwrap();
        assert_eq!(reopen, None, "metadata-only edit must not reopen");

        // Different slug → folder moves → reopen the open song.
        let (_, reopen) = app
            .update_apply(json!({ "song_id": song.id, "title": "Renamed", "artist": "Band" }))
            .unwrap();
        assert_eq!(reopen, Some(song.id), "a folder move must reopen");
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;
    use crate::control::MockEngine;
    use crate::stems::FakeSeparator;
    use practice::store::Store;
    use std::sync::{Arc, Mutex};

    fn make_shared_mock() -> (Arc<Mutex<MockEngine>>, App) {
        let mock = Arc::new(Mutex::new(MockEngine::default()));
        let app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(mock.clone()),
            Arc::new(FakeSeparator),
        );
        (mock, app)
    }

    #[test]
    fn set_output_persists_and_forwards_id() {
        let (mock, mut app) = make_shared_mock();

        let resp = app.dispatch(Request {
            id: 1,
            cmd: "device.setOutput".into(),
            params: json!({ "id": "123" }),
        });
        assert!(resp.ok, "expected ok, got: {:?}", resp.error);

        // Setting persisted.
        let saved = app.store.get_setting("output_device").unwrap().unwrap();
        assert_eq!(saved, json!("123"));

        // Mock forwarded the call.
        let log = &mock.lock().unwrap().output_device_log;
        assert_eq!(log.last(), Some(&Some("123".to_string())));
    }

    #[test]
    fn set_input_persists_id() {
        let (_mock, mut app) = make_shared_mock();

        let resp = app.dispatch(Request {
            id: 1,
            cmd: "device.setInput".into(),
            params: json!({ "id": "7" }),
        });
        assert!(resp.ok, "expected ok, got: {:?}", resp.error);

        // Setting persisted; input does not forward to audio.
        let saved = app.store.get_setting("input_device").unwrap().unwrap();
        assert_eq!(saved, json!("7"));
    }

    #[test]
    fn set_output_null_clears_setting_and_forwards_none() {
        let (mock, mut app) = make_shared_mock();

        // Seed a value first so the null actually clears something meaningful.
        app.store
            .set_setting("output_device", &json!("old"))
            .unwrap();

        let resp = app.dispatch(Request {
            id: 2,
            cmd: "device.setOutput".into(),
            params: json!({ "id": null }),
        });
        assert!(resp.ok, "expected ok, got: {:?}", resp.error);

        let saved = app.store.get_setting("output_device").unwrap().unwrap();
        assert_eq!(saved, Value::Null);

        let log = &mock.lock().unwrap().output_device_log;
        assert_eq!(log.last(), Some(&None));
    }

    #[test]
    fn startup_applies_saved_output_device() {
        let store = Store::open_in_memory().unwrap();
        store.set_setting("output_device", &json!("456")).unwrap();

        let mock = Arc::new(Mutex::new(MockEngine::default()));
        let _app = App::new(store, Box::new(mock.clone()), Arc::new(FakeSeparator));

        let log = &mock.lock().unwrap().output_device_log;
        assert_eq!(
            log.last(),
            Some(&Some("456".to_string())),
            "App::new must apply the saved output_device"
        );
    }

    #[test]
    fn startup_skips_apply_when_no_saved_device() {
        let mock = Arc::new(Mutex::new(MockEngine::default()));
        let _app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(mock.clone()),
            Arc::new(FakeSeparator),
        );

        assert!(
            mock.lock().unwrap().output_device_log.is_empty(),
            "no saved device → set_output_device must not be called"
        );
    }
}

#[cfg(test)]
mod count_in_tests {
    use super::*;
    use crate::control::MockEngine;
    use crate::stems::FakeSeparator;
    use practice::store::Store;
    use std::sync::{Arc, Mutex};

    fn make_shared_mock() -> (Arc<Mutex<MockEngine>>, App) {
        let mock = Arc::new(Mutex::new(MockEngine::default()));
        let app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(mock.clone()),
            Arc::new(FakeSeparator),
        );
        (mock, app)
    }

    #[test]
    fn beat_secs_from_bpm_is_sixty_over_bpm() {
        assert!((beat_secs_from_bpm(120.0) - 0.5).abs() < 1e-9);
        assert!((beat_secs_from_bpm(60.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn countin_set_persists_and_forwards_off_without_analysis() {
        let (mock, mut app) = make_shared_mock();
        let resp = app.dispatch(Request {
            id: 1,
            cmd: "countin.set".into(),
            params: json!({ "enabled": true, "beats": 4, "loop_mode": "first" }),
        });
        assert!(resp.ok, "expected ok, got: {:?}", resp.error);

        // Persisted.
        let saved = app.store.get_setting("count_in").unwrap().unwrap();
        assert_eq!(saved["enabled"], json!(true));
        assert_eq!(saved["beats"], json!(4));
        assert_eq!(saved["loop_mode"], json!("first"));

        // No open song / no bpm → engine told beats: 0.
        let last = mock
            .lock()
            .unwrap()
            .sent
            .iter()
            .rev()
            .find_map(|c| match c {
                EngineCmd::SetCountIn { beats, .. } => Some(*beats),
                _ => None,
            });
        assert_eq!(last, Some(0), "no analysis → count-in forced off");
    }

    #[test]
    fn countin_set_disabled_persists_beats_but_forwards_off() {
        // Toggling off keeps the beat count in the setting (so it survives a
        // later toggle-on) but the engine is told beats: 0.
        let (mock, mut app) = make_shared_mock();
        app.dispatch(Request {
            id: 1,
            cmd: "countin.set".into(),
            params: json!({ "enabled": false, "beats": 4, "loop_mode": "first" }),
        });
        let saved = app.store.get_setting("count_in").unwrap().unwrap();
        assert_eq!(saved["enabled"], json!(false));
        assert_eq!(saved["beats"], json!(4), "beat count remembered while off");
        let last = mock
            .lock()
            .unwrap()
            .sent
            .iter()
            .rev()
            .find_map(|c| match c {
                EngineCmd::SetCountIn { beats, .. } => Some(*beats),
                _ => None,
            });
        assert_eq!(last, Some(0), "disabled → engine forced off");
    }

    #[test]
    fn countin_set_every_forwards_every_loop_true() {
        // The loop mode is forwarded independently of BPM (it is a config flag,
        // not gated like the beat count), so no open song is needed.
        let (mock, mut app) = make_shared_mock();
        app.dispatch(Request {
            id: 1,
            cmd: "countin.set".into(),
            params: json!({ "enabled": true, "beats": 4, "loop_mode": "every" }),
        });
        let last = mock
            .lock()
            .unwrap()
            .sent
            .iter()
            .rev()
            .find_map(|c| match c {
                EngineCmd::SetCountIn { every_loop, .. } => Some(*every_loop),
                _ => None,
            });
        assert_eq!(last, Some(true));
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    use crate::control::MockEngine;
    use crate::recording::FakeRecorder;
    use crate::stems::FakeSeparator;
    use engine::buffer::{CHANNELS, SAMPLE_RATE};
    use practice::store::Store;
    use std::sync::{Arc, Mutex};

    /// Like `make_shared_mock` but installs a recording backend and a tempdir
    /// library, then registers + opens a song so handlers have a bundle dir.
    fn make_shared_mock_with_recorder(
        rec: Box<dyn crate::recording::RecordingControl>,
    ) -> (Arc<Mutex<MockEngine>>, App, SongId, tempfile::TempDir) {
        let mock = Arc::new(Mutex::new(MockEngine::default()));
        let mut app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(mock.clone()),
            Arc::new(FakeSeparator),
        );
        app.set_recorder(rec);
        let lib_dir = tempfile::tempdir().unwrap();
        app.set_library_root(lib_dir.path().to_path_buf());

        // A real (silent) audio file in a source dir; create_song copies it in.
        let src = lib_dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let audio = src.join("a.flac");
        std::fs::write(&audio, b"AUDIO").unwrap();
        let song = app
            .library
            .create_song(&audio, "Title", Some("Band"), "hash", 2.0)
            .unwrap();
        // Mark it open without paying a real decode — handlers only need the
        // open-song header + bundle dir.
        app.open_song = Some(OpenSong {
            song: song.clone(),
            stems: false,
        });
        (mock, app, song.id, lib_dir)
    }

    #[test]
    fn record_start_stop_persists_a_take_and_pushes_a_layer() {
        let (mock, mut app, song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                started: None,
                stopped: false,
                input_delay: 0,
            }));

        let started = app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        assert!(started.ok, "start failed: {:?}", started.error);

        let finished = app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        assert!(finished.ok, "stop failed: {:?}", finished.error);

        let recs = app.library.recordings(song_id);
        assert_eq!(recs.len(), 1);
        let dir = app.song_bundle_dir(song_id).unwrap();
        assert!(dir.join(&recs[0].file).exists(), "WAV should be written");
        assert_eq!(mock.lock().unwrap().layers_len, 1, "one layer pushed");

        // The finished-take payload carries the take's waveform peaks so its
        // lane draws immediately.
        let peaks = &finished.data["peaks"];
        assert!(peaks.is_object(), "finished take carries peaks: {peaks:?}");
        assert!(
            !peaks["buckets"].as_array().unwrap().is_empty(),
            "peaks have buckets"
        );
    }

    /// `recording.list` (and thus `song.open`) ships each take's non-persisted
    /// waveform peaks alongside the persisted fields.
    #[test]
    fn recording_list_carries_peaks() {
        let (_mock, mut app, _song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                started: None,
                stopped: false,
                input_delay: 0,
            }));
        app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });

        let listed = app.dispatch(Request {
            id: 3,
            cmd: "recording.list".into(),
            params: json!(null),
        });
        assert!(listed.ok, "list failed: {:?}", listed.error);
        let arr = listed.data.as_array().expect("list is an array");
        assert_eq!(arr.len(), 1);
        let peaks = &arr[0]["peaks"];
        assert!(peaks.is_object(), "listed take carries peaks: {peaks:?}");
        assert_eq!(
            peaks["frames_per_bucket"],
            json!(engine::peaks::FRAMES_PER_BUCKET),
            "peaks use the standard bucket size"
        );
    }

    /// AS-6: with `latency_source` unset (auto), finalizing a take writes the
    /// RTL baseline = output delay + input delay back to `input_latency_frames`.
    #[test]
    fn finalize_writes_auto_rtl_baseline_from_pipewire_delays() {
        let (mock, mut app, _song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                input_delay: 120,
                ..Default::default()
            }));
        mock.lock().unwrap().output_delay = 80;

        app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        let finished = app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        assert!(finished.ok, "stop failed: {:?}", finished.error);

        assert_eq!(
            app.store.get_setting(INPUT_LATENCY_AUTO_KEY).unwrap(),
            Some(json!(200)),
            "auto RTL baseline = output_delay(80) + input_delay(120)"
        );
    }

    /// The auto baseline is recorded even when a loopback calibration is the
    /// active source, so the estimate stays current and the devices readout can
    /// show both. The active value still derives from the loopback calibration.
    #[test]
    fn finalize_refreshes_auto_baseline_even_under_loopback() {
        let (mock, mut app, _song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                input_delay: 120,
                ..Default::default()
            }));
        mock.lock().unwrap().output_delay = 80;
        app.store
            .set_setting(LATENCY_SOURCE_KEY, &json!("loopback"))
            .unwrap();
        app.store
            .set_setting(INPUT_LATENCY_LOOPBACK_KEY, &json!(512))
            .unwrap();

        app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        let finished = app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        assert!(finished.ok, "stop failed: {:?}", finished.error);

        assert_eq!(
            app.store.get_setting(INPUT_LATENCY_AUTO_KEY).unwrap(),
            Some(json!(200)),
            "auto baseline kept current even under loopback"
        );
        assert_eq!(
            app.store.get_setting(INPUT_LATENCY_LOOPBACK_KEY).unwrap(),
            Some(json!(512)),
            "loopback value untouched by finalize"
        );
        assert_eq!(
            app.input_latency_frames(),
            512,
            "active value still derives from the loopback calibration"
        );
    }

    /// `recording.latency` reports both stored measurements and the active
    /// source.
    #[test]
    fn recording_latency_reports_both_values_and_source() {
        let (_mock, mut app, _song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder::default()));

        // Fresh: neither measurement taken yet, default source.
        let fresh = app.dispatch(Request {
            id: 1,
            cmd: "recording.latency".into(),
            params: json!(null),
        });
        assert!(fresh.ok, "latency failed: {:?}", fresh.error);
        assert_eq!(fresh.data["auto_frames"], json!(null));
        assert_eq!(fresh.data["loopback_frames"], json!(null));
        assert_eq!(fresh.data["source"], json!("auto"));

        app.store
            .set_setting(INPUT_LATENCY_AUTO_KEY, &json!(200))
            .unwrap();
        app.store
            .set_setting(INPUT_LATENCY_LOOPBACK_KEY, &json!(512))
            .unwrap();
        app.store
            .set_setting(LATENCY_SOURCE_KEY, &json!("loopback"))
            .unwrap();
        let resp = app.dispatch(Request {
            id: 2,
            cmd: "recording.latency".into(),
            params: json!(null),
        });
        assert_eq!(resp.data["auto_frames"], json!(200));
        assert_eq!(resp.data["loopback_frames"], json!(512));
        assert_eq!(resp.data["source"], json!("loopback"));
    }

    /// Locks the anchor wiring used by `finalize_recording`: a song frame is
    /// mapped through graph time (playback clock) to the capture ring frame
    /// acquired at that instant (capture clock + ring total). Mirrors
    /// `stream_clock::ring_frame_maps_through_graph_time` at this layer so a
    /// change to the finalize expression that diverges from the clock math is
    /// caught here.
    #[test]
    fn anchor_math_maps_song_frame_to_ring_start() {
        use engine::stream_clock::{ring_frame_at_ns, ClockSnapshot};
        // Capture stream: at graph time 0 it was at tick 1000, and the ring had
        // written 1000 frames by then.
        let cap = ClockSnapshot {
            now_ns: 0,
            ticks: 1000,
            rate_hz: 48_000,
        };
        let ring_total = 1000;
        // Playback song clock: song frame 0 was output at graph time 0.
        let play = ClockSnapshot {
            now_ns: 0,
            ticks: 0,
            rate_hz: 48_000,
        };
        let anchor = 48_000; // 1.0s into the song
                             // Exactly the expression `finalize_recording` runs:
        let t = play.ns_at_frame(anchor);
        let ring_start = ring_frame_at_ns(&cap, ring_total, t);
        // 1.0s after the snapshot the ring advanced 48_000 frames from 1000.
        assert_eq!(ring_start, 1000 + 48_000);
    }

    #[test]
    fn set_mute_and_delete_round_trip_through_the_manifest() {
        let (mock, mut app, song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                started: None,
                stopped: false,
                input_delay: 0,
            }));
        app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        let finished = app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        let rec: Recording = serde_json::from_value(finished.data).unwrap();

        // Mute keeps the layer present (muted layers still count in the set).
        let muted = app.dispatch(Request {
            id: 3,
            cmd: "recording.setMute".into(),
            params: json!({ "id": rec.id, "muted": true }),
        });
        assert!(muted.ok, "setMute failed: {:?}", muted.error);
        assert!(app.library.recordings(song_id)[0].muted);
        assert_eq!(mock.lock().unwrap().layers_len, 1);

        // Delete removes the take and its WAV, then rebuilds to zero layers.
        let dir = app.song_bundle_dir(song_id).unwrap();
        let wav = dir.join(&rec.file);
        let deleted = app.dispatch(Request {
            id: 4,
            cmd: "recording.delete".into(),
            params: json!({ "id": rec.id }),
        });
        assert!(deleted.ok, "delete failed: {:?}", deleted.error);
        assert!(app.library.recordings(song_id).is_empty());
        assert!(!wav.exists(), "WAV should be removed");
        assert_eq!(mock.lock().unwrap().layers_len, 0);
    }

    #[test]
    fn set_nudge_converts_ms_to_frames() {
        let (_mock, mut app, song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                started: None,
                stopped: false,
                input_delay: 0,
            }));
        app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        let finished = app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        let rec: Recording = serde_json::from_value(finished.data).unwrap();

        let nudged = app.dispatch(Request {
            id: 3,
            cmd: "recording.setNudge".into(),
            params: json!({ "id": rec.id, "nudge_ms": 10.0 }),
        });
        assert!(nudged.ok, "setNudge failed: {:?}", nudged.error);
        // 10 ms at 48 kHz = 480 frames.
        assert_eq!(app.library.recordings(song_id)[0].nudge_frames, 480);
    }

    #[test]
    fn start_twice_without_stop_errors() {
        let (_mock, mut app, _song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                started: None,
                stopped: false,
                input_delay: 0,
            }));
        let first = app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        assert!(first.ok, "first start failed: {:?}", first.error);
        let second = app.dispatch(Request {
            id: 2,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        assert!(!second.ok, "second start should be rejected");
        assert!(second.error.unwrap().contains("already recording"));
    }

    /// 0.5 s of silence as a real, decodable 48 kHz stereo WAV — used as the
    /// source audio for a song we open through the real `song.open` path.
    fn write_silent_wav(path: &std::path::Path) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..(SAMPLE_RATE as usize / 2 * CHANNELS) {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn opening_a_different_song_clears_the_layer_cache() {
        // Song A: record a take so the cache holds A's RecordingId 1.
        let (_mock, mut app, song_a, lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS],
                started: None,
                stopped: false,
                input_delay: 0,
            }));
        app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        let finished = app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        let rec_a: Recording = serde_json::from_value(finished.data).unwrap();
        assert!(
            app.layer_cache.contains_key(&rec_a.id),
            "song A's take should be cached"
        );

        // Song B: a real decodable WAV opened through the real `song.open` path,
        // which is what fires the clear. Its own RecordingId space starts at 1,
        // colliding with A's id — the bug would serve A's audio for B.
        let wav = lib.path().join("b.wav");
        write_silent_wav(&wav);
        let song_b = app
            .library
            .create_song(&wav, "Other", Some("Artist"), "hash-b", 0.5)
            .unwrap();
        let opened = app.dispatch(Request {
            id: 3,
            cmd: "song.open".into(),
            params: json!({ "song_id": song_b.id }),
        });
        assert!(opened.ok, "song.open failed: {:?}", opened.error);

        assert_ne!(song_a, song_b.id, "the two songs must be distinct");
        assert!(
            !app.layer_cache.contains_key(&rec_a.id),
            "opening song B must evict song A's cached take (no cross-song bleed)"
        );
        // B has no takes, so the cache is empty after the switch.
        assert!(app.layer_cache.is_empty(), "cache holds only the open song");
    }

    /// Loopback calibration: with a canned ring holding an impulse `OFFSET`
    /// frames after the emit instant, `recording.calibrate` stores `OFFSET` as
    /// `input_latency_frames` and pins `latency_source = "loopback"`.
    ///
    /// The fake capture snapshot is `now_ns=0, ticks=0, ring_total=0`, and the
    /// mock's canned emit time (1 ns) maps to ring frame 0, so the canned buffer
    /// is read from its start and the onset index is exactly the latency.
    #[test]
    fn calibrate_stores_loopback_onset_as_latency() {
        const OFFSET: usize = 240; // 5 ms at 48 kHz
        let mut canned = vec![0.0f32; OFFSET * CHANNELS];
        canned.extend_from_slice(&[0.8, 0.8]); // the loud loopback click
        canned.extend(std::iter::repeat_n(0.0f32, 1000 * CHANNELS));

        let (mock, app, _song_id, _lib) = make_shared_mock_with_recorder(Box::new(FakeRecorder {
            canned,
            ..Default::default()
        }));
        mock.lock().unwrap().canned_emit_ns = 1; // nonzero; maps to f_emit = 0

        let app = Arc::new(Mutex::new(app));
        let resp = super::dispatch_shared(
            &app,
            Request {
                id: 1,
                cmd: "recording.calibrate".into(),
                params: json!({ "device_id": "0" }),
            },
        );
        assert!(resp.ok, "calibrate failed: {:?}", resp.error);
        assert_eq!(resp.data["latency_frames"], json!(OFFSET as i64));
        assert_eq!(resp.data["source"], json!("loopback"));

        // Envelope for the UI: 240 points, emit at index 0, the onset bucket
        // holds the click, and the window is ~150 ms. OFFSET 240 / 30 = bucket 8.
        assert_eq!(resp.data["emit_index"], json!(0));
        assert_eq!(resp.data["onset_index"], json!(8));
        assert_eq!(resp.data["window_ms"], json!(150.0));
        let env = resp.data["envelope"].as_array().unwrap();
        assert_eq!(env.len(), 240);
        assert!(
            (env[8].as_f64().unwrap() - 0.8).abs() < 1e-6,
            "onset bucket holds the loopback click"
        );
        assert_eq!(env[0].as_f64().unwrap(), 0.0, "emit bucket is silent");

        let g = app.lock().unwrap();
        assert_eq!(
            g.store.get_setting(INPUT_LATENCY_LOOPBACK_KEY).unwrap(),
            Some(json!(OFFSET as i64))
        );
        assert_eq!(
            g.store.get_setting(LATENCY_SOURCE_KEY).unwrap(),
            Some(json!("loopback"))
        );
    }

    /// `recording.calibrate.reset` returns to auto latency: source flips to
    /// `"auto"` so the auto baseline becomes active, while the calibrated value
    /// stays stored (inactive) for the devices readout.
    #[test]
    fn calibrate_reset_returns_to_auto() {
        let (_mock, mut app, _song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder::default()));
        app.store
            .set_setting(LATENCY_SOURCE_KEY, &json!("loopback"))
            .unwrap();
        app.store
            .set_setting(INPUT_LATENCY_LOOPBACK_KEY, &json!(512))
            .unwrap();
        app.store
            .set_setting(INPUT_LATENCY_AUTO_KEY, &json!(200))
            .unwrap();

        let resp = app.dispatch(Request {
            id: 1,
            cmd: "recording.calibrate.reset".into(),
            params: json!(null),
        });
        assert!(resp.ok, "reset failed: {:?}", resp.error);
        assert_eq!(
            app.store.get_setting(LATENCY_SOURCE_KEY).unwrap(),
            Some(json!("auto"))
        );
        assert_eq!(
            app.store.get_setting(INPUT_LATENCY_LOOPBACK_KEY).unwrap(),
            Some(json!(512)),
            "the calibrated value persists, just inactive"
        );
        assert_eq!(
            app.input_latency_frames(),
            200,
            "active value now derives from the auto baseline"
        );
    }

    #[test]
    fn stop_without_start_errors() {
        let (_mock, mut app, _song_id, _lib) =
            make_shared_mock_with_recorder(Box::new(FakeRecorder {
                canned: vec![],
                started: None,
                stopped: false,
                input_delay: 0,
            }));
        let resp = app.dispatch(Request {
            id: 1,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("not recording"));
    }
}

#[cfg(test)]
mod mix_tests {
    use super::*;
    use crate::control::MockEngine;
    use crate::stems::FakeSeparator;
    use practice::store::Store;
    use std::sync::{Arc, Mutex};

    fn make_shared_mock() -> (Arc<Mutex<MockEngine>>, App) {
        let mock = Arc::new(Mutex::new(MockEngine::default()));
        let app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(mock.clone()),
            Arc::new(FakeSeparator),
        );
        (mock, app)
    }

    fn req(cmd: &str, params: Value) -> Request {
        Request {
            id: 1,
            cmd: cmd.into(),
            params,
        }
    }

    #[test]
    fn mix_get_starts_at_default() {
        let (_mock, mut app) = make_shared_mock();
        let resp = app.dispatch(req("mix.get", json!({})));
        assert!(resp.ok, "got: {:?}", resp.error);
        let mix: Mix = serde_json::from_value(resp.data).unwrap();
        assert_eq!(mix, Mix::default());
    }

    #[test]
    fn bass_focus_updates_current_mix_and_forwards() {
        let (mock, mut app) = make_shared_mock();
        let resp = app.dispatch(req("bass_focus", json!({ "on": true })));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert!(app.current_mix().bass_focus);
        assert!(mock
            .lock()
            .unwrap()
            .sent
            .iter()
            .any(|c| matches!(c, EngineCmd::BassFocus(true))));
    }

    #[test]
    fn stems_gains_updates_current_mix() {
        let (_mock, mut app) = make_shared_mock();
        // Pretend a stems-loaded song is open (no decode paid).
        app.open_song = Some(OpenSong {
            song: Song {
                id: SongId(1),
                title: "T".into(),
                artist: None,
                path: "p".into(),
                file_hash: "h".into(),
                duration_secs: 1.0,
            },
            stems: true,
        });
        let gains = [0.0_f32, 1.0, 0.5, 0.25, 0.75, 0.1];
        let resp = app.dispatch(req("stems.gains", json!({ "gains": gains })));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert_eq!(app.current_mix().stems, gains);
    }

    #[test]
    fn mix_set_applies_bass_focus_and_stem_gains_with_stems() {
        let (mock, mut app) = make_shared_mock();
        app.open_song = Some(OpenSong {
            song: Song {
                id: SongId(1),
                title: "T".into(),
                artist: None,
                path: "p".into(),
                file_hash: "h".into(),
                duration_secs: 1.0,
            },
            stems: true,
        });
        let mix = Mix {
            bass_focus: true,
            stems: [0.0, 1.0, 0.5, 0.25, 0.75, 0.1],
        };
        let resp = app.dispatch(req("mix.set", serde_json::to_value(mix).unwrap()));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert_eq!(app.current_mix(), mix);

        let sent = &mock.lock().unwrap().sent;
        assert!(sent.iter().any(|c| matches!(c, EngineCmd::BassFocus(true))));
        let gain_cmds: Vec<_> = sent
            .iter()
            .filter_map(|c| match c {
                EngineCmd::SetStemGain { idx, gain } => Some((*idx, *gain)),
                _ => None,
            })
            .collect();
        assert_eq!(
            gain_cmds,
            vec![(0, 0.0), (1, 1.0), (2, 0.5), (3, 0.25), (4, 0.75), (5, 0.1)]
        );
    }

    #[test]
    fn mix_set_skips_stem_gains_without_stems() {
        let (mock, mut app) = make_shared_mock();
        // No open song → no stems → stem gains must not be sent, bass-focus still is.
        let mix = Mix {
            bass_focus: true,
            stems: [0.2, 0.3, 0.4, 0.5, 0.6, 0.7],
        };
        let resp = app.dispatch(req("mix.set", serde_json::to_value(mix).unwrap()));
        assert!(resp.ok, "got: {:?}", resp.error);
        let sent = &mock.lock().unwrap().sent;
        assert!(sent.iter().any(|c| matches!(c, EngineCmd::BassFocus(true))));
        assert!(
            !sent
                .iter()
                .any(|c| matches!(c, EngineCmd::SetStemGain { .. })),
            "stem gains must be skipped when no stems are loaded"
        );
    }
}

#[cfg(test)]
mod routine_tests {
    use super::*;
    use crate::control::MockEngine;
    use crate::stems::FakeSeparator;
    use practice::model::{Block, CountIn, Span};
    use practice::store::Store;

    fn app_with_song() -> (App, SongId) {
        let lib_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let audio = src.path().join("a.flac");
        std::fs::write(&audio, b"AUDIO").unwrap();
        // Keep the temp dirs alive for the test by leaking them — fine in tests.
        std::mem::forget(src);
        let mut app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(MockEngine::default()),
            Arc::new(FakeSeparator),
        );
        app.set_library_root(lib_dir.path().to_path_buf());
        std::mem::forget(lib_dir);
        let song = app
            .library
            .create_song(&audio, "Title", Some("Band"), "hash", 1.0)
            .unwrap();
        (app, song.id)
    }

    fn sample_routine() -> Routine {
        Routine {
            id: RoutineId(0),
            name: "verse drill".into(),
            blocks: vec![
                Block {
                    span: Span {
                        start: 0.0,
                        end: 8.0,
                    },
                    mix: Mix {
                        bass_focus: false,
                        stems: [0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
                    },
                    speed: 1.0,
                    passes: 1,
                    lead_in_beats: 0,
                    count_in: CountIn::default(),
                    name: Some("bass".into()),
                },
                Block {
                    span: Span {
                        start: 0.0,
                        end: 8.0,
                    },
                    mix: Mix::default(),
                    speed: 0.85,
                    passes: 2,
                    lead_in_beats: 4,
                    count_in: CountIn::default(),
                    name: None,
                },
            ],
        }
    }

    fn req(cmd: &str, params: Value) -> Request {
        Request {
            id: 1,
            cmd: cmd.into(),
            params,
        }
    }

    #[test]
    fn save_list_delete_round_trip() {
        let (mut app, song_id) = app_with_song();

        let save = app.dispatch(req(
            "routine.save",
            json!({ "song_id": song_id, "routine": sample_routine() }),
        ));
        assert!(save.ok, "got: {:?}", save.error);
        let saved: Routine = serde_json::from_value(save.data).unwrap();
        assert_ne!(saved.id, RoutineId(0), "a fresh id is minted");
        assert_eq!(saved.blocks.len(), 2);

        let list = app.dispatch(req("routine.list", json!({ "song_id": song_id })));
        let routines: Vec<Routine> = serde_json::from_value(list.data).unwrap();
        assert_eq!(routines.len(), 1);
        assert_eq!(routines[0].name, "verse drill");

        let del = app.dispatch(req(
            "routine.delete",
            json!({ "song_id": song_id, "id": saved.id }),
        ));
        assert!(del.ok, "got: {:?}", del.error);
        let list2 = app.dispatch(req("routine.list", json!({ "song_id": song_id })));
        let routines2: Vec<Routine> = serde_json::from_value(list2.data).unwrap();
        assert!(routines2.is_empty());
    }

    #[test]
    fn save_with_id_updates_in_place() {
        let (mut app, song_id) = app_with_song();
        let save = app.dispatch(req(
            "routine.save",
            json!({ "song_id": song_id, "routine": sample_routine() }),
        ));
        let mut saved: Routine = serde_json::from_value(save.data).unwrap();
        saved.name = "renamed".into();
        saved.blocks.truncate(1);
        let again = app.dispatch(req(
            "routine.save",
            json!({ "song_id": song_id, "routine": saved }),
        ));
        assert!(again.ok, "got: {:?}", again.error);
        let list = app.dispatch(req("routine.list", json!({ "song_id": song_id })));
        let routines: Vec<Routine> = serde_json::from_value(list.data).unwrap();
        assert_eq!(routines.len(), 1, "upsert, not append");
        assert_eq!(routines[0].name, "renamed");
        assert_eq!(routines[0].blocks.len(), 1);
    }

    fn app_with_song_shared() -> (Arc<Mutex<MockEngine>>, App, SongId) {
        let lib_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let audio = src.path().join("a.flac");
        std::fs::write(&audio, b"AUDIO").unwrap();
        std::mem::forget(src);
        let mock = Arc::new(Mutex::new(MockEngine::default()));
        let mut app = App::new(
            Store::open_in_memory().unwrap(),
            Box::new(mock.clone()),
            Arc::new(FakeSeparator),
        );
        app.set_library_root(lib_dir.path().to_path_buf());
        std::mem::forget(lib_dir);
        let song = app
            .library
            .create_song(&audio, "Title", Some("Band"), "hash", 1.0)
            .unwrap();
        (mock, app, song.id)
    }

    fn save_sample(app: &mut App, song_id: SongId) -> Routine {
        let save = app.dispatch(req(
            "routine.save",
            json!({ "song_id": song_id, "routine": sample_routine() }),
        ));
        serde_json::from_value(save.data).unwrap()
    }

    #[test]
    fn start_applies_first_block_and_plays() {
        let (mock, mut app, song_id) = app_with_song_shared();
        // Open the song with stems so the mix applies stem gains.
        app.open_song = Some(OpenSong {
            song: app.library.song_by_id(song_id).unwrap(),
            stems: true,
        });
        let saved = save_sample(&mut app, song_id);

        let start = app.dispatch(req(
            "routine.start",
            json!({ "song_id": song_id, "id": saved.id }),
        ));
        assert!(start.ok, "got: {:?}", start.error);
        assert_eq!(start.data["block_index"], json!(0));

        let sent = &mock.lock().unwrap().sent;
        assert!(sent
            .iter()
            .any(|c| matches!(c, EngineCmd::SetLoopSecs { .. })));
        assert!(sent.iter().any(|c| matches!(c, EngineCmd::SetRate(_))));
        assert!(sent
            .iter()
            .any(|c| matches!(c, EngineCmd::SetCountIn { .. })));
        assert!(sent.iter().any(|c| matches!(c, EngineCmd::Play)));
        // Block 0 isolates bass (stem idx 2 at unity, others muted).
        let gains: Vec<_> = sent
            .iter()
            .filter_map(|c| match c {
                EngineCmd::SetStemGain { idx, gain } => Some((*idx, *gain)),
                _ => None,
            })
            .collect();
        assert!(gains.contains(&(2, 1.0)), "bass up: {gains:?}");
        assert!(gains.contains(&(0, 0.0)), "vocals down: {gains:?}");
    }

    #[test]
    fn loop_wrap_advances_to_next_block() {
        let (mock, mut app, song_id) = app_with_song_shared();
        let saved = save_sample(&mut app, song_id);
        app.dispatch(req(
            "routine.start",
            json!({ "song_id": song_id, "id": saved.id }),
        ));

        mock.lock().unwrap().sent.clear();
        mock.lock()
            .unwrap()
            .queued_events
            .push_back(EngineEvent::LoopWrapped);
        let evs = app.tick();

        let routine_ev = evs
            .iter()
            .find(|e| e.event == "routine")
            .expect("a routine event on block change");
        assert_eq!(routine_ev.data["block_index"], json!(1));
        // Block 1 plays at 0.85×.
        let sent = &mock.lock().unwrap().sent;
        assert!(
            sent.iter()
                .any(|c| matches!(c, EngineCmd::SetRate(r) if (*r - 0.85).abs() < 1e-9)),
            "block 1 rate applied"
        );
    }

    #[test]
    fn start_from_block_index_jumps_into_that_block() {
        let (mock, mut app, song_id) = app_with_song_shared();
        let saved = save_sample(&mut app, song_id);
        let start = app.dispatch(req(
            "routine.start",
            json!({ "song_id": song_id, "id": saved.id, "block_index": 1 }),
        ));
        assert!(start.ok, "got: {:?}", start.error);
        assert_eq!(start.data["block_index"], json!(1));
        // Block 1 runs at 0.85×.
        let sent = &mock.lock().unwrap().sent;
        assert!(sent
            .iter()
            .any(|c| matches!(c, EngineCmd::SetRate(r) if (*r - 0.85).abs() < 1e-9)));
    }

    #[test]
    fn stop_halts_advancement() {
        let (mock, mut app, song_id) = app_with_song_shared();
        let saved = save_sample(&mut app, song_id);
        app.dispatch(req(
            "routine.start",
            json!({ "song_id": song_id, "id": saved.id }),
        ));
        let stop = app.dispatch(req("routine.stop", json!({})));
        assert_eq!(stop.data["running"], json!(false));

        mock.lock()
            .unwrap()
            .queued_events
            .push_back(EngineEvent::LoopWrapped);
        let evs = app.tick();
        assert!(
            evs.iter().all(|e| e.event != "routine"),
            "no advance after stop"
        );
    }
}
