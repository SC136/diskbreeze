//! Installed programs, biggest first, with a button that opens each program's own uninstaller
//! (the same one Windows Settings uses). We never uninstall anything ourselves.
//!
//! Safety: the UI only sends back the registry key id we listed. Before anything runs we check
//! the key sits under one of the three standard "Uninstall" keys and read the uninstall command
//! from the registry ourselves.

use super::MB;
use crate::model::{Finding, Item, Plan, Tier};
use crate::{paths, size};
use std::path::PathBuf;

const MIN_BYTES: u64 = 300 * MB;
const MAX_APPS: usize = 60;

const ROOTS: [(&str, &str); 3] = [
    ("HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ("HKLM", r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    ("HKCU", r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
];

#[derive(Debug, Clone)]
pub struct App {
    pub key_id: String,
    pub name: String,
    pub publisher: String,
    pub installed: String,
    pub bytes: u64,
}

/// Skip things that aren't programs a person installed: system components, updates, hidden entries.
pub fn counts_as_program(name: &str, system_component: bool, has_parent: bool, has_uninstall: bool) -> bool {
    !name.trim().is_empty() && !system_component && !has_parent && has_uninstall
}

/// 20240131 -> "Jan 2024"
pub fn nice_date(raw: &str) -> String {
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    if raw.len() == 8 && raw.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(m) = raw[4..6].parse::<usize>() {
            if (1..=12).contains(&m) {
                return format!("{} {}", M[m - 1], &raw[..4]);
            }
        }
    }
    String::new()
}

/// A key id we handed out is only valid if it points under one of the standard Uninstall keys.
pub fn valid_key_id(id: &str) -> Option<(&str, &str)> {
    let (hive, path) = id.split_once('|')?;
    let lower = path.to_lowercase();
    let ok = ROOTS.iter().any(|(h, r)| *h == hive && lower.starts_with(&format!("{}\\", r.to_lowercase())));
    (ok && !path.contains("..")).then_some((hive, path))
}

#[cfg(windows)]
pub fn list() -> Vec<App> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let mut apps = vec![];
    for (hive, root) in ROOTS {
        let base = RegKey::predef(if hive == "HKLM" { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER });
        let Ok(key) = base.open_subkey(root) else { continue };
        for sub in key.enum_keys().flatten() {
            let Ok(k) = key.open_subkey(&sub) else { continue };
            let name: String = k.get_value("DisplayName").unwrap_or_default();
            let system: u32 = k.get_value("SystemComponent").unwrap_or(0);
            let parent: String = k.get_value("ParentKeyName").unwrap_or_default();
            let uninstall: String = k.get_value("UninstallString").unwrap_or_default();
            if !counts_as_program(&name, system == 1, !parent.is_empty(), !uninstall.trim().is_empty()) {
                continue;
            }
            // Steam registers each game here too; the "Steam games" finding already covers them.
            if uninstall.to_lowercase().contains("steam://") {
                continue;
            }
            let est_kb: u32 = k.get_value("EstimatedSize").unwrap_or(0);
            let mut bytes = est_kb as u64 * 1024;
            if bytes == 0 {
                let loc: String = k.get_value("InstallLocation").unwrap_or_default();
                let loc = PathBuf::from(loc.trim().trim_matches('"'));
                if loc.as_os_str().len() > 3 && loc.is_dir() && !paths::is_protected(&loc) {
                    bytes = size::size_of(&loc);
                }
            }
            apps.push(App {
                key_id: format!("{hive}|{root}\\{sub}"),
                name,
                publisher: k.get_value("Publisher").unwrap_or_default(),
                installed: nice_date(&k.get_value::<String, _>("InstallDate").unwrap_or_default()),
                bytes,
            });
        }
    }
    // The same program can be registered twice (32- and 64-bit views).
    apps.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    apps.dedup_by(|a, b| a.name == b.name && a.bytes == b.bytes);
    apps
}

#[cfg(not(windows))]
pub fn list() -> Vec<App> {
    vec![]
}

