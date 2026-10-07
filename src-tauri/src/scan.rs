use crate::catalog;
use crate::detect::{bigfiles, downloads, projects, recycle_bin, steam};
use crate::model::{DiskInfo, DriveInfo, Finding, ScanResult};
use crate::paths;
use std::time::Instant;
use sysinfo::Disks;

/// "d", "D:", "d:\" -> "D:"
pub fn normalize(drive: &str) -> String {
    let c = drive.trim().chars().next().unwrap_or('C').to_ascii_uppercase();
    format!("{c}:")
}

fn drive_letter(mount: &std::path::Path) -> Option<String> {
    paths::drive_of(mount)
}

/// Every local drive with a size, for the picker.
pub fn list_drives() -> Vec<DriveInfo> {
    let profile = paths::profile_drive();
    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<DriveInfo> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .filter_map(|d| {
            let letter = drive_letter(d.mount_point())?;
            Some(DriveInfo {
                label: d.name().to_string_lossy().into_owned(),
                total: d.total_space(),
                free: d.available_space(),
                removable: d.is_removable(),
                has_profile: letter.eq_ignore_ascii_case(&profile),
                letter,
            })
        })
        .collect();
    drives.sort_by(|a, b| b.has_profile.cmp(&a.has_profile).then(a.letter.cmp(&b.letter)));
    drives.dedup_by(|a, b| a.letter == b.letter);
    drives
}

pub fn disk_info(drive: &str) -> DiskInfo {
    let drive = normalize(drive);
    let found = list_drives().into_iter().find(|d| d.letter == drive);
    match found {
        Some(d) => DiskInfo { mount: format!("{}\\", d.letter), total: d.total, free: d.free },
        None => DiskInfo { mount: format!("{drive}\\"), total: 0, free: 0 },
    }
}

/// Run the detectors that make sense for `drive` and return findings, biggest first.
///
/// The drive holding the user's profile also gets the app-cache catalog and Downloads (they
/// live in the profile). Any other drive is searched for old projects, big loose files, Steam
/// games and its own Recycle Bin.
pub fn run(drive: &str, progress: impl Fn(&str, &str) + Sync) -> ScanResult {
    let started = Instant::now();
    let drive = normalize(drive);
    let disk = disk_info(&drive);
    let has_profile = drive.eq_ignore_ascii_case(&paths::profile_drive());
    let mut findings: Vec<Finding> = vec![];

    if has_profile {
        progress("catalog", "Checking known space hogs…");
        findings.extend(catalog::findings(catalog::WINDOWS));
    }

    progress("projects", "Looking for old code projects…");
    let scan = projects::detect(&projects::default_roots(&drive));
    findings.extend(scan.finding);

    if has_profile {
        progress("downloads", "Looking through Downloads…");
        findings.extend(downloads::detect());
    } else {
        progress("bigfiles", "Looking for big old files…");
        findings.extend(bigfiles::detect(&drive));
    }

    progress("steam", "Checking Steam games…");
    findings.extend(steam::detect(&drive));

    progress("bin", "Measuring the Recycle Bin…");
    findings.extend(recycle_bin::detect(&drive));

    findings.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    ScanResult {
        disk,
        findings,
        active_projects: scan.active,
        stale_projects: scan.stale,
        took_ms: started.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_drive_names() {
        assert_eq!(normalize("d"), "D:");
        assert_eq!(normalize("D:"), "D:");
        assert_eq!(normalize(r"e:\"), "E:");
        assert_eq!(normalize(""), "C:");
    }

    #[test]
    fn lists_at_least_the_profile_drive() {
        let drives = list_drives();
        assert!(drives.iter().any(|d| d.has_profile), "the profile drive must be listed");
        assert!(drives.windows(2).all(|w| w[0].letter != w[1].letter), "no duplicate drives");
    }
}
