// Usage: cargo run -p engine --example play -- <audio-file> [loop_start] [loop_end] [rate]
// Plays via PipeWire; Ctrl-C to stop. Prints position/wrap events.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = std::path::Path::new(&args[1]);
    let buf = engine::decode::decode_file(path).expect("decode");
    println!("decoded: {:.1}s", buf.duration_secs());
    let mut eng = engine::Engine::start().expect("engine");
    eng.load(engine::buffer::StemSet::single(buf));
    if let (Some(s), Some(e)) = (args.get(2), args.get(3)) {
        eng.send(engine::pipeline::EngineCmd::SetLoopSecs {
            start: s.parse().unwrap(),
            end: e.parse().unwrap(),
        });
    }
    if let Some(r) = args.get(4) {
        eng.send(engine::pipeline::EngineCmd::SetRate(r.parse().unwrap()));
    }
    eng.send(engine::pipeline::EngineCmd::Play);
    loop {
        for ev in eng.poll_events() {
            println!("{ev:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
