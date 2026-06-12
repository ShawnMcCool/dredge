use engine::capture::{CaptureNode, CaptureSession};

/// Everything App needs from the capture side — real PipeWire or test mock.
pub trait CaptureControl: Send {
    fn list_nodes(&mut self) -> Result<Vec<CaptureNode>, String>;
    fn start(&mut self, node_id: u32, buffer_secs: f64) -> Result<(), String>;
    fn stop(&mut self);
    /// (filled_secs, node) if a session is running.
    fn status(&self) -> Option<(f64, CaptureNode)>;
    /// Snapshot last `secs`, chronological interleaved samples.
    fn snapshot(&mut self, secs: f64) -> Result<Vec<f32>, String>;
}

/// Production implementation over `engine::capture`.
#[derive(Default)]
pub struct RealCapture {
    session: Option<CaptureSession>,
}

impl CaptureControl for RealCapture {
    fn list_nodes(&mut self) -> Result<Vec<CaptureNode>, String> {
        engine::capture::list_output_streams().map_err(|e| e.to_string())
    }

    fn start(&mut self, node_id: u32, buffer_secs: f64) -> Result<(), String> {
        let node = self
            .list_nodes()?
            .into_iter()
            .find(|n| n.id == node_id)
            .ok_or_else(|| format!("capture node not found: {node_id}"))?;
        self.stop();
        let session =
            engine::capture::start_capture(node, buffer_secs).map_err(|e| e.to_string())?;
        self.session = Some(session);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(s) = self.session.take() {
            s.stop();
        }
    }

    fn status(&self) -> Option<(f64, CaptureNode)> {
        self.session.as_ref().map(|s| {
            let filled = s.ring.lock().map(|r| r.filled_secs()).unwrap_or(0.0);
            (filled, s.node.clone())
        })
    }

    fn snapshot(&mut self, secs: f64) -> Result<Vec<f32>, String> {
        let s = self.session.as_ref().ok_or("no capture running")?;
        let ring = s.ring.lock().map_err(|_| "capture ring poisoned")?;
        Ok(ring.snapshot_last(secs))
    }
}

/// Test double: scripted node list and snapshot buffer.
#[derive(Default)]
pub struct MockCapture {
    pub nodes: Vec<CaptureNode>,
    pub snapshot_buf: Vec<f32>,
    pub filled_secs: f64,
    pub running: Option<CaptureNode>,
}

impl CaptureControl for MockCapture {
    fn list_nodes(&mut self) -> Result<Vec<CaptureNode>, String> {
        Ok(self.nodes.clone())
    }

    fn start(&mut self, node_id: u32, _buffer_secs: f64) -> Result<(), String> {
        let node = self
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or_else(|| format!("capture node not found: {node_id}"))?;
        self.running = Some(node.clone());
        Ok(())
    }

    fn stop(&mut self) {
        self.running = None;
    }

    fn status(&self) -> Option<(f64, CaptureNode)> {
        self.running.clone().map(|n| (self.filled_secs, n))
    }

    fn snapshot(&mut self, _secs: f64) -> Result<Vec<f32>, String> {
        if self.running.is_none() {
            return Err("no capture running".into());
        }
        Ok(self.snapshot_buf.clone())
    }
}
