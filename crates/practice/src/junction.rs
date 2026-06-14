use crate::model::{LoopId, LoopKind, LoopRegion, Section};

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
                name_override: None,
                start: (a.end - tail).max(a.start),
                end: (b.start + head).min(b.end),
                kind: LoopKind::Junction {
                    from_section: a.id,
                    to_section: b.id,
                },
            }
        })
        .collect()
}

/// Seconds either side of a boundary when no downbeat exists on that side —
/// matches the default tail/head of the seconds-based derivation.
const FALLBACK_SECS: f64 = 2.0;

/// Bar-aware practice window around a section boundary: from the last
/// downbeat strictly before it to the first downbeat strictly after it.
/// A side with no downbeat falls back to `boundary ∓ 2.0 s`. A boundary
/// sitting exactly on a downbeat therefore gets the full bar either side.
pub fn junction_window(downbeats: &[f64], boundary: f64) -> (f64, f64) {
    let before = downbeats
        .iter()
        .copied()
        .filter(|d| *d < boundary)
        .fold(f64::NEG_INFINITY, f64::max);
    let after = downbeats
        .iter()
        .copied()
        .filter(|d| *d > boundary)
        .fold(f64::INFINITY, f64::min);
    (
        if before.is_finite() {
            before
        } else {
            boundary - FALLBACK_SECS
        },
        if after.is_finite() {
            after
        } else {
            boundary + FALLBACK_SECS
        },
    )
}

/// Downbeat-snapped variant of [`derive_junctions`]: each loop runs from the
/// last downbeat before the end of one section to the first downbeat after
/// the start of the next, clamped inside the two sections.
pub fn derive_junctions_snapped(sections: &[Section], downbeats: &[f64]) -> Vec<LoopRegion> {
    let mut ordered: Vec<&Section> = sections.iter().collect();
    ordered.sort_by_key(|s| s.position);
    ordered
        .windows(2)
        .map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let (start, _) = junction_window(downbeats, a.end);
            let (_, end) = junction_window(downbeats, b.start);
            LoopRegion {
                id: LoopId(0),
                song_id: a.song_id,
                name: format!("{}→{}", a.name, b.name),
                name_override: None,
                start: start.max(a.start),
                end: end.min(b.end),
                kind: LoopKind::Junction {
                    from_section: a.id,
                    to_section: b.id,
                },
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
            LoopKind::Junction {
                from_section: SectionId(1),
                to_section: SectionId(2)
            }
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

    const DOWNBEATS: [f64; 5] = [10.0, 12.0, 14.0, 16.0, 18.0];

    #[test]
    fn window_on_a_downbeat_spans_the_neighbouring_bars() {
        // strictly before / strictly after: the boundary's own downbeat is skipped
        assert_eq!(junction_window(&DOWNBEATS, 14.0), (12.0, 16.0));
    }

    #[test]
    fn window_between_downbeats_picks_the_enclosing_pair() {
        assert_eq!(junction_window(&DOWNBEATS, 13.2), (12.0, 14.0));
    }

    #[test]
    fn window_before_first_downbeat_falls_back_on_the_left() {
        assert_eq!(junction_window(&DOWNBEATS, 9.0), (7.0, 10.0));
    }

    #[test]
    fn window_after_last_downbeat_falls_back_on_the_right() {
        assert_eq!(junction_window(&DOWNBEATS, 19.0), (18.0, 21.0));
    }

    #[test]
    fn window_with_no_downbeats_is_the_seconds_fallback() {
        assert_eq!(junction_window(&[], 30.0), (28.0, 32.0));
    }

    #[test]
    fn snapped_junctions_land_on_downbeats() {
        let secs = vec![sec(1, "A", 0.0, 13.5, 0), sec(2, "B", 13.5, 30.0, 1)];
        let loops = derive_junctions_snapped(&secs, &DOWNBEATS);
        assert_eq!(loops.len(), 1);
        // boundary 13.5: last downbeat before = 12.0, first after = 14.0
        assert_eq!(loops[0].start, 12.0);
        assert_eq!(loops[0].end, 14.0);
        assert_eq!(
            loops[0].kind,
            LoopKind::Junction {
                from_section: SectionId(1),
                to_section: SectionId(2)
            }
        );
    }

    #[test]
    fn snapped_junctions_clamp_inside_the_sections() {
        // section A is tiny: window's left downbeat (10.0) precedes A.start
        let secs = vec![sec(1, "A", 12.5, 13.0, 0), sec(2, "B", 13.0, 13.8, 1)];
        let loops = derive_junctions_snapped(&secs, &DOWNBEATS);
        assert_eq!(loops[0].start, 12.5); // clamped to A.start (downbeat 12.0 is outside)
        assert_eq!(loops[0].end, 13.8); // clamped to B.end (downbeat 14.0 is outside)
    }
}
