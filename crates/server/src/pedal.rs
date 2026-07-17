//! The global pedal mapping: rows of `{trigger, action, slot?}` stored as JSON
//! in the `pedal_mapping` setting. Parsing is total — malformed rows are
//! skipped, malformed JSON yields an empty mapping.

use serde::Deserialize;

pub const PEDAL_MAPPING_KEY: &str = "pedal_mapping";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Binding {
    pub trigger: String,
    pub action: String,
    #[serde(default)]
    pub slot: Option<u32>,
}

pub fn parse_mapping(v: &serde_json::Value) -> Vec<Binding> {
    v.as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_rows_and_skips_malformed() {
        let v = json!([
            { "trigger": "pc:0:0", "action": "play_pause" },
            { "trigger": "pc:0:1", "action": "play_marker", "slot": 2 },
            { "nope": true },
        ]);
        let m = parse_mapping(&v);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].action, "play_pause");
        assert_eq!(m[1].slot, Some(2));
    }

    #[test]
    fn non_array_is_empty() {
        assert!(parse_mapping(&json!("garbage")).is_empty());
        assert!(parse_mapping(&json!(null)).is_empty());
    }
}
