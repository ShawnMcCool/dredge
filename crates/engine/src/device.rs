//! Audio device enumeration (outputs and inputs).
//!
//! Linux: one-shot PipeWire registry scan, mirroring `capture.rs`.
//! Non-Linux: cpal host enumeration, mirroring `capture_cpal.rs`.

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AudioDevice {
    pub id: String, // opaque, backend-stable; PipeWire: object.serial; cpal: name
    pub name: String,
    pub is_default: bool,
}

// ─── Linux / PipeWire ────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
use pipewire as pw;

#[cfg(target_os = "linux")]
fn pw_err(e: pw::Error) -> crate::error::Error {
    std::io::Error::other(e.to_string()).into()
}

/// One-shot registry scan for output devices (media.class == "Audio/Sink").
#[cfg(target_os = "linux")]
pub fn list_output_devices() -> crate::error::Result<Vec<AudioDevice>> {
    let handle = std::thread::Builder::new()
        .name("dredge-pw-scan-out".into())
        .spawn(|| scan("Audio/Sink", "default.audio.sink"))?;
    handle
        .join()
        .map_err(|_| std::io::Error::other("pipewire scan thread panicked"))?
        .map_err(pw_err)
}

/// One-shot registry scan for input devices (media.class == "Audio/Source").
#[cfg(target_os = "linux")]
pub fn list_input_devices() -> crate::error::Result<Vec<AudioDevice>> {
    let handle = std::thread::Builder::new()
        .name("dredge-pw-scan-in".into())
        .spawn(|| scan("Audio/Source", "default.audio.source"))?;
    handle
        .join()
        .map_err(|_| std::io::Error::other("pipewire scan thread panicked"))?
        .map_err(pw_err)
}

/// Shared PipeWire scan: collects all nodes matching `media_class`, then marks
/// the one whose `node.name` matches the PipeWire `default` metadata key
/// `default_key` as `is_default`.
#[cfg(target_os = "linux")]
fn scan(media_class: &str, default_key: &str) -> Result<Vec<AudioDevice>, pw::Error> {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    // Collected devices (by node.name for default matching).
    struct Entry {
        device: AudioDevice,
        node_name: String,
    }
    let found: Rc<RefCell<Vec<Entry>>> = Rc::new(RefCell::new(Vec::new()));
    // Default node.name as reported by the `default` Metadata object, if seen.
    let default_node_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let media_class = media_class.to_owned();
    let default_key = default_key.to_owned();

    let _listener = registry
        .add_listener_local()
        .global({
            let found = found.clone();
            let default_node_name = default_node_name.clone();
            move |global| {
                let Some(props) = global.props.as_ref() else {
                    return;
                };

                // Capture the default metadata node (type Metadata, name "default").
                // Its properties include JSON values like:
                //   default.audio.sink = {"name":"<node.name>"}
                if props.get("metadata.name") == Some("default") {
                    if let Some(raw) = props.get(&default_key) {
                        // Value is a JSON object: {"name":"alsa_output.pci..."}
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
                            if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                                *default_node_name.borrow_mut() = Some(name.to_owned());
                            }
                        }
                    }
                    return;
                }

                if props.get("media.class") != Some(media_class.as_str()) {
                    return;
                }

                let node_name = props.get("node.name").unwrap_or("").to_owned();

                // Friendly display name: description → nick → node.name
                let name = props
                    .get("node.description")
                    .or_else(|| props.get("node.nick"))
                    .or_else(|| props.get("node.name"))
                    .unwrap_or("")
                    .to_owned();

                let id = props
                    .get("object.serial")
                    .unwrap_or("")
                    .to_owned()
                    .pipe_or_else(|| global.id.to_string());

                found.borrow_mut().push(Entry {
                    device: AudioDevice {
                        id,
                        name,
                        is_default: false,
                    },
                    node_name,
                });
            }
        })
        .register();

    let timer = mainloop.loop_().add_timer({
        let weak = mainloop.downgrade();
        move |_| {
            if let Some(ml) = weak.upgrade() {
                ml.quit();
            }
        }
    });
    timer
        .update_timer(Some(Duration::from_millis(300)), None)
        .into_result()
        .map_err(pw::Error::SpaError)?;

    mainloop.run();
    drop(timer);

    // Apply is_default based on the metadata we captured (best-effort).
    let default_name = default_node_name.borrow();
    let mut entries = found.take();
    if let Some(ref dn) = *default_name {
        for entry in &mut entries {
            if &entry.node_name == dn {
                entry.device.is_default = true;
            }
        }
    }

    Ok(entries.into_iter().map(|e| e.device).collect())
}

// ─── Non-Linux / cpal ────────────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
pub fn list_output_devices() -> crate::error::Result<Vec<AudioDevice>> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let default_name = host.default_output_device().and_then(|d| d.name().ok());
    let devices = host
        .output_devices()
        .map_err(|e| crate::error::Error::Audio(format!("enumerate output devices: {e}")))?;
    let mut out = Vec::new();
    for dev in devices {
        let name = dev.name().unwrap_or_else(|_| "unknown".into());
        let is_default = default_name.as_deref() == Some(name.as_str());
        out.push(AudioDevice {
            id: name.clone(),
            name,
            is_default,
        });
    }
    Ok(out)
}

#[cfg(not(target_os = "linux"))]
pub fn list_input_devices() -> crate::error::Result<Vec<AudioDevice>> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let devices = host
        .input_devices()
        .map_err(|e| crate::error::Error::Audio(format!("enumerate input devices: {e}")))?;
    let mut out = Vec::new();
    for dev in devices {
        let name = dev.name().unwrap_or_else(|_| "unknown".into());
        let is_default = default_name.as_deref() == Some(name.as_str());
        out.push(AudioDevice {
            id: name.clone(),
            name,
            is_default,
        });
    }
    Ok(out)
}

// ─── Helper trait (Linux only, avoids a let-else dance on the serial string) ─

#[cfg(target_os = "linux")]
trait PipeOrElse {
    fn pipe_or_else(self, f: impl FnOnce() -> String) -> String;
}

#[cfg(target_os = "linux")]
impl PipeOrElse for String {
    fn pipe_or_else(self, f: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            f()
        } else {
            self
        }
    }
}
