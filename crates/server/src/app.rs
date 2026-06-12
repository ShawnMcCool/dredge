use crate::capture_control::CaptureControl;
use crate::control::AudioControl;
use crate::protocol::{Event, Request, Response};
use crate::stems::{StemSeparator, STEM_NAMES};
use engine::pipeline::{EngineCmd, EngineEvent};
use practice::model::{
    LoopId, LoopKind, LoopRegion, Plan, PlanId, PlanStep, Rating, Song, SongId, TempoCurve,
};
use practice::runner::{PlanRunner, RepMode, RepSpec};
use practice::schedule::Resurfacing;
use practice::store::{NewLoop, NewRep, NewSection, NewSong, Store};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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

const DATE_FMT: &[time::format_description::FormatItem<'static>] =
    time::macros::format_description!("[year]-[month]-[day]");

/// UTC is fine for a practice ladder.
fn today_utc() -> time::Date {
    time::OffsetDateTime::now_utc().date()
}

pub struct App {
    store: Store,
    audio: Box<dyn AudioControl>,
    capture: Box<dyn CaptureControl>,
    separator: Arc<dyn StemSeparator>,
    captures_dir: PathBuf,
    stems_dir: PathBuf,
    open_song: Option<OpenSong>,
    active_plan: Option<ActivePlan>,
    /// Unsaved region of an ephemeral quick session (`LoopId(0)` sentinel);
    /// while set, `tick()` skips rep journaling — `practice.quick_rate` is
    /// the single persistence point.
    ephemeral: Option<LoopRegion>,
    last_position: Option<(f64, f64, bool)>, // secs, rate, playing
    /// Background-job events (stem separation); drained by `tick()`.
    job_tx: mpsc::Sender<Event>,
    job_rx: mpsc::Receiver<Event>,
    /// Song ids with a separation thread in flight.
    separating: Arc<Mutex<HashSet<i64>>>,
}

struct OpenSong {
    song: Song,
    /// True when the engine got a 4-stem StemSet for this song.
    stems: bool,
}

struct ActivePlan {
    plan_id: PlanId,
    runner: PlanRunner,
    loops: HashMap<LoopId, LoopRegion>,
}

impl App {
    pub fn new(
        store: Store,
        audio: Box<dyn AudioControl>,
        capture: Box<dyn CaptureControl>,
        separator: Arc<dyn StemSeparator>,
    ) -> Self {
        let (job_tx, job_rx) = mpsc::channel();
        Self {
            store,
            audio,
            capture,
            separator,
            captures_dir: default_captures_dir(),
            stems_dir: default_stems_dir(),
            open_song: None,
            active_plan: None,
            ephemeral: None,
            last_position: None,
            job_tx,
            job_rx,
            separating: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Override where grabbed captures are written (tests use a tempdir).
    pub fn set_captures_dir(&mut self, dir: PathBuf) {
        self.captures_dir = dir;
    }

    /// Override the stems cache root (tests use a tempdir).
    pub fn set_stems_dir(&mut self, dir: PathBuf) {
        self.stems_dir = dir;
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
            "song.list" => serde_json::to_value(self.store.list_songs().err_str()?).err_str(),
            "song.open" => self.song_open(p),
            "section.replace" => self.section_replace(p),
            "loop.create" => self.loop_create(p),
            "loop.update" => self.loop_update(p),
            "loop.delete" => self.loop_delete(p),
            "loop.list" => self.loop_list(p),
            "junctions.derive" => self.junctions_derive(p),
            "plan.save" => self.plan_save(p),
            "plan.list" => self.plan_list(p),
            "play" => self.send_ok(EngineCmd::Play),
            "pause" => self.send_ok(EngineCmd::Pause),
            "seek" => self.seek(p),
            "rate" => self.rate(p),
            "loop.set" => self.loop_set(p),
            "loop.clear" => self.send_ok(EngineCmd::ClearLoop),
            "bass_focus" => self.bass_focus(p),
            "mute" => self.mute(p),
            "pitch" => self.pitch(p),
            "status" => self.status(),
            "plan.start" => self.plan_start(p),
            "plan.stop" => self.plan_stop(),
            "plan.skip_step" => self.plan_skip_step(),
            "practice.quick" => self.quick_start(p),
            "practice.quick_rate" => self.quick_rate(p),
            "practice.quick_discard" => self.quick_discard(),
            "rep.rate" => self.rep_rate(p),
            "due.list" => self.due_list(),
            "retention" => self.retention(p),
            "capture.nodes" => serde_json::to_value(self.capture.list_nodes()?).err_str(),
            "capture.start" => self.capture_start(p),
            "capture.stop" => {
                self.capture.stop();
                Ok(Value::Null)
            }
            "capture.status" => self.capture_status(),
            "capture.grab" => self.capture_grab(p),
            "stems.separate" => self.stems_separate(p),
            "stems.status" => self.stems_status(p),
            "stems.gains" => self.stems_gains(p),
            _ => Err(format!("unknown command: {cmd}")),
        }
    }

    // --- ratings / scheduling ---------------------------------------------

    fn rep_rate(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            loop_id: LoopId,
            rating: Rating,
            #[serde(default)]
            is_retest: bool,
        }
        let p: P = from_params(p)?;
        let rate = self.last_position.map(|(_, r, _)| r).unwrap_or(1.0);
        self.store
            .record_rep(NewRep {
                loop_id: p.loop_id,
                plan_id: self.active_plan.as_ref().map(|a| a.plan_id),
                mode: "rated".into(),
                rate,
                rating: Some(p.rating),
                is_retest: p.is_retest,
            })
            .err_str()?;
        let next = self.reschedule(p.loop_id, p.rating)?;
        Ok(json!({
            "interval_idx": next.interval_idx,
            "due_on": next.due_on.format(DATE_FMT).err_str()?,
        }))
    }

    /// Advance the resurfacing ladder after a rated practice — shared by
    /// `rep.rate` and `practice.quick_rate`.
    fn reschedule(&mut self, loop_id: LoopId, rating: Rating) -> Result<Resurfacing, String> {
        let prev = self
            .store
            .all_resurfacing()
            .err_str()?
            .into_iter()
            .find(|r| r.loop_id == loop_id);
        let next = practice::schedule::next_state(prev, loop_id, rating, today_utc());
        self.store.upsert_resurfacing(next).err_str()?;
        Ok(next)
    }

    fn due_list(&mut self) -> Result<Value, String> {
        let items = self.store.all_resurfacing().err_str()?;
        let mut out = Vec::new();
        for id in practice::schedule::due(&items, today_utc()) {
            if let Some(l) = self.store.loop_by_id(id).err_str()? {
                out.push(json!({"loop_id": l.id, "name": l.name, "song_id": l.song_id}));
            }
        }
        Ok(Value::Array(out))
    }

    fn retention(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        let rows: Vec<Value> = self
            .store
            .retention(p.song_id)
            .err_str()?
            .into_iter()
            .map(|(loop_id, rating, at)| json!({"loop_id": loop_id, "rating": rating, "at": at}))
            .collect();
        Ok(Value::Array(rows))
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
        let scale =
            2f64.powf((p.semitones + p.cents / 100.0) / 12.0) * if p.octave_up { 2.0 } else { 1.0 };
        self.send_ok(EngineCmd::SetPitchScale(scale))
    }

    fn status(&self) -> Result<Value, String> {
        let (secs, rate, playing) = self.last_position.unwrap_or((0.0, 1.0, false));
        let plan = self.active_plan.as_ref().map(|ap| {
            let cur = ap.runner.current();
            json!({
                "plan_id": ap.plan_id,
                "step_idx": cur.map(|s| s.step_idx),
                "rep_idx": cur.map(|s| s.rep_idx),
                "mode": cur.map(|s| s.mode),
                "loop_id": cur.map(|s| s.loop_id),
            })
        });
        Ok(json!({
            "position_secs": secs,
            "rate": rate,
            "playing": playing,
            "song_id": self.open_song.as_ref().map(|o| o.song.id),
            "plan": plan,
        }))
    }

    // --- plan execution ----------------------------------------------------

    fn plan_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            plan_id: PlanId,
        }
        let p: P = from_params(p)?;
        let plan = self.plan_row(p.plan_id)?;
        let open_id = self
            .open_song
            .as_ref()
            .map(|o| o.song.id)
            .ok_or("no song open")?;
        if open_id != plan.song_id {
            return Err("plan belongs to a different song than the open one".into());
        }
        let loops: HashMap<LoopId, LoopRegion> = self
            .store
            .list_loops(plan.song_id)
            .err_str()?
            .into_iter()
            .map(|l| (l.id, l))
            .collect();
        let runner = PlanRunner::new(plan);
        let spec = runner.current().ok_or("plan has no reps")?;
        self.active_plan = Some(ActivePlan {
            plan_id: p.plan_id,
            runner,
            loops,
        });
        self.ephemeral = None;
        self.apply_rep(spec)?;
        serde_json::to_value(spec).err_str()
    }

    fn plan_row(&self, id: PlanId) -> Result<Plan, String> {
        for song in self.store.list_songs().err_str()? {
            if let Some(plan) = self
                .store
                .list_plans(song.id)
                .err_str()?
                .into_iter()
                .find(|p| p.id == id)
            {
                return Ok(plan);
            }
        }
        Err(format!("plan not found: {}", id.0))
    }

    fn plan_stop(&mut self) -> Result<Value, String> {
        self.active_plan = None;
        self.ephemeral = None;
        self.send_ok(EngineCmd::Pause)
    }

    fn plan_skip_step(&mut self) -> Result<Value, String> {
        let next = {
            let ap = self.active_plan.as_mut().ok_or("no active plan")?;
            ap.runner.skip_step();
            ap.runner.current()
        };
        match next {
            Some(spec) => {
                self.apply_rep(spec)?;
                serde_json::to_value(spec).err_str()
            }
            None => {
                self.active_plan = None;
                self.send_ok(EngineCmd::Pause)
            }
        }
    }

    fn apply_rep(&mut self, spec: RepSpec) -> Result<(), String> {
        let l = self
            .active_plan
            .as_ref()
            .ok_or("no active plan")?
            .loops
            .get(&spec.loop_id)
            .ok_or_else(|| format!("loop not found: {}", spec.loop_id.0))?;
        let (start, end) = (l.start, l.end);
        self.audio.send(EngineCmd::SetLoopSecs { start, end });
        self.audio.send(EngineCmd::SetRate(spec.rate));
        self.audio
            .send(EngineCmd::Mute(spec.mode == RepMode::RecallSilent));
        self.audio.send(EngineCmd::Play);
        Ok(())
    }

    // --- ephemeral practice (select → `p` → play) ---------------------------

    /// Instant micro-session on a raw span: listen ×2 → 6 play reps on the
    /// oscillate curve. Nothing persists unless the user rates it.
    fn quick_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            start: f64,
            end: f64,
        }
        let p: P = from_params(p)?;
        let song = &self.open_song.as_ref().ok_or("no song open")?.song;
        let end = p.end.min(song.duration_secs);
        if p.start < 0.0 || p.start >= end {
            return Err(format!("invalid span: {}–{}", p.start, p.end));
        }
        let region = LoopRegion {
            id: LoopId(0), // unsaved sentinel — never reaches the DB
            song_id: song.id,
            name: format!("riff {}–{}", fmt_ts(p.start), fmt_ts(end)),
            start: p.start,
            end,
            kind: LoopKind::Manual,
        };
        let runner = PlanRunner::new(Plan {
            id: PlanId(0),
            song_id: song.id,
            name: region.name.clone(),
            steps: vec![
                PlanStep::ListenFirst {
                    loop_id: LoopId(0),
                    reps: 2,
                },
                PlanStep::PlayReps {
                    loop_id: LoopId(0),
                    reps: 6,
                    curve: TempoCurve::Oscillate {
                        low: 0.7,
                        high: 1.0,
                        period: 3,
                    },
                },
            ],
        });
        let spec = runner.current().ok_or("plan has no reps")?;
        // a quick session replaces whatever plan was running
        self.active_plan = Some(ActivePlan {
            plan_id: PlanId(0),
            runner,
            loops: HashMap::from([(LoopId(0), region.clone())]),
        });
        self.ephemeral = Some(region);
        self.apply_rep(spec)?;
        serde_json::to_value(spec).err_str()
    }

    /// The single persistence point: auto-named loop saved, the rated rep
    /// recorded, resurfacing scheduled. Mid-session rating just ends it.
    fn quick_rate(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            rating: Rating,
        }
        let p: P = from_params(p)?;
        let region = self.ephemeral.take().ok_or("no quick session")?;
        if self.active_plan.is_some() {
            self.active_plan = None;
            self.audio.send(EngineCmd::Pause);
        }
        let saved = self
            .store
            .insert_loop(
                region.song_id,
                NewLoop {
                    name: &region.name,
                    start: region.start,
                    end: region.end,
                    kind: region.kind,
                },
            )
            .err_str()?;
        let rate = self.last_position.map(|(_, r, _)| r).unwrap_or(1.0);
        self.store
            .record_rep(NewRep {
                loop_id: saved.id,
                plan_id: None,
                mode: "play".into(),
                rate,
                rating: Some(p.rating),
                is_retest: false,
            })
            .err_str()?;
        let next = self.reschedule(saved.id, p.rating)?;
        self.write_sidecar_for(saved.song_id);
        Ok(json!({
            "loop": saved,
            "interval_idx": next.interval_idx,
            "due_on": next.due_on.format(DATE_FMT).err_str()?,
        }))
    }

    /// Discard leaves no trace; always ok, even without a session.
    fn quick_discard(&mut self) -> Result<Value, String> {
        if self.ephemeral.take().is_some() && self.active_plan.is_some() {
            self.active_plan = None;
            self.audio.send(EngineCmd::Pause);
        }
        Ok(Value::Null)
    }

    /// Drain engine events, drive the plan runner (loop-wrap = rep done) and
    /// return events for broadcast. Call ~every 50 ms.
    pub fn tick(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        // background-job reports (stem separation done/failed)
        while let Ok(ev) = self.job_rx.try_recv() {
            events.push(ev);
        }
        let mut last_pos = None;
        for ev in self.audio.poll_events() {
            match ev {
                EngineEvent::Position {
                    secs,
                    rate,
                    playing,
                } => last_pos = Some((secs, rate, playing)),
                EngineEvent::LoopWrapped => {
                    events.push(Event {
                        event: "loop_wrapped".into(),
                        data: Value::Null,
                    });
                    self.on_loop_wrapped(&mut events);
                }
                EngineEvent::Finished => events.push(Event {
                    event: "song_finished".into(),
                    data: Value::Null,
                }),
            }
        }
        // only the final Position per tick is broadcast (throttling)
        if let Some((secs, rate, playing)) = last_pos {
            self.last_position = Some((secs, rate, playing));
            events.push(Event {
                event: "position".into(),
                data: json!({"secs": secs, "rate": rate, "playing": playing}),
            });
        }
        events
    }

    fn on_loop_wrapped(&mut self, events: &mut Vec<Event>) {
        let Some(ap) = self.active_plan.as_mut() else {
            return;
        };
        let Some(done) = ap.runner.current() else {
            return;
        };
        let plan_id = ap.plan_id;
        ap.runner.advance();
        let next = ap.runner.current();

        // journal the rep that just completed (unrated); ephemeral sessions
        // persist nothing until quick_rate (the FK on reps.loop_id would
        // reject the LoopId(0) sentinel anyway)
        if self.ephemeral.is_none() {
            if let Err(e) = self.store.record_rep(NewRep {
                loop_id: done.loop_id,
                plan_id: Some(plan_id),
                mode: mode_str(done.mode).into(),
                rate: done.rate,
                rating: None,
                is_retest: false,
            }) {
                eprintln!("earworm: rep journal write failed: {e}");
            }
        }

        match next {
            Some(spec) => {
                if spec.step_idx != done.step_idx {
                    events.push(Event {
                        event: "step_finished".into(),
                        data: json!({"step_idx": done.step_idx}),
                    });
                }
                if let Err(e) = self.apply_rep(spec) {
                    eprintln!("earworm: rep apply failed: {e}");
                    return;
                }
                events.push(Event {
                    event: "rep_changed".into(),
                    data: serde_json::to_value(spec).unwrap_or(Value::Null),
                });
            }
            None => {
                events.push(Event {
                    event: "step_finished".into(),
                    data: json!({"step_idx": done.step_idx}),
                });
                self.audio.send(EngineCmd::Pause);
                self.active_plan = None;
                events.push(Event {
                    event: "plan_finished".into(),
                    data: Value::Null,
                });
            }
        }
    }

    // --- stems -------------------------------------------------------------

    fn stems_cache_dir(&self, file_hash: &str) -> PathBuf {
        self.stems_dir.join(file_hash)
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
        let cache = self.stems_cache_dir(&song.file_hash);
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
        let separating = self.separating.clone();
        let audio_path = PathBuf::from(&song.path);
        let song_id = p.song_id;
        std::thread::spawn(move || {
            let result = separator.separate(&audio_path, &cache);
            separating.lock().unwrap().remove(&song_id.0);
            let data = match result {
                Ok(_) => json!({"song_id": song_id, "state": "done"}),
                Err(e) => json!({"song_id": song_id, "state": "failed", "error": e}),
            };
            let _ = tx.send(Event {
                event: "stems_progress".into(),
                data,
            });
        });
        Ok(json!({"state": "running"}))
    }

    fn stems_status(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        let song = self.song_row(p.song_id)?;
        let state = if self.separating.lock().unwrap().contains(&p.song_id.0) {
            "running"
        } else if Self::stems_cached(&self.stems_cache_dir(&song.file_hash)) {
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

    // --- capture -----------------------------------------------------------

    fn capture_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            node_id: u32,
            buffer_secs: Option<f64>,
        }
        let p: P = from_params(p)?;
        self.capture
            .start(p.node_id, p.buffer_secs.unwrap_or(180.0))?;
        Ok(Value::Null)
    }

    fn capture_status(&mut self) -> Result<Value, String> {
        Ok(match self.capture.status() {
            Some((filled, node)) => json!({
                "running": true,
                "filled_secs": filled,
                "app": node.app,
                "media": node.media,
            }),
            None => json!({ "running": false }),
        })
    }

    /// Snapshot the last `last_secs` of the rolling capture to a WAV under
    /// `captures_dir`, then funnel it through `song.import` (hash, sidecar,
    /// peaks all reused).
    fn capture_grab(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            last_secs: f64,
        }
        let p: P = from_params(p)?;
        let (_, node) = self.capture.status().ok_or("no capture running")?;
        let samples = self.capture.snapshot(p.last_secs)?;
        if samples.is_empty() {
            return Err("capture buffer is empty".into());
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stem = if node.media.is_empty() {
            format!("{}-{ts}", sanitize_filename(&node.app))
        } else {
            format!(
                "{}-{}-{ts}",
                sanitize_filename(&node.app),
                sanitize_filename(&node.media)
            )
        };
        let path = self.captures_dir.join(format!("{stem}.wav"));
        engine::capture::write_wav(&path, &samples).err_str()?;
        let title = if node.media.is_empty() {
            node.app.clone()
        } else {
            format!("{} — {}", node.app, node.media)
        };
        self.song_import(json!({
            "path": path.to_string_lossy(),
            "title": title,
        }))
    }

    // --- library ---------------------------------------------------------

    fn song_import(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            path: String,
            title: Option<String>,
        }
        let p: P = from_params(p)?;
        let path = Path::new(&p.path);
        let hash = engine::decode::file_hash(path).err_str()?;
        if let Some(existing) = self.store.song_by_hash(&hash).err_str()? {
            return serde_json::to_value(existing).err_str();
        }
        let buf = engine::decode::decode_file(path).err_str()?;
        // explicit title wins over the file stem
        let title = p.title.unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_owned()
        });
        let song = self
            .store
            .insert_song(NewSong {
                title: &title,
                artist: None,
                path: &p.path,
                file_hash: &hash,
                duration_secs: buf.duration_secs(),
            })
            .err_str()?;
        if let Some(sc) = practice::sidecar::read_sidecar(path).err_str()? {
            self.restore_sidecar(song.id, &sc)?;
        }
        serde_json::to_value(song).err_str()
    }

    /// Restore annotations from a sidecar into a freshly imported song.
    /// Ids are re-assigned by insertion; junction/plan references are remapped.
    fn restore_sidecar(
        &mut self,
        song_id: SongId,
        sc: &practice::sidecar::Sidecar,
    ) -> Result<(), String> {
        let new_sections = self
            .store
            .replace_sections(
                song_id,
                &sc.sections
                    .iter()
                    .map(|s| NewSection {
                        name: &s.name,
                        start: s.start,
                        end: s.end,
                        position: s.position,
                    })
                    .collect::<Vec<_>>(),
            )
            .err_str()?;
        let sec_map: HashMap<_, _> = sc
            .sections
            .iter()
            .filter_map(|old| {
                let new = new_sections.iter().find(|n| n.position == old.position)?;
                Some((old.id, new.id))
            })
            .collect();
        let mut loop_map: HashMap<LoopId, LoopId> = HashMap::new();
        for l in &sc.loops {
            let kind = match l.kind {
                LoopKind::Manual => LoopKind::Manual,
                LoopKind::Junction {
                    from_section,
                    to_section,
                } => LoopKind::Junction {
                    from_section: *sec_map.get(&from_section).unwrap_or(&from_section),
                    to_section: *sec_map.get(&to_section).unwrap_or(&to_section),
                },
            };
            let new = self
                .store
                .insert_loop(
                    song_id,
                    NewLoop {
                        name: &l.name,
                        start: l.start,
                        end: l.end,
                        kind,
                    },
                )
                .err_str()?;
            loop_map.insert(l.id, new.id);
        }
        for plan in &sc.plans {
            let steps: Vec<PlanStep> = plan
                .steps
                .iter()
                .map(|s| remap_step(s, &loop_map))
                .collect();
            self.store
                .save_plan(song_id, &plan.name, &steps)
                .err_str()?;
        }
        self.write_sidecar_for(song_id);
        Ok(())
    }

    fn song_open(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        let song = self.song_row(p.song_id)?;
        let buf = engine::decode::decode_file(Path::new(&song.path)).err_str()?;
        let peaks = engine::peaks::load_or_compute(&buf, &song.file_hash).err_str()?;
        // Cached stems auto-load as a 4-stem set; otherwise the plain mix.
        let cache = self.stems_cache_dir(&song.file_hash);
        let stems = if Self::stems_cached(&cache) {
            let mut bufs = Vec::with_capacity(STEM_NAMES.len());
            for name in STEM_NAMES {
                bufs.push(
                    engine::decode::decode_file(&cache.join(format!("{name}.wav"))).err_str()?,
                );
            }
            self.audio.load(engine::buffer::StemSet::new(bufs));
            true
        } else {
            self.audio.load(engine::buffer::StemSet::single(buf));
            false
        };
        let out = json!({
            "song": song,
            "sections": self.store.list_sections(p.song_id).err_str()?,
            "loops": self.store.list_loops(p.song_id).err_str()?,
            "plans": self.store.list_plans(p.song_id).err_str()?,
            "peaks": peaks,
            "stems": stems,
        });
        self.open_song = Some(OpenSong { song, stems });
        Ok(out)
    }

    fn section_replace(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct SecIn {
            name: String,
            start: f64,
            end: f64,
            position: i32,
        }
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            sections: Vec<SecIn>,
        }
        let p: P = from_params(p)?;
        let sections = self
            .store
            .replace_sections(
                p.song_id,
                &p.sections
                    .iter()
                    .map(|s| NewSection {
                        name: &s.name,
                        start: s.start,
                        end: s.end,
                        position: s.position,
                    })
                    .collect::<Vec<_>>(),
            )
            .err_str()?;
        let junctions = self.refresh_junctions(p.song_id, 2.0, 2.0)?;
        self.write_sidecar_for(p.song_id);
        Ok(json!({ "sections": sections, "junctions": junctions }))
    }

    /// Delete existing junction loops for the song and re-derive them from
    /// its current sections.
    fn refresh_junctions(
        &mut self,
        song_id: SongId,
        tail: f64,
        head: f64,
    ) -> Result<Vec<LoopRegion>, String> {
        for l in self.store.list_loops(song_id).err_str()? {
            if matches!(l.kind, LoopKind::Junction { .. }) {
                self.store.delete_loop(l.id).err_str()?;
            }
        }
        let sections = self.store.list_sections(song_id).err_str()?;
        let mut saved = Vec::new();
        for j in practice::junction::derive_junctions(&sections, tail, head) {
            saved.push(
                self.store
                    .insert_loop(
                        song_id,
                        NewLoop {
                            name: &j.name,
                            start: j.start,
                            end: j.end,
                            kind: j.kind,
                        },
                    )
                    .err_str()?,
            );
        }
        Ok(saved)
    }

    fn junctions_derive(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            tail: Option<f64>,
            head: Option<f64>,
        }
        let p: P = from_params(p)?;
        let junctions =
            self.refresh_junctions(p.song_id, p.tail.unwrap_or(2.0), p.head.unwrap_or(2.0))?;
        self.write_sidecar_for(p.song_id);
        serde_json::to_value(junctions).err_str()
    }

    fn loop_create(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            name: String,
            start: f64,
            end: f64,
        }
        let p: P = from_params(p)?;
        let l = self
            .store
            .insert_loop(
                p.song_id,
                NewLoop {
                    name: &p.name,
                    start: p.start,
                    end: p.end,
                    kind: LoopKind::Manual,
                },
            )
            .err_str()?;
        self.write_sidecar_for(p.song_id);
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
            .store
            .loop_by_id(p.loop_id)
            .err_str()?
            .ok_or_else(|| format!("loop not found: {}", p.loop_id.0))?;
        let updated = self
            .store
            .update_loop(
                p.loop_id,
                p.name.as_deref().unwrap_or(&old.name),
                p.start.unwrap_or(old.start),
                p.end.unwrap_or(old.end),
            )
            .err_str()?;
        self.write_sidecar_for(old.song_id);
        serde_json::to_value(updated).err_str()
    }

    fn loop_delete(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            loop_id: LoopId,
        }
        let p: P = from_params(p)?;
        let l = self
            .store
            .loop_by_id(p.loop_id)
            .err_str()?
            .ok_or_else(|| format!("loop not found: {}", p.loop_id.0))?;
        self.store.delete_loop(p.loop_id).err_str()?;
        self.write_sidecar_for(l.song_id);
        Ok(Value::Null)
    }

    fn loop_list(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        serde_json::to_value(self.store.list_loops(p.song_id).err_str()?).err_str()
    }

    fn plan_save(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            name: String,
            steps: Vec<PlanStep>,
        }
        let p: P = from_params(p)?;
        let plan = self
            .store
            .save_plan(p.song_id, &p.name, &p.steps)
            .err_str()?;
        self.write_sidecar_for(p.song_id);
        serde_json::to_value(plan).err_str()
    }

    fn plan_list(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        serde_json::to_value(self.store.list_plans(p.song_id).err_str()?).err_str()
    }

    // --- shared helpers ---------------------------------------------------

    fn song_row(&self, id: SongId) -> Result<Song, String> {
        self.store
            .list_songs()
            .err_str()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("song not found: {}", id.0))
    }

    /// Mirror annotations to the JSON sidecar; DB is primary, so IO errors
    /// only log to stderr.
    fn write_sidecar_for(&self, song_id: SongId) {
        let write = || -> Result<(), String> {
            let song = self.song_row(song_id)?;
            let sc = practice::sidecar::Sidecar {
                version: 1,
                sections: self.store.list_sections(song_id).err_str()?,
                loops: self.store.list_loops(song_id).err_str()?,
                plans: self.store.list_plans(song_id).err_str()?,
                song,
            };
            practice::sidecar::write_sidecar(&sc).err_str()?;
            Ok(())
        };
        if let Err(e) = write() {
            eprintln!("earworm: sidecar write failed for song {}: {e}", song_id.0);
        }
    }
}

