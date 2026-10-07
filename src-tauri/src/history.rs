//! A plain-text log of every cleanup, kept on this computer only (JSON lines, newest last).
//! It builds trust ("what did it actually do?") and makes bug reports easy.

use crate::model::{CleanOutcome, DiskInfo};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const MAX_ENTRIES: usize = 300;
const MAX_PATHS_PER_ITEM: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryOutcome {
    pub name: String,
    pub ok: bool,
    pub bytes: u64,
    pub recycled: bool,
    pub message: Option<String>,
    /// What was touched (capped), so the log answers "what did it delete?".
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Local time, ISO 8601.
    pub when: String,
    pub drive: String,
    pub app_version: String,
    pub free_before: u64,
    pub free_after: u64,
    pub outcomes: Vec<HistoryOutcome>,
}

/// The app's own data folder (named after its identifier, like the rest of its data). It must
/// not be the install folder: uninstalling or updating the app owns that one.
pub fn dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("com.diskbreeze.app"))
}

/// Where 0.2.1 and 0.3.0 kept the log: inside the install folder.
fn legacy_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("DiskBreeze"))
}

fn file() -> Option<PathBuf> {
    migrate_legacy();
    dir().map(|d| d.join("history.jsonl"))
}

/// One-time move of the log from the old location. Never overwrites a newer log.
fn migrate_legacy() {
    if let (Some(old), Some(new)) = (legacy_dir(), dir()) {
        let _ = migrate(&old.join("history.jsonl"), &new.join("history.jsonl"));
    }
}

fn migrate(old: &std::path::Path, new: &std::path::Path) -> std::io::Result<()> {
    if !old.is_file() || new.exists() {
        return Ok(());
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    // Copy first and only then remove the original, so a failure never loses the log.
    fs::copy(old, new)?;
    fs::remove_file(old)
}

pub fn entry_from(drive: &str, before: &DiskInfo, after: &DiskInfo, outcomes: &[CleanOutcome]) -> HistoryEntry {
    HistoryEntry {
        when: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        drive: drive.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        free_before: before.free,
        free_after: after.free,
        outcomes: outcomes
            .iter()
            .map(|o| HistoryOutcome {
                name: o.name.clone(),
                ok: o.ok,
                bytes: o.bytes,
                recycled: o.recycled,
                message: o.message.clone(),
                paths: o.paths.iter().take(MAX_PATHS_PER_ITEM).cloned().collect(),
            })
            .collect(),
    }
}

/// Append one entry. Logging must never get in the way of cleaning, so errors are returned
/// for the caller to ignore.
pub fn append(entry: &HistoryEntry) -> std::io::Result<()> {
    let (Some(dir), Some(file)) = (dir(), file()) else { return Ok(()) };
    fs::create_dir_all(&dir)?;
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&file)?;
    writeln!(f, "{line}")?;
    drop(f);
    trim(&file)
}

fn trim(file: &PathBuf) -> std::io::Result<()> {
    let text = fs::read_to_string(file)?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > MAX_ENTRIES {
        fs::write(file, lines[lines.len() - MAX_ENTRIES..].join("\n") + "\n")?;
    }
    Ok(())
}

/// Newest first. Lines that don't parse (e.g. from a newer version) are skipped.
pub fn load(limit: usize) -> Vec<HistoryEntry> {
    let Some(file) = file() else { return vec![] };
    let Ok(text) = fs::read_to_string(file) else { return vec![] };
    text.lines()
        .filter_map(|l| serde_json::from_str::<HistoryEntry>(l).ok())
        .rev()
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(name: &str) -> CleanOutcome {
        CleanOutcome {
            id: "x".into(),
            name: name.into(),
            ok: true,
            bytes: 5,
            recycled: false,
            message: None,
            paths: (0..100).map(|i| format!(r"C:\a\{i}")).collect(),
        }
    }

    #[test]
    fn migrates_once_and_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let old = d.path().join("DiskBreeze").join("history.jsonl");
        let new = d.path().join("com.diskbreeze.app").join("history.jsonl");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, "old log\n").unwrap();
        migrate(&old, &new).unwrap();
        assert_eq!(fs::read_to_string(&new).unwrap(), "old log\n");
        assert!(!old.exists(), "the original is removed after a successful copy");
        // A later, newer log at the old path must not clobber the new one.
        fs::write(&old, "stale\n").unwrap();
        migrate(&old, &new).unwrap();
        assert_eq!(fs::read_to_string(&new).unwrap(), "old log\n");
    }

    #[test]
    fn entries_cap_paths_and_round_trip() {
        let d = DiskInfo { mount: "C:\\".into(), total: 100, free: 40 };
        let e = entry_from("C:", &d, &d, &[outcome("npm cache")]);
        assert_eq!(e.outcomes[0].paths.len(), MAX_PATHS_PER_ITEM);
        let json = serde_json::to_string(&e).unwrap();
        let back: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.outcomes[0].name, "npm cache");
        assert_eq!(back.drive, "C:");
    }
}
