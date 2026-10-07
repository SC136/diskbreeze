//! OneDrive "free up space": find big files that are fully available on this PC and haven't
//! been touched for a while, so they can become online-only. Nothing is deleted: the copy in
//! OneDrive stays, and the file downloads again the next time it is opened. Needs Files
//! On-Demand (on by default).

use super::{ago, MB};
use crate::model::{Finding, Item, Plan, Tier};
use crate::size;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MIN_FILE: u64 = 20 * MB;
const MIN_DAYS: u64 = 30;
const MAX_ITEMS: usize = 300;

/// OneDrive folders on this PC (personal and work/school), deduplicated.
pub fn roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect();
    roots.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    roots.dedup_by_key(|p| p.to_string_lossy().to_lowercase());
    roots
}

/// Worth freeing: big, fully local, not pinned ("always keep"), not already queued to free up,
/// and not used lately.
pub fn is_candidate(attrs: u32, bytes: u64, days_since_change: u64) -> bool {
    !size::is_cloud_only(attrs)
        && attrs & (size::ATTR_PINNED | size::ATTR_UNPINNED) == 0
        && bytes >= MIN_FILE
        && days_since_change >= MIN_DAYS
}

pub fn detect() -> Option<Finding> {
    let roots = roots();
    if roots.is_empty() {
        return None;
    }
    let found = Mutex::new(Vec::<(PathBuf, u64, u64)>::new());
    roots.par_iter().for_each(|r| walk(r, 0, &found));
    let mut found = found.into_inner().unwrap();
    found.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = found.iter().map(|f| f.1).sum();
    if total < 200 * MB {
        return None;
    }
    found.truncate(MAX_ITEMS);
    let items: Vec<Item> = found
        .iter()
        .map(|(p, bytes, days)| Item {
            path: p.to_string_lossy().into_owned(),
            bytes: *bytes,
            note: Some(format!("last changed {}", ago(*days))),
            open: None,
        })
        .collect();
    let bytes: u64 = items.iter().map(|i| i.bytes).sum();
    let plan = Plan::Dehydrate(found.iter().map(|f| f.0.clone()).collect());
    Some(Finding {
        id: "onedrive-free-up".into(),
        name: "OneDrive files you can keep online only".into(),
        category: "OneDrive".into(),
        tier: Tier::Ask,
        what: "Big files that are stored both here and in OneDrive and haven't changed in over a month. Making them online-only frees the space on this PC; the copy in OneDrive stays safe.".into(),
        after: Some("They show a cloud icon in File Explorer and download again when you open them (you need internet for that). Files you pinned with \"Always keep on this device\" are never listed.".into()),
        how: None,
        open: None,
        bytes,
        selectable: items.len() > 1,
        estimate: false,
        items,
        recycles: false,
        action: plan.summary(),
        plan,
    })
}

fn walk(dir: &Path, depth: usize, out: &Mutex<Vec<(PathBuf, u64, u64)>>) {
    if depth > 14 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let entries: Vec<fs::DirEntry> = rd.flatten().collect();
    entries.par_iter().for_each(|e| {
        let Ok(ft) = e.file_type() else { return };
        if ft.is_symlink() {
            return;
        }
        if ft.is_dir() {
            walk(&e.path(), depth + 1, out);
            return;
        }
        let Ok(meta) = e.metadata() else { return };
        let attrs = size::attrs_of(&e.path());
        let days = meta.modified().map(size::days_since).unwrap_or(0);
        if is_candidate(attrs, meta.len(), days) {
            out.lock().unwrap().push((e.path(), meta.len(), days));
        }
    });
}

fn under_a_root(path: &Path, roots: &[PathBuf]) -> bool {
    let p = path.to_string_lossy().to_lowercase();
    roots.iter().any(|r| p.starts_with(&format!("{}\\", r.to_string_lossy().to_lowercase().trim_end_matches('\\'))))
}

/// Mark the files "free up space" and wait (briefly) for OneDrive to turn them into placeholders.
pub fn dehydrate(paths: &[PathBuf]) -> Result<(), String> {
    let roots = roots();
    for p in paths {
        if !under_a_root(p, &roots) || !p.is_file() {
            return Err(format!("{} isn't a file in your OneDrive folder", p.display()));
        }
    }
    let mut failed = 0usize;
    for p in paths {
        let mut cmd = Command::new("attrib");
        cmd.arg("+U").arg("-P").arg(p);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        if !cmd.status().map(|s| s.success()).unwrap_or(false) {
            failed += 1;
        }
    }
    if failed == paths.len() {
        return Err("Windows wouldn't change those files. Is OneDrive running with Files On-Demand turned on?".into());
    }
    // OneDrive frees synced files within seconds; files it hasn't uploaded yet stay put.
    let deadline = Instant::now() + Duration::from_secs(25);
    while Instant::now() < deadline {
        if paths.iter().all(|p| size::is_cloud_only(size::attrs_of(p))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(700));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_big_local_unpinned_untouched_files_qualify() {
        let big = 50 * MB;
        assert!(is_candidate(0x20, big, 90), "plain local file");
        assert!(!is_candidate(0x20, big, 2), "used recently");
        assert!(!is_candidate(0x20, MB, 90), "too small");
        assert!(!is_candidate(0x20 | size::ATTR_PINNED, big, 90), "pinned: user wants it here");
        assert!(!is_candidate(0x20 | size::ATTR_UNPINNED, big, 90), "already queued to free up");
        assert!(!is_candidate(0x20 | 0x0040_0000, big, 90), "already online-only");
    }

    #[test]
    fn refuses_files_outside_onedrive() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("x.bin");
        fs::write(&f, "x").unwrap();
        assert!(dehydrate(&[f]).is_err());
    }
}
