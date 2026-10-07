//! Finds large, old, loose files (disk images, archives, videos, VM disks, backups) on a
//! drive that doesn't hold the user's profile. Only a short list of file types is offered, so
//! the pieces of an installed game or app are never suggested. Everything found is personal,
//! so it is always `ask` and always goes to the Recycle Bin.

use super::{ago, GB, MB, SYSTEM_DIRS};
use crate::model::{Finding, Item, Plan, Tier};
use crate::size;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_DEPTH: usize = 7;
const SKIP: &[&str] = &[".git", "node_modules", "steamapps", "$recycle.bin", "system volume information"];
const KINDS: &[&str] = &[
    "iso", "img", "dmg", "zip", "rar", "7z", "tar", "gz", "tgz", "xz", "bz2", "mkv", "mp4",
    "avi", "mov", "wmv", "m4v", "vhd", "vhdx", "vmdk", "ova", "bak", "backup",
];

pub fn detect(drive: &str) -> Option<Finding> {
    let root = PathBuf::from(format!("{drive}\\"));
    if is_boot_media(&root) {
        return None;
    }
    make(scan(&root, GB, 90))
}

/// Windows installers and other bootable sticks: their big files (install.wim, images)
/// are what make them work, so they are never offered for cleanup.
fn is_boot_media(root: &Path) -> bool {
    root.join("bootmgr").exists() || root.join("bootmgr.efi").exists() || root.join(r"efi\boot").exists()
}

struct Big {
    path: PathBuf,
    bytes: u64,
    days: u64,
}

fn scan(root: &Path, min_bytes: u64, min_days: u64) -> Vec<Big> {
    let out = Mutex::new(vec![]);
    walk(root, 0, true, min_bytes, min_days, &out);
    out.into_inner().unwrap()
}

fn walk(dir: &Path, depth: usize, top: bool, min_bytes: u64, min_days: u64, out: &Mutex<Vec<Big>>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let entries: Vec<fs::DirEntry> = rd.flatten().collect();
    entries.par_iter().for_each(|e| {
        let Ok(ft) = e.file_type() else { return };
        if ft.is_symlink() {
            return;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        if ft.is_dir() {
            if SKIP.contains(&name.as_str()) || (top && SYSTEM_DIRS.contains(&name.as_str())) {
                return;
            }
            walk(&e.path(), depth + 1, false, min_bytes, min_days, out);
        } else {
            let ext = e.path().extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !KINDS.contains(&ext.as_str()) {
                return;
            }
            let Ok(meta) = e.metadata() else { return };
            let days = meta.modified().map(size::days_since).unwrap_or(0);
            let bytes = size::size_of(&e.path());
            if bytes >= min_bytes && days >= min_days {
                out.lock().unwrap().push(Big { path: e.path(), bytes, days });
            }
        }
    });
}

fn make(mut found: Vec<Big>) -> Option<Finding> {
    let bytes: u64 = found.iter().map(|b| b.bytes).sum();
    if bytes < 100 * MB {
        return None;
    }
    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let items: Vec<Item> = found
        .iter()
        .map(|b| Item {
            path: b.path.to_string_lossy().into_owned(),
            bytes: b.bytes,
            note: Some(format!("last changed {}", ago(b.days))),
            open: None,
        })
        .collect();
    let plan = Plan::Recycle(found.iter().map(|b| b.path.clone()).collect());
    Some(Finding {
        id: "bigfiles".into(),
        name: "Big files you haven't opened in months".into(),
        category: "Big files".into(),
        tier: Tier::Ask,
        what: "Large disk images, archives, videos, VM disks and backups that haven't changed in over 3 months. Only loose files like these are listed, never parts of an installed game or app.".into(),
        after: Some("They go to the Recycle Bin, so you can still get them back. Empty the bin to actually free the space.".into()),
        how: None,
        open: None,
        bytes,
        selectable: items.len() > 1,
        items,
        recycles: true,
        action: plan.summary(),
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn aged(path: &Path, days: u64, len: u64) {
        let f = fs::File::create(path).unwrap();
        f.set_len(len).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(days * 86_400)).unwrap();
    }

    #[test]
    fn bootable_media_is_left_alone() {
        let d = tempfile::tempdir().unwrap();
        assert!(!is_boot_media(d.path()));
        fs::write(d.path().join("bootmgr"), "x").unwrap();
        assert!(is_boot_media(d.path()));
        let e = tempfile::tempdir().unwrap();
        fs::create_dir_all(e.path().join("efi/boot")).unwrap();
        assert!(is_boot_media(e.path()));
    }

    #[test]
    fn offers_only_old_big_loose_files() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        fs::create_dir_all(r.join("backups")).unwrap();
        fs::create_dir_all(r.join("Game/data")).unwrap();
        fs::create_dir_all(r.join("node_modules")).unwrap();
        aged(&r.join("backups/old.iso"), 200, 5000);
        aged(&r.join("backups/new.iso"), 1, 5000); // too recent
        aged(&r.join("backups/small.iso"), 200, 10); // too small
        aged(&r.join("Game/data/level1.pak"), 200, 5000); // not a loose-file type
        aged(&r.join("node_modules/huge.zip"), 200, 5000); // skipped folder
        let found = scan(r, 1000, 90);
        let names: Vec<String> = found.iter().map(|b| b.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["old.iso"]);
    }
}
