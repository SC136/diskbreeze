mod catalog;
mod clean;
mod detect;
mod model;
mod paths;
mod scan;
mod size;

use base64::Engine;
use model::{CleanReport, Finding, Progress, ScanResult};
use std::process::Command;
use std::sync::Mutex;
use tauri::{Emitter, State};

#[derive(Default)]
struct AppState {
    findings: Mutex<Vec<Finding>>,
}

#[tauri::command]
async fn scan(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        scan::run(|stage, detail| {
            let _ = app.emit("progress", Progress { stage: stage.into(), detail: detail.into() });
        })
    })
    .await
    .map_err(|e| e.to_string())?;
    *state.findings.lock().unwrap() = result.findings.clone();
    Ok(result)
}

/// The frontend only ever sends finding ids. What gets deleted comes from the
/// scan we ran ourselves, never from the UI.
#[tauri::command]
async fn clean(ids: Vec<String>, state: State<'_, AppState>) -> Result<CleanReport, String> {
    let selected: Vec<Finding> = state
        .findings
        .lock()
        .unwrap()
        .iter()
        .filter(|f| ids.contains(&f.id))
        .cloned()
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let before = scan::disk_info();
        let refs: Vec<&Finding> = selected.iter().collect();
        let outcomes = clean::run(&refs);
        CleanReport { before, after: scan::disk_info(), outcomes }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_target(target: String) -> Result<(), String> {
    let t = target.as_str();
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
    let path = dir.join(format!("disk-cleanup-{}.png", chrono::Local::now().format("%Y-%m-%d")));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// `disk-doctor --scan-json` prints a scan as JSON; handy for testing and bug reports.
pub fn scan_json() -> String {
    serde_json::to_string_pretty(&scan::run(|_, _| {})).unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![scan, clean, open_target, reveal, save_poster])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
