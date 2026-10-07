use rayon::prelude::*;
use std::fs::{self, Metadata};
use std::path::Path;
use std::time::SystemTime;

/// Windows file attributes used by OneDrive's Files On-Demand.
pub const ATTR_PINNED: u32 = 0x0008_0000; // "Always keep on this device"
pub const ATTR_UNPINNED: u32 = 0x0010_0000; // "Free up space"
const ATTR_CLOUD_ONLY: u32 = 0x1000 /* OFFLINE */ | 0x0004_0000 /* RECALL_ON_OPEN */ | 0x0040_0000 /* RECALL_ON_DATA_ACCESS */;

/// True when the file is only a placeholder: its content lives in the cloud, not on this disk.
pub fn is_cloud_only(attrs: u32) -> bool {
    attrs & ATTR_CLOUD_ONLY != 0
}

#[cfg(windows)]
pub fn attrs_of(path: &Path) -> u32 {
    use std::os::windows::fs::MetadataExt;
    fs::symlink_metadata(path).map(|m| m.file_attributes()).unwrap_or(0)
}

#[cfg(not(windows))]
pub fn attrs_of(_path: &Path) -> u32 {
    0
}

/// Bytes actually stored on this disk for one file. OneDrive "online-only"
/// placeholders report their full size but take no space, so they count as 0.
fn file_bytes(meta: &Metadata) -> u64 {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x40000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
        if meta.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0 {
            return 0;
        }
    }
    meta.len()
}

/// Total size of a file or folder. Never follows symlinks or junctions, so a
/// link pointing elsewhere can't inflate the number (or get deleted through).
pub fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.file_type().is_symlink() {
        0
    } else if meta.is_dir() {
        dir_size(path)
    } else {
        file_bytes(&meta)
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else {
        return 0;
    };
    let entries: Vec<fs::DirEntry> = rd.flatten().collect();
    entries
        .par_iter()
        .map(|e| {
            let Ok(ft) = e.file_type() else { return 0 };
            if ft.is_symlink() {
                0
            } else if ft.is_dir() {
                dir_size(&e.path())
            } else {
                e.metadata().map(|m| file_bytes(&m)).unwrap_or(0)
            }
        })
        .sum()
}

/// Newest modification time of any file under `root`, skipping folders named
/// in `skip` (build output, dependencies, .git). Looks `max_depth` levels deep
/// and stops after `max_entries` so huge trees stay fast.
pub fn newest_mtime(root: &Path, skip: &[&str], max_depth: usize, max_entries: usize) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    let mut seen = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > max_entries {
                return newest;
            }
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if depth < max_depth && !skip.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    stack.push((e.path(), depth + 1));
                }
            } else if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                if newest.is_none_or(|n| t > n) {
                    newest = Some(t);
                }
            }
        }
    }
    newest
}

pub fn days_since(t: SystemTime) -> u64 {
    SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_nested_folders() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("a/one.bin"), vec![0u8; 1000]).unwrap();
        fs::write(dir.path().join("a/b/two.bin"), vec![0u8; 500]).unwrap();
        assert_eq!(size_of(dir.path()), 1500);
        assert_eq!(size_of(&dir.path().join("a/one.bin")), 1000);
        assert_eq!(size_of(&dir.path().join("missing")), 0);
    }

    #[test]
    fn newest_mtime_skips_named_folders() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("index.js"), "x").unwrap();
        let before = newest_mtime(dir.path(), &["node_modules"], 3, 1000).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(dir.path().join("node_modules/dep.js"), "y").unwrap();
        let after = newest_mtime(dir.path(), &["node_modules"], 3, 1000).unwrap();
        assert_eq!(before, after, "files inside skipped folders must not count");
    }
}
