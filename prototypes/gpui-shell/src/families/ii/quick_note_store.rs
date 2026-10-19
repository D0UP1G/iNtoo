//! Narrow adapter for the persisted current tab in QuickNote's JSON store.
//!
//! This adapter deliberately does not migrate `notepad.txt` or create a blank
//! tab. Writes require a valid existing current tab and preserve other JSON
//! fields/tabs.

use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickNoteStatus {
    Loading,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickNoteSnapshot {
    pub status: QuickNoteStatus,
    pub title: Option<String>,
    pub text: Option<String>,
    pub current_tab: Option<usize>,
    pub error: Option<String>,
}

impl QuickNoteSnapshot {
    pub fn loading() -> Self {
        Self {
            status: QuickNoteStatus::Loading,
            title: None,
            text: None,
            current_tab: None,
            error: None,
        }
    }

    fn unavailable(error: impl Into<String>) -> Self {
        Self {
            status: QuickNoteStatus::Unavailable,
            title: None,
            text: None,
            current_tab: None,
            error: Some(error.into()),
        }
    }
}

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn store_path() -> PathBuf {
    let state_root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"));
    state_root.join("user/notepad-tabs.json")
}

pub fn load() -> QuickNoteSnapshot {
    let path = store_path();
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return QuickNoteSnapshot::unavailable(format!(
                "Could not read {}: {error}",
                path.display()
            ));
        }
    };
    snapshot_from_bytes(&bytes)
}

pub fn save_current_text(text: &str) -> Result<QuickNoteSnapshot, String> {
    update_current_text(text)
}

pub fn clear_current_text() -> Result<QuickNoteSnapshot, String> {
    update_current_text("")
}

fn update_current_text(text: &str) -> Result<QuickNoteSnapshot, String> {
    let path = store_path();
    let original =
        fs::read(&path).map_err(|error| format!("Could not read note store: {error}"))?;
    let snapshot = snapshot_from_bytes(&original);
    if snapshot.status != QuickNoteStatus::Ready {
        return Err(snapshot
            .error
            .unwrap_or_else(|| "QuickNote store is not ready".into()));
    }
    let tab_index = snapshot
        .current_tab
        .ok_or_else(|| "QuickNote store has no valid current tab".to_owned())?;
    let mut document: Value = serde_json::from_slice(&original)
        .map_err(|error| format!("QuickNote store contains invalid JSON: {error}"))?;
    let tabs = document
        .get_mut("tabs")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "QuickNote store has no tabs array".to_owned())?;
    let tab = tabs
        .get_mut(tab_index)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "QuickNote current tab is not an object".to_owned())?;
    tab.insert("text".into(), Value::String(text.to_owned()));
    let serialized = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("Could not serialize QuickNote store: {error}"))?;
    atomic_replace(&path, &original, &serialized)?;
    Ok(snapshot_from_bytes(&serialized))
}

fn snapshot_from_bytes(bytes: &[u8]) -> QuickNoteSnapshot {
    let document: Value = match serde_json::from_slice(bytes) {
        Ok(document) => document,
        Err(error) => {
            return QuickNoteSnapshot::unavailable(format!("Invalid QuickNote JSON: {error}"));
        }
    };
    let Some(tabs) = document.get("tabs").and_then(Value::as_array) else {
        return QuickNoteSnapshot::unavailable("QuickNote JSON has no tabs array");
    };
    if tabs.is_empty() {
        return QuickNoteSnapshot::unavailable("QuickNote JSON has no saved tabs");
    }
    let requested = document
        .get("currentTab")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .unwrap_or(0);
    let current_tab = requested.min(tabs.len() - 1);
    let Some(tab) = tabs.get(current_tab).and_then(Value::as_object) else {
        return QuickNoteSnapshot::unavailable("QuickNote current tab is not an object");
    };
    QuickNoteSnapshot {
        status: QuickNoteStatus::Ready,
        title: tab.get("title").and_then(Value::as_str).map(str::to_owned),
        // QML's `text` property resolves a missing tab text to the empty string.
        // This represents a real stored tab, not a fabricated tab or default note.
        text: Some(
            tab.get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ),
        current_tab: Some(current_tab),
        error: None,
    }
}

fn atomic_replace(path: &Path, expected: &[u8], replacement: &[u8]) -> Result<(), String> {
    // Refuse to overwrite a concurrent QML or GPUI update observed between load
    // and commit. QML does not share a lock, so a tiny compare/rename race remains.
    let current =
        fs::read(path).map_err(|error| format!("Could not recheck note store: {error}"))?;
    if current != expected {
        return Err("QuickNote changed while saving; reload and try again".into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "QuickNote path has no parent directory".to_owned())?;
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".notepad-tabs.json.{}.{}.tmp",
        std::process::id(),
        id
    ));
    let old_permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| format!("Could not create note-store temporary file: {error}"))?;
    let write_result = (|| {
        file.write_all(replacement)?;
        file.sync_all()?;
        if let Some(permissions) = old_permissions {
            file.set_permissions(permissions)?;
        }
        fs::rename(&temp, path)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp);
        return Err(format!("Could not atomically save QuickNote: {error}"));
    }
    if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}