pub fn detect() -> Option<Finding> {
    let apps: Vec<App> = list().into_iter().filter(|a| a.bytes >= MIN_BYTES).take(MAX_APPS).collect();
    if apps.is_empty() {
        return None;
    }
    let bytes: u64 = apps.iter().map(|a| a.bytes).sum();
    let items: Vec<Item> = apps
        .iter()
        .map(|a| {
            let detail = [a.publisher.as_str(), a.installed.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
            Item {
                path: a.key_id.clone(),
                bytes: a.bytes,
                note: Some(format!("{} · {}", a.name, if detail.is_empty() { "installed program".into() } else { detail })),
                open: Some(format!("uninstall:{}", a.key_id)),
            }
        })
        .collect();
    Some(Finding {
        id: "installed-apps".into(),
        name: "Installed programs".into(),
        category: "Apps".into(),
        tier: Tier::Manual,
        what: "Your biggest installed programs. Sizes are what each program reports about itself, so they can be rough.".into(),
        after: None,
        how: Some("Uninstall programs you no longer use. Click Uninstall… next to one: its own uninstaller opens, and Windows may ask for permission. Nothing is removed until you confirm there.".into()),
        open: Some("ms-settings:appsfeatures".into()),
        bytes,
        selectable: false,
        estimate: false,
        items,
        recycles: false,
        action: Plan::Manual.summary(),
        plan: Plan::Manual,
    })
}

/// Read the uninstall command for a key we listed. Fails for anything that isn't a standard
/// Uninstall key.
#[cfg(windows)]
pub fn uninstall_command(key_id: &str) -> Result<String, String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let (hive, path) = valid_key_id(key_id).ok_or("That isn't a program DiskBreeze listed")?;
    let base = RegKey::predef(if hive == "HKLM" { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER });
    let key = base.open_subkey(path).map_err(|_| "That program is no longer installed")?;
    let cmd: String = key.get_value("UninstallString").map_err(|_| "This program has no uninstaller")?;
    let cmd = cmd.trim().to_string();
    if cmd.is_empty() {
        return Err("This program has no uninstaller".into());
    }
    Ok(cmd)
}

#[cfg(not(windows))]
pub fn uninstall_command(_key_id: &str) -> Result<String, String> {
    Err("Not supported on this system".into())
}

/// Open the program's own uninstaller (it asks for admin permission itself when it needs it).
pub fn launch_uninstaller(key_id: &str) -> Result<(), String> {
    let cmd = uninstall_command(key_id)?;
    let mut c = std::process::Command::new("cmd");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `start` detaches the uninstaller so DiskBreeze isn't blocked waiting for it.
        c.raw_arg(format!("/C start \"\" {cmd}")).creation_flags(0x0800_0000);
    }
    c.spawn().map(|_| ()).map_err(|e| format!("couldn't open the uninstaller: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_non_programs() {
        assert!(counts_as_program("Blender", false, false, true));
        assert!(!counts_as_program("", false, false, true));
        assert!(!counts_as_program("KB5012345", false, true, true), "an update has a parent");
        assert!(!counts_as_program("Hidden thing", true, false, true));
        assert!(!counts_as_program("No uninstaller", false, false, false));
    }

    #[test]
    fn formats_install_dates() {
        assert_eq!(nice_date("20240131"), "Jan 2024");
        assert_eq!(nice_date("2024"), "");
        assert_eq!(nice_date("20241399"), "");
    }

    #[test]
    fn only_standard_uninstall_keys_are_accepted() {
        assert!(valid_key_id(r"HKLM|SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Blender").is_some());
        assert!(valid_key_id(r"HKCU|Software\Microsoft\Windows\CurrentVersion\Uninstall\Foo").is_some());
        assert!(valid_key_id(r"HKLM|SOFTWARE\Microsoft\Windows\CurrentVersion\Run\Evil").is_none());
        assert!(valid_key_id(r"HKCU|SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\..\Run\x").is_none());
        assert!(valid_key_id(r"HKLM|SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall").is_none(), "the parent key itself");
        assert!(valid_key_id("no-separator").is_none());
    }
}
