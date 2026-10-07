pub mod apps;
pub mod bigfiles;
pub mod docker;
pub mod downloads;
pub mod onedrive;
pub mod projects;
pub mod recycle_bin;
pub mod steam;
pub mod wsl;

/// Top-level folders (lowercase) that belong to Windows, apps or game launchers. Never searched
/// for projects or big files on a drive.
pub const SYSTEM_DIRS: &[&str] = &[
    "windows", "program files", "program files (x86)", "programdata", "$recycle.bin",
    "system volume information", "recovery", "$winreagent", "windowsapps", "msocache",
    "config.msi", "perflogs", "$sysreset", "boot", "steamlibrary", "steamapps", "$windows.~bt", "$windows.~ws",
];

pub const MB: u64 = 1024 * 1024;
pub const GB: u64 = 1024 * MB;

pub fn ago(days: u64) -> String {
    match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        2..=59 => format!("{days} days ago"),
        60..=729 => format!("{} months ago", days / 30),
        _ => format!("{} years ago", days / 365),
    }
}
