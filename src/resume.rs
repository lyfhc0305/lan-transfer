//! What a receiver keeps of a batch that broke off, so that sending it again
//! continues where it stopped instead of starting over.
//!
//! The sender names each batch with a key derived from its entries and the
//! files' modification times, so the key changes when a file is edited. The
//! receiver stores, per key, how many files were completed and where the
//! part of the next file is. Kept for a week.
use crate::model::config_dir;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct State {
    /// Device ID of the sender.
    pub sender: String,
    /// Hash of the batch's entries.
    pub entries: String,
    /// Receive folder at the time; a changed folder starts over.
    pub root: PathBuf,
    /// Files (not folders) completed, in list order.
    pub files_done: usize,
    /// Top-level folders already created, by the name in the batch.
    pub folders: Vec<(String, PathBuf)>,
    /// Items saved so far (top level), for the transfers list.
    pub saved: Vec<PathBuf>,
    /// Received part of the next file.
    pub part: Option<PathBuf>,
}

fn dir() -> PathBuf {
    config_dir().join("resume")
}

/// Keys come from the other computer; only plain hex is used as a file name.
pub fn valid_key(key: &str) -> bool {
    key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

fn path(key: &str) -> PathBuf {
    dir().join(format!("{key}.json"))
}

/// Where the part of the next file is kept: next to where it will be saved,
/// so that finishing it is a rename.
pub fn part_path(folder: &Path, key: &str) -> PathBuf {
    folder.join(format!(".lantransfer-{key}.part"))
}

pub fn load(key: &str) -> Option<State> {
    if !valid_key(key) {
        return None;
    }
    serde_json::from_slice(&fs::read(path(key)).ok()?).ok()
}

pub fn save(key: &str, state: &State) {
    if !valid_key(key) {
        return;
    }
    let _ = fs::create_dir_all(dir());
    if let Ok(data) = serde_json::to_vec(state) {
        let _ = fs::write(path(key), data);
    }
}

/// Forget a batch and delete its part file.
pub fn clear(key: &str) {
    if !valid_key(key) {
        return;
    }
    if let Some(part) = load(key).and_then(|s| s.part) {
        let _ = fs::remove_file(part);
    }
    let _ = fs::remove_file(path(key));
}

/// Remove batches not continued within a week.
pub fn expire() {
    let Ok(list) = fs::read_dir(dir()) else {
        return;
    };
    for e in list.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > KEEP);
        let name = e.file_name().to_string_lossy().into_owned();
        if let (true, Some(key)) = (old, name.strip_suffix(".json")) {
            clear(key);
        }
    }
}
