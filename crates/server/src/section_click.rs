//! Pure construction of the section-click schedule: intersect the analyzed beat
//! grid with the marked sections, accenting downbeats. No audio, no I/O.

use engine::pipeline::ClickMark;
use practice::model::{Analysis, Section};

/// Tolerance (seconds) for matching a beat to a downbeat — the two grids come
/// from the same analysis but are stored as separate float lists.
const DOWNBEAT_EPS: f64 = 0.001;

fn is_downbeat(beat: f64, downbeats: &[f64]) -> bool {
    downbeats.iter().any(|d| (d - beat).abs() <= DOWNBEAT_EPS)
}

/// Every beat that falls inside a `click_guide` section, accented on downbeats.
/// Empty when no section is marked.
pub fn build_schedule(analysis: &Analysis, sections: &[Section]) -> Vec<ClickMark> {
    let spans: Vec<(f64, f64)> = sections
        .iter()
        .filter(|s| s.click_guide)
        .map(|s| (s.start, s.end))
        .collect();
    if spans.is_empty() {
        return Vec::new();
    }
    analysis
        .beats
        .iter()
        .filter(|&&b| spans.iter().any(|&(s, e)| b >= s && b < e))
        .map(|&b| ClickMark { secs: b, accent: is_downbeat(b, &analysis.downbeats) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(id: i64, start: f64, end: f64, click: bool) -> Section {
        Section {
            id: practice::model::SectionId(id),
            song_id: practice::model::SongId(1),
            name: "s".into(),
            start,
            end,
            position: 0,
            click_guide: click,
        }
    }

    fn analysis(beats: Vec<f64>, downbeats: Vec<f64>) -> Analysis {
        Analysis { bpm: Some(120.0), beats, downbeats, sections: vec![], engine: "test".into() }
    }

    #[test]
    fn empty_when_no_section_marked() {
        let a = analysis(vec![0.0, 0.5, 1.0], vec![0.0]);
        let secs = vec![section(1, 0.0, 2.0, false)];
        assert!(build_schedule(&a, &secs).is_empty());
    }

    #[test]
    fn includes_only_beats_inside_marked_spans() {
        let a = analysis(vec![0.0, 0.5, 1.0, 1.5, 2.0], vec![0.0, 2.0]);
        // mark [1.0, 2.0): beats 1.0 and 1.5 (2.0 is the exclusive end)
        let secs = vec![section(1, 0.0, 1.0, false), section(2, 1.0, 2.0, true)];
        let marks = build_schedule(&a, &secs);
        let times: Vec<f64> = marks.iter().map(|m| m.secs).collect();
        assert_eq!(times, vec![1.0, 1.5]);
    }

    #[test]
    fn accents_downbeats() {
        let a = analysis(vec![0.0, 0.5, 1.0], vec![0.0, 1.0]);
        let secs = vec![section(1, 0.0, 2.0, true)];
        let marks = build_schedule(&a, &secs);
        assert!(marks[0].accent); // 0.0 downbeat
        assert!(!marks[1].accent); // 0.5 offbeat
        assert!(marks[2].accent); // 1.0 downbeat
    }
}
