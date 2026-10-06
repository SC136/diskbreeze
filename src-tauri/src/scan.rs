use crate::detect::{downloads, projects, recycle_bin, steam};
use crate::model::{DiskInfo, Finding, ScanResult};
use crate::catalog;
use std::time::Instant;
use sysinfo::Disks;

pub fn disk_info() -> DiskInfo {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_lowercase();
    let disks = Disks::new_with_refreshed_list();
    let disk = disks
        .list()
        .iter()
        .find(|d| d.mount_point().to_string_lossy().to_lowercase().starts_with(&drive))
        .or_else(|| disks.list().first());
    match disk {
        Some(d) => DiskInfo {
            mount: d.mount_point().to_string_lossy().into_owned(),
            total: d.total_space(),
            free: d.available_space(),
        },
        None => DiskInfo { mount: drive, total: 0, free: 0 },
    }
}

/// Run every detector plus the catalog and return findings, biggest first.
pub fn run(progress: impl Fn(&str, &str) + Sync) -> ScanResult {
    let started = Instant::now();
    let disk = disk_info();

    progress("catalog", "Checking known space hogs…");
    let mut findings: Vec<Finding> = catalog::findings(catalog::WINDOWS);

    progress("projects", "Looking for old code projects…");
    let roots = projects::default_roots();
    let scan = projects::detect(&roots);
    findings.extend(scan.finding);

    progress("downloads", "Looking through Downloads…");
    findings.extend(downloads::detect());

    progress("steam", "Checking Steam games…");
    findings.extend(steam::detect());

    progress("bin", "Measuring the Recycle Bin…");
    findings.extend(recycle_bin::detect());

    findings.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    ScanResult {
        disk,
        findings,
        active_projects: scan.active,
        stale_projects: scan.stale,
        took_ms: started.elapsed().as_millis() as u64,
    }
}
