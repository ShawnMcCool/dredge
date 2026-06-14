//! Dynamic loop names derived from a song's sections. A loop is named after the
//! section(s) it covers; the occurrence number distinguishes the Nth section of
//! a given name (the 2nd `verse` is `verse 2`). Names recompute as bounds or
//! sections change, unless the user pins a manual override (handled by the
//! caller). Section labels are used verbatim (lowercase `verse`, letters `A`).

use crate::model::Section;

/// Boundary tolerance (seconds). An edge within EPS of a section boundary counts
/// as "on" it — header-drag selections hit boundaries exactly; this absorbs
/// float noise and lets "fit to section" read as full coverage.
const EPS: f64 = 0.05;

/// `name occurrence` for a section — 1-based count among same-named sections, in
/// `position` order. The only section named `chorus` is `chorus 1`.
fn occurrence_label(section: &Section, sections: &[Section]) -> String {
    let mut ordered: Vec<&Section> = sections.iter().collect();
    ordered.sort_by_key(|s| s.position);
    let n = ordered
        .iter()
        .filter(|s| s.name == section.name)
        .take_while(|s| s.id != section.id)
        .count()
        + 1;
    format!("{} {}", section.name, n)
}

/// Sections the loop `[start, end]` overlaps, in `position` order. A section
/// barely touched at a shared boundary (within EPS) does not count.
fn overlapping(start: f64, end: f64, sections: &[Section]) -> Vec<&Section> {
    let mut ordered: Vec<&Section> = sections
        .iter()
        .filter(|s| s.start < end - EPS && s.end > start + EPS)
        .collect();
    ordered.sort_by_key(|s| s.position);
    ordered
}

/// `riff m:ss.t–m:ss.t` — the fallback when the loop covers no section.
/// Mirrors `server::app::fmt_ts`.
fn fmt_ts(secs: f64) -> String {
    let tenths = (secs * 10.0).round() as i64;
    format!("{}:{:02}.{}", tenths / 600, tenths % 600 / 10, tenths % 10)
}

/// Compute a loop's display name from its bounds and the song's sections,
/// disambiguating against `existing` loop names with a `(n)` suffix.
pub fn loop_name(start: f64, end: f64, sections: &[Section], existing: &[String]) -> String {
    let base = base_name(start, end, sections);
    disambiguate(base, existing)
}

fn base_name(start: f64, end: f64, sections: &[Section]) -> String {
    let hit = overlapping(start, end, sections);
    match hit.as_slice() {
        [] => format!("riff {}–{}", fmt_ts(start), fmt_ts(end)),
        [s] => {
            let full = start <= s.start + EPS && end >= s.end - EPS;
            let label = occurrence_label(s, sections);
            if full {
                label
            } else {
                format!("sub {label}")
            }
        }
        [first, .., last] => {
            let left = edge_label(first, sections, start <= first.start + EPS);
            let right = edge_label(last, sections, end >= last.end - EPS);
            format!("{left} → {right}")
        }
    }
}

/// `verse 2` when the loop fully covers this endpoint section, `sub verse 2`
/// when it only partially covers it.
fn edge_label(section: &Section, sections: &[Section], full: bool) -> String {
    let label = occurrence_label(section, sections);
    if full {
        label
    } else {
        format!("sub {label}")
    }
}

/// Append `(2)`, `(3)`, … until the name is unique among `existing`.
fn disambiguate(base: String, existing: &[String]) -> String {
    if !existing.iter().any(|n| n == &base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base} ({n})");
        if !existing.iter().any(|x| x == &candidate) {
            return candidate;
        }
        n += 1;
    }
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

    // intro[0,10] verse[10,30] verse[30,50] chorus[50,70]
    fn song() -> Vec<Section> {
        vec![
            sec(1, "intro", 0.0, 10.0, 0),
            sec(2, "verse", 10.0, 30.0, 1),
            sec(3, "verse", 30.0, 50.0, 2),
            sec(4, "chorus", 50.0, 70.0, 3),
        ]
    }

    #[test]
    fn full_single_section_uses_occurrence() {
        assert_eq!(loop_name(30.0, 50.0, &song(), &[]), "verse 2");
    }

    #[test]
    fn first_occurrence_is_one() {
        assert_eq!(loop_name(0.0, 10.0, &song(), &[]), "intro 1");
    }

    #[test]
    fn strict_subset_is_sub() {
        assert_eq!(loop_name(34.0, 46.0, &song(), &[]), "sub verse 2");
    }

    #[test]
    fn spans_multiple_names_first_and_last() {
        // verse2[30,50] fully + chorus[50,70] fully
        assert_eq!(loop_name(30.0, 70.0, &song(), &[]), "verse 2 → chorus 1");
    }

    #[test]
    fn partial_end_section_is_sub() {
        // starts at verse2.start, ends inside chorus
        assert_eq!(
            loop_name(30.0, 60.0, &song(), &[]),
            "verse 2 → sub chorus 1"
        );
    }

    #[test]
    fn partial_start_section_is_sub() {
        assert_eq!(
            loop_name(40.0, 70.0, &song(), &[]),
            "sub verse 2 → chorus 1"
        );
    }

    #[test]
    fn middle_sections_dropped() {
        // intro..chorus spanning everything: only first & last named
        assert_eq!(loop_name(0.0, 70.0, &song(), &[]), "intro 1 → chorus 1");
    }

    #[test]
    fn boundary_within_eps_reads_as_full() {
        assert_eq!(loop_name(30.02, 49.97, &song(), &[]), "verse 2");
    }

    #[test]
    fn no_section_falls_back_to_timestamp() {
        assert_eq!(loop_name(83.0, 105.2, &[], &[]), "riff 1:23.0–1:45.2");
    }

    #[test]
    fn collision_gets_numeric_suffix() {
        let existing = vec!["verse 2".to_string()];
        assert_eq!(loop_name(30.0, 50.0, &song(), &existing), "verse 2 (2)");
    }

    #[test]
    fn collision_skips_taken_suffixes() {
        let existing = vec!["verse 2".to_string(), "verse 2 (2)".to_string()];
        assert_eq!(loop_name(30.0, 50.0, &song(), &existing), "verse 2 (3)");
    }
}