fn default_captures_dir() -> PathBuf {
    // ~/music, matching this user's lowercase dirs (XDG parsing not worth it)
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("music/earworm-captures")
}

/// `~/.local/share/earworm/stems/<file_hash>/{vocals,drums,bass,other}.wav`
fn default_stems_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("earworm/stems")
}

/// Keep filenames boring: alphanumerics, dash, underscore, dot survive;
/// everything else becomes a dash (collapsed).
fn sanitize_filename(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "capture".into()
    } else {
        trimmed.into()
    }
}

/// `M:SS.t` — tenths are enough to recognize a riff by eye.
fn fmt_ts(secs: f64) -> String {
    let tenths = (secs * 10.0).round() as i64;
    format!("{}:{:02}.{}", tenths / 600, tenths % 600 / 10, tenths % 10)
}

fn mode_str(mode: RepMode) -> &'static str {
    match mode {
        RepMode::Listen => "listen",
        RepMode::Play => "play",
        RepMode::RecallSilent => "recall_silent",
    }
}

fn remap_step(step: &PlanStep, map: &HashMap<LoopId, LoopId>) -> PlanStep {
    let m = |id: LoopId| *map.get(&id).unwrap_or(&id);
    match step {
        PlanStep::ListenFirst { loop_id, reps } => PlanStep::ListenFirst {
            loop_id: m(*loop_id),
            reps: *reps,
        },
        PlanStep::PlayReps {
            loop_id,
            reps,
            curve,
        } => PlanStep::PlayReps {
            loop_id: m(*loop_id),
            reps: *reps,
            curve: *curve,
        },
        PlanStep::Rotation {
            loop_ids,
            rounds,
            reps_per_visit,
            curve,
        } => PlanStep::Rotation {
            loop_ids: loop_ids.iter().copied().map(m).collect(),
            rounds: *rounds,
            reps_per_visit: *reps_per_visit,
            curve: *curve,
        },
        PlanStep::RecallTest {
            loop_id,
            alternations,
            rate,
        } => PlanStep::RecallTest {
            loop_id: m(*loop_id),
            alternations: *alternations,
            rate: *rate,
        },
    }
}
