//! The user's own way of working through emails: a one-line "next step" on an
//! email, and the order chosen by hand in the to-do column. One file per
//! account, beside its settings: these are the user's notes, not a copy of the
//! server, so they do not live in the local cache.

use crate::account::config_dir;
use crate::store::bare_id;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Longest next step kept: one line, not a document.
const MAX_STEP: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notes {
    /// Message-ID (without angle brackets) -> the next step for that email.
    #[serde(default)]
    pub next: BTreeMap<String, String>,
    /// Message-IDs of the to-do column in the order chosen by hand, top first.
    #[serde(default)]
    pub todo_order: Vec<String>,
}

const FILE: &str = "notes.json";

pub fn load() -> Result<Notes> {
    load_from(&config_dir()?)
}

pub fn save(notes: &Notes) -> Result<()> {
    save_to(&config_dir()?, notes)
}

fn load_from(dir: &Path) -> Result<Notes> {
    let path = dir.join(FILE);
    if !path.exists() {
        return Ok(Notes::default());
    }
    let text = std::fs::read_to_string(&path)?;
    serde_json::from_str(&text).with_context(|| format!("invalid {}", path.display()))
}

fn save_to(dir: &Path, notes: &Notes) -> Result<()> {
    crate::files::write_atomic(&dir.join(FILE), serde_json::to_string_pretty(notes)?)
}

/// Sets the next step of an email; an empty text removes it. The text is kept
/// on one line and cut to a sensible length.
pub fn set_step(notes: &mut Notes, id: &str, text: &str) {
    let id = bare_id(id);
    if id.is_empty() {
        return;
    }
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let line: String = line.chars().take(MAX_STEP).collect();
    if line.is_empty() {
        notes.next.remove(&id);
    } else {
        notes.next.insert(id, line);
    }
}

/// Takes the next steps off emails (any of their ids).
pub fn clear_steps(notes: &mut Notes, ids: &[String]) {
    for id in ids {
        notes.next.remove(&bare_id(id));
    }
}

/// Replaces the order of the to-do column, without blanks or repeats.
pub fn set_order(notes: &mut Notes, ids: &[String]) {
    let mut seen = std::collections::HashSet::new();
    notes.todo_order = ids
        .iter()
        .map(|i| bare_id(i))
        .filter(|i| !i.is_empty() && seen.insert(i.clone()))
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_one_short_line_and_empty_removes() {
        let mut n = Notes::default();
        set_step(&mut n, "<a@x>", "  call   Anna\nbefore replying ");
        assert_eq!(n.next["a@x"], "call Anna before replying");
        set_step(&mut n, "a@x", &"x".repeat(500));
        assert_eq!(n.next["a@x"].chars().count(), MAX_STEP);
        set_step(&mut n, "<a@x>", "   ");
        assert!(n.next.is_empty());
        set_step(&mut n, "", "lost");
        assert!(n.next.is_empty());
    }

    #[test]
    fn order_drops_blanks_and_repeats_and_survives_a_save() {
        let dir = std::env::temp_dir().join(format!("pigeonhole-notes-{}", std::process::id()));
        let mut n = Notes::default();
        set_order(&mut n, &["<b@x>".into(), "".into(), "a@x".into(), "<b@x>".into()]);
        assert_eq!(n.todo_order, ["b@x", "a@x"]);
        set_step(&mut n, "b@x", "send the quote");
        clear_steps(&mut n, &["<a@x>".into()]);
        save_to(&dir, &n).unwrap();
        assert_eq!(load_from(&dir).unwrap(), n);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load_from(&dir).unwrap(), Notes::default());
    }
}
