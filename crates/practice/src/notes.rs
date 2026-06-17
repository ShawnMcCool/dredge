//! The note document for a section: an ordered list of text and tab blocks.
//! Tab blocks are a fixed grid (`strings` rows × `width` cols, `-` for an empty
//! cell); the bottom row is the lowest string. Stored as serde_json.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Text { text: String },
    Tab {
        strings: usize,
        width: usize,
        rows: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotesDoc {
    pub blocks: Vec<Block>,
}

impl NotesDoc {
    /// True when there's nothing worth storing: no blocks, or only empty text.
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|b| match b {
            Block::Text { text } => text.trim().is_empty(),
            Block::Tab { .. } => false,
        })
    }

    /// Enforce the grid invariants on every tab block: `rows.len() == strings`
    /// and every row is exactly `width` chars.
    pub fn validate(&self) -> Result<(), String> {
        for b in &self.blocks {
            if let Block::Tab { strings, width, rows } = b {
                if rows.len() != *strings {
                    return Err(format!("tab: {} rows for {strings} strings", rows.len()));
                }
                if let Some(bad) = rows.iter().find(|r| r.chars().count() != *width) {
                    return Err(format!("tab: row {bad:?} is not {width} wide"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_json() {
        let doc = NotesDoc {
            blocks: vec![
                Block::Text { text: "intro riff".into() },
                Block::Tab { strings: 2, width: 4, rows: vec!["--5-".into(), "7---".into()] },
            ],
        };
        let s = serde_json::to_string(&doc).unwrap();
        let back: NotesDoc = serde_json::from_str(&s).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn empty_doc_is_empty() {
        assert!(NotesDoc::default().is_empty());
        assert!(NotesDoc { blocks: vec![Block::Text { text: "  \n".into() }] }.is_empty());
        assert!(!NotesDoc { blocks: vec![Block::Text { text: "x".into() }] }.is_empty());
    }

    #[test]
    fn validate_rejects_malformed_tab() {
        let bad = NotesDoc { blocks: vec![Block::Tab { strings: 2, width: 4, rows: vec!["--5-".into()] }] };
        assert!(bad.validate().is_err());
        let bad2 = NotesDoc { blocks: vec![Block::Tab { strings: 1, width: 4, rows: vec!["--".into()] }] };
        assert!(bad2.validate().is_err());
        let ok = NotesDoc { blocks: vec![Block::Tab { strings: 1, width: 4, rows: vec!["----".into()] }] };
        assert!(ok.validate().is_ok());
    }
}
