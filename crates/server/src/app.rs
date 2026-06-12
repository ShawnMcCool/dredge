use crate::control::AudioControl;
use crate::protocol::{Request, Response};
use practice::model::{LoopId, LoopKind, LoopRegion, PlanId, PlanStep, Song, SongId};
use practice::runner::PlanRunner;
use practice::store::{NewLoop, NewSection, NewSong, Store};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

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

pub struct App {
    store: Store,
    audio: Box<dyn AudioControl>,
    open_song: Option<OpenSong>,
    active_plan: Option<ActivePlan>,
    last_position: Option<(f64, f64, bool)>, // secs, rate, playing
}

struct OpenSong {
    song: Song,
}

struct ActivePlan {
    plan_id: PlanId,
    runner: PlanRunner,
    loops: HashMap<LoopId, LoopRegion>,
}

impl App {
    pub fn new(store: Store, audio: Box<dyn AudioControl>) -> Self {
        Self {
            store,
            audio,
            open_song: None,
            active_plan: None,
            last_position: None,
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
            "song.list" => serde_json::to_value(self.store.list_songs().err_str()?).err_str(),
            "song.open" => self.song_open(p),
            "section.replace" => self.section_replace(p),
            "loop.create" => self.loop_create(p),
            "loop.delete" => self.loop_delete(p),
            "loop.list" => self.loop_list(p),
            "junctions.derive" => self.junctions_derive(p),
            "plan.save" => self.plan_save(p),
            "plan.list" => self.plan_list(p),
            _ => Err(format!("unknown command: {cmd}")),
        }
    }

    // --- library ---------------------------------------------------------

    fn song_import(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            path: String,
        }
        let p: P = from_params(p)?;
        let path = Path::new(&p.path);
        let hash = engine::decode::file_hash(path).err_str()?;
        if let Some(existing) = self.store.song_by_hash(&hash).err_str()? {
            return serde_json::to_value(existing).err_str();
        }
        let buf = engine::decode::decode_file(path).err_str()?;
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_owned();
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
        self.audio.load(buf);
        let out = json!({
            "song": song,
            "sections": self.store.list_sections(p.song_id).err_str()?,
            "loops": self.store.list_loops(p.song_id).err_str()?,
            "plans": self.store.list_plans(p.song_id).err_str()?,
            "peaks": peaks,
        });
        self.open_song = Some(OpenSong { song });
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
