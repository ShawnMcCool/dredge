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
    Analysis, AnalysisSection, LoopId, LoopKind, ProfileRun, Section, SectionId, Song, SongId,
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
}

struct OpenSong {
    song: Song,
    /// True when the engine got a 4-stem StemSet for this song.
    stems: bool,
}

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
            "section.click.set" => self.section_click_set(p),
            "sectionclick.set" => self.section_click_enable(p),
            "loop.create" => self.loop_create(p),
            "loop.update" => self.loop_update(p),
            "loop.delete" => self.loop_delete(p),
            "loop.fit" => self.loop_fit(p),
            "loop.list" => self.loop_list(p),
            "play" => self.send_ok(EngineCmd::Play),
            "pause" => self.send_ok(EngineCmd::Pause),
            "seek" => self.seek(p),
            "rate" => self.rate(p),
            "volume" => self.volume(p),
            "loop.set" => self.loop_set(p),
            "loop.clear" => self.send_ok(EngineCmd::ClearLoop),
            "bass_focus" => self.bass_focus(p),
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
        self.send_ok(EngineCmd::BassFocus(p.on))
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
        }
        let p: P = from_params(p)?;
        let song = self.song_row(p.song_id)?;
        let cache = self
            .stems_cache_dir(p.song_id)
            .ok_or("song not in library")?;
        if Self::stems_cached(&cache) {
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
            gains: [f32; 4],
        }
        let p: P = from_params(p)?;
        let open = self.open_song.as_ref().ok_or("no song open")?;
        if !open.stems {
            return Err("no stems loaded for the open song".into());
        }
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
        let out = json!({
            "song": song,
            "sections": sections,
            "loops": self.library.list_loops(song_id),
            "peaks": decoded.peaks,
            "stems": decoded.stems,
            "analysis": self.library.get_analysis(song_id),
            "orphan_notes": orphan_notes,
        });
        self.open_song = Some(OpenSong {
            song,
            stems: decoded.stems,
        });
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
