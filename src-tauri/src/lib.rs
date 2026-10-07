mod admin;
mod catalog;
mod clean;
mod detect;
mod history;
mod model;
mod paths;
mod scan;
mod size;

use base64::Engine;
use model::{CleanReport, DriveInfo, Finding, Progress, ScanResult, Selection};
use std::collections::HashSet;
use std::process::Command;
use std::sync::Mutex;
use tauri::{Emitter, State};

#[derive(Default)]
struct AppState {
    findings: Mutex<Vec<Finding>>,
    /// The drive the current findings came from, e.g. "D:".
    drive: Mutex<String>,
}

#[tauri::command]
fn list_drives() -> Vec<DriveInfo> {
    scan::list_drives()
}

#[tauri::command]
fn get_history(limit: Option<usize>) -> Vec<history::HistoryEntry> {
    history::load(limit.unwrap_or(50).min(300))
}

/// Opens the folder that holds the log, so people can attach it to a bug report.
#[tauri::command]
fn open_history_folder() -> Result<(), String> {
    let dir = history::dir().ok_or("No data folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Command::new("explorer").arg(dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan(drive: Option<String>, app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let drive = scan::normalize(&drive.unwrap_or_else(paths::profile_drive));
    if !scan::list_drives().iter().any(|d| d.letter == drive) {
        return Err(format!("Drive {drive} isn't available"));
    }
    let d = drive.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        scan::run(&d, |stage, detail| {
            let _ = app.emit("progress", Progress { stage: stage.into(), detail: detail.into() });
        })
    })
    .await
    .map_err(|e| e.to_string())?;
    *state.findings.lock().unwrap() = result.findings.clone();
    *state.drive.lock().unwrap() = drive;
    Ok(result)
}

/// The frontend sends finding ids, and optionally which of a finding's items to
/// keep. Every path must be one the scan itself found; what gets deleted comes
/// from our own scan, never from the UI.
#[tauri::command]
async fn clean(selections: Vec<Selection>, state: State<'_, AppState>) -> Result<CleanReport, String> {
    let known = state.findings.lock().unwrap().clone();
    let mut selected: Vec<Finding> = vec![];
    for s in selections {
        let Some(found) = known.iter().find(|f| f.id == s.id) else { continue };
        let mut f = found.clone();
        if let Some(paths) = s.paths {
            if !f.selectable {
                return Err(format!("{} can't be cleaned item by item", f.name));
            }
            if !paths.iter().all(|p| f.owns(p)) {
                return Err("One of the selected items wasn't part of the scan".into());
            }
            let keep: HashSet<String> = paths.iter().map(|p| p.to_lowercase()).collect();
            if keep.is_empty() {
                continue;
            }
            f.plan = f.plan.restrict(&keep);
            f.items.retain(|i| keep.contains(&i.path.to_lowercase()));
            f.bytes = f.items.iter().map(|i| i.bytes).sum();
            f.action = f.plan.summary();
        }
        selected.push(f);
    }
    let drive = state.drive.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let before = scan::disk_info(&drive);
        let refs: Vec<&Finding> = selected.iter().collect();
        let outcomes = clean::run(&refs);
        let after = scan::disk_info(&drive);
        // Logging must never get in the way of the clean itself.
        let _ = history::append(&history::entry_from(&drive, &before, &after, &outcomes));
        CleanReport { before, after, outcomes }
    })
    .await
    .map_err(|e| e.to_string())
}

/// "1.5 GB" / "60 MB" for messages built in the backend.
pub fn fmt_bytes(b: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let b = b as f64;
    if b >= 1024.0 * MB {
        format!("{:.1} GB", b / (1024.0 * MB))
    } else {
        format!("{:.0} MB", b / MB)
    }
}

#[tauri::command]
fn open_target(target: String) -> Result<(), String> {
    let t = target.as_str();
    // "uninstall:<registry key we listed>": opens that program's own uninstaller.
    if let Some(key) = t.strip_prefix("uninstall:") {
        return detect::apps::launch_uninstaller(key);
    }
    if t == "run:cleanmgr" {
        return Command::new("cleanmgr").spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    if ["steam://", "shell:", "ms-settings:"].iter().any(|p| t.starts_with(p)) {
        return Command::new("explorer").arg(t).spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    Err("That link isn't allowed".into())
}

#[tauri::command]
fn reveal(path: String, state: State<'_, AppState>) -> Result<(), String> {
    if !state.findings.lock().unwrap().iter().any(|f| f.owns(&path)) {
        return Err("Unknown path".into());
    }
    Command::new("explorer").arg(format!("/select,{path}")).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_poster(png_base64: String) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(png_base64).map_err(|e| e.to_string())?;
    let dir = dirs::picture_dir().ok_or("No Pictures folder")?;
    // Time-stamped so saving twice in one day never overwrites an earlier card.
    let path = dir.join(format!("diskbreeze-{}.png", chrono::Local::now().format("%Y-%m-%d-%H%M%S")));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// `diskbreeze --scan-json` prints a scan as JSON; handy for testing and bug reports.
pub fn scan_json(drive: Option<&str>) -> String {
    let drive = scan::normalize(drive.unwrap_or(&paths::profile_drive()));
    serde_json::to_string_pretty(&scan::run(&drive, |_, _| {})).unwrap_or_default()
}

/// `diskbreeze --list-drives` prints the drives the picker would show.
pub fn drives_json() -> String {
    serde_json::to_string_pretty(&scan::list_drives()).unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![list_drives, get_history, open_history_folder, scan, clean, open_target, reveal, save_poster])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
