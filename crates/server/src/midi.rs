//! MIDI input → normalized trigger strings. A background thread rescans ALSA
//! MIDI sources every few seconds and auto-connects to everything except
//! `Midi Through`, so the pedal works over USB or BLE-MIDI, hotplug included.
//! Raw messages normalize to compact trigger keys (`pc:0:0`) that the pedal
//! mapping is keyed by; everything else about the device stays out of the app.

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RESCAN: Duration = Duration::from_secs(2);

/// Normalize a raw MIDI message to a trigger key, or None for messages the
/// pedal mapping doesn't speak (note-off, clock, sysex, truncated).
pub fn normalize(msg: &[u8]) -> Option<String> {
    let status = *msg.first()?;
    let ch = status & 0x0F;
    match status & 0xF0 {
        0xC0 => Some(format!("pc:{ch}:{}", msg.get(1)?)),
        0x90 if *msg.get(2)? > 0 => Some(format!("note:{ch}:{}", msg.get(1)?)),
        0xB0 => {
            let num = *msg.get(1)?;
            let val = *msg.get(2)?;
            let edge = if val >= 64 { "press" } else { "release" };
            Some(format!("cc:{ch}:{num}:{edge}"))
        }
        _ => None,
    }
}

/// Names of the currently connected MIDI sources, shared with the listener
/// thread. `App` reads it for the `midi.status` command.
#[derive(Clone, Default)]
pub struct MidiStatus(Arc<Mutex<Vec<String>>>);

impl MidiStatus {
    pub fn devices(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

/// Spawn the listener thread: rescan every `RESCAN`, connect to every MIDI
/// source except `Midi Through`, send each normalized trigger down `tx`.
/// Connections to vanished ports are dropped on the next rescan.
pub fn spawn(tx: mpsc::Sender<String>) -> MidiStatus {
    let status = MidiStatus::default();
    let shared = status.clone();
    std::thread::Builder::new()
        .name("midi-listen".into())
        .spawn(move || {
            let mut conns: HashMap<String, midir::MidiInputConnection<()>> = HashMap::new();
            loop {
                if let Ok(probe) = midir::MidiInput::new("dredge-probe") {
                    let ports: Vec<(String, midir::MidiInputPort)> = probe
                        .ports()
                        .into_iter()
                        .filter_map(|p| probe.port_name(&p).ok().map(|n| (n, p)))
                        .filter(|(n, _)| !n.contains("Midi Through"))
                        .collect();
                    conns.retain(|name, _| ports.iter().any(|(n, _)| n == name));
                    for (name, port) in ports {
                        if conns.contains_key(&name) {
                            continue;
                        }
                        let Ok(input) = midir::MidiInput::new("dredge") else {
                            continue;
                        };
                        let tx = tx.clone();
                        if let Ok(conn) = input.connect(
                            &port,
                            "dredge-in",
                            move |_ts, msg, _data| {
                                if let Some(t) = normalize(msg) {
                                    let _ = tx.send(t);
                                }
                            },
                            (),
                        ) {
                            conns.insert(name, conn);
                        }
                    }
                    *shared.0.lock().unwrap() = conns.keys().cloned().collect();
                }
                std::thread::sleep(RESCAN);
            }
        })
        .expect("spawn midi listener");
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_change_normalizes() {
        assert_eq!(normalize(&[0xC0, 2]), Some("pc:0:2".into()));
        assert_eq!(normalize(&[0xC5, 0]), Some("pc:5:0".into()));
    }

    #[test]
    fn note_on_normalizes_note_off_ignored() {
        assert_eq!(normalize(&[0x90, 60, 100]), Some("note:0:60".into()));
        assert_eq!(normalize(&[0x90, 60, 0]), None); // running-status note-off
        assert_eq!(normalize(&[0x80, 60, 64]), None);
    }

    #[test]
    fn cc_normalizes_press_release_on_value() {
        assert_eq!(normalize(&[0xB0, 64, 127]), Some("cc:0:64:press".into()));
        assert_eq!(normalize(&[0xB0, 64, 0]), Some("cc:0:64:release".into()));
    }

    #[test]
    fn junk_is_none() {
        assert_eq!(normalize(&[]), None);
        assert_eq!(normalize(&[0xF8]), None); // clock
        assert_eq!(normalize(&[0xC0]), None); // truncated
    }
}
