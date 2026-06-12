use crate::model::{LoopKind, LoopRegion, LoopId, Section};

/// A junction loop spans the tail of one section into the head of the next.
/// `tail`/`head` are window lengths in seconds; windows are clamped so the
/// loop never extends outside the two sections. Sections are taken in
/// `position` order; non-adjacent gaps are included (the gap is part of the
/// transition). Returned loops have `id: LoopId(0)` (unsaved sentinel).
pub fn derive_junctions(sections: &[Section], tail: f64, head: f64) -> Vec<LoopRegion> {
    let mut ordered: Vec<&Section> = sections.iter().collect();
    ordered.sort_by_key(|s| s.position);
    ordered
        .windows(2)
        .map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            LoopRegion {
                id: LoopId(0),
                song_id: a.song_id,
                name: format!("{}→{}", a.name, b.name),
                start: (a.end - tail).max(a.start),
                end: (b.start + head).min(b.end),
                kind: LoopKind::Junction { from_section: a.id, to_section: b.id },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SectionId, SongId};

    fn sec(id: i64, name: &str, start: f64, end: f64, position: i32) -> Section {
        Section {
            id: SectionId(id),
            song_id: SongId(1),
            name: name.into(),
            start,
            end,
            position,
        }
    }

    #[test]
    fn derives_one_loop_per_adjacent_pair() {
        let secs = vec![
            sec(1, "Verse", 10.0, 30.0, 0),
            sec(2, "Chorus", 30.0, 50.0, 1),
            sec(3, "Bridge", 50.0, 60.0, 2),
        ];
        let loops = derive_junctions(&secs, 2.0, 2.0);
        assert_eq!(loops.len(), 2);
        assert_eq!(loops[0].name, "Verse→Chorus");
        assert_eq!(loops[0].start, 28.0);
        assert_eq!(loops[0].end, 32.0);
        assert_eq!(
            loops[0].kind,
            LoopKind::Junction { from_section: SectionId(1), to_section: SectionId(2) }
        );
    }

    #[test]
    fn windows_clamped_to_section_bounds() {
        let secs = vec![sec(1, "A", 0.0, 1.0, 0), sec(2, "B", 1.0, 1.5, 1)];
        let loops = derive_junctions(&secs, 5.0, 5.0);
        assert_eq!(loops[0].start, 0.0); // clamped to A.start
        assert_eq!(loops[0].end, 1.5); // clamped to B.end
    }

    #[test]
    fn gap_between_sections_is_included() {
        let secs = vec![sec(1, "A", 0.0, 10.0, 0), sec(2, "B", 14.0, 20.0, 1)];
        let loops = derive_junctions(&secs, 1.0, 1.0);
        assert_eq!(loops[0].start, 9.0);
        assert_eq!(loops[0].end, 15.0);
    }

    #[test]
    fn unsorted_input_is_ordered_by_position() {
        let secs = vec![sec(2, "B", 30.0, 50.0, 1), sec(1, "A", 10.0, 30.0, 0)];
        let loops = derive_junctions(&secs, 2.0, 2.0);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].name, "A→B");
    }

    #[test]
    fn fewer_than_two_sections_yields_nothing() {
        assert!(derive_junctions(&[], 2.0, 2.0).is_empty());
        assert!(derive_junctions(&[sec(1, "A", 0.0, 10.0, 0)], 2.0, 2.0).is_empty());
    }
}
