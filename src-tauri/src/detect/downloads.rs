//! Looks through Downloads for things people forget: OS images, installers,
//! archives they already unpacked, and game/app installer folders. Everything
//! here is personal, so it is always `ask` and always goes to the Recycle Bin.

use super::{ago, GB, MB};
use crate::model::{Finding, Item, Plan, Tier};
use crate::size;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

struct FileInfo {
    path: PathBuf,
    bytes: u64,
    days: u64,
}

pub fn detect() -> Vec<Finding> {
    let Some(dl) = dirs::download_dir() else { return vec![] };
    detect_in(&dl)
}

pub fn detect_in(dl: &Path) -> Vec<Finding> {
    let mut files = vec![];
    let mut dirs_seen = vec![];
    collect(dl, 0, &mut files, &mut dirs_seen);

    // Installer folders first, so their contents aren't listed twice.
    let installer_dirs: Vec<(PathBuf, u64)> = dirs_seen
        .iter()
        .filter(|d| looks_like_installer_folder(d))
        .map(|d| (d.clone(), size::size_of(d)))
        .filter(|(_, b)| *b >= 500 * MB)
        .collect();
    let inside_installer =
        |p: &Path| installer_dirs.iter().any(|(d, _)| p.starts_with(d));

    let (mut images, mut archives, mut installers, mut old_big) = (vec![], vec![], vec![], vec![]);
    for f in files.iter().filter(|f| !inside_installer(&f.path)) {
        let ext = extension(&f.path);
        if matches!(ext.as_str(), "iso" | "img" | "dmg") && f.bytes >= 100 * MB {
            images.push(f);
        } else if matches!(ext.as_str(), "zip" | "rar" | "7z" | "tgz" | "gz" | "tar" | "xz")
            && f.bytes >= 50 * MB
            && has_extracted_sibling(&f.path)
        {
            archives.push(f);
        } else if matches!(ext.as_str(), "exe" | "msi" | "msix" | "appx") && f.bytes >= 50 * MB && f.days >= 14 {
            installers.push(f);
        } else if f.bytes >= GB && f.days >= 90 {
            old_big.push(f);
        }
    }

    let mut out = vec![];
    out.extend(make(
        "downloads-disk-images",
        "Disk images (ISO files)",
        "Operating system and software images. Once installed or written to a USB stick you rarely need them again.",
        to_items(&images, |f| format!("downloaded {}", ago(f.days))),
    ));
    out.extend(make(
        "downloads-extracted-archives",
        "Archives you already unpacked",
        "Each of these has an unpacked folder right next to it, so the archive is a duplicate.",
        to_items(&archives, |_| "unpacked copy found next to it".into()),
    ));
    out.extend(make(
        "downloads-installers",
        "Old installers",
        "Setup files for apps you've probably already installed.",
        to_items(&installers, |f| format!("downloaded {}", ago(f.days))),
    ));
    out.extend(make(
        "downloads-installer-folders",
        "Installer folders",
        "Folders with a setup program and its data files. If you've already installed what's inside, they're only taking up space.",
        installer_dirs
            .iter()
            .map(|(d, b)| Item { path: d.to_string_lossy().into_owned(), bytes: *b, note: Some("setup program + data files".into()), open: None })
            .collect(),
    ));
    out.extend(make(
        "downloads-old-big-files",
        "Big files you haven't opened in months",
        "Large downloads that haven't changed in over 3 months.",
        to_items(&old_big, |f| format!("last changed {}", ago(f.days))),
    ));
    out
}

fn to_items(files: &[&FileInfo], note: impl Fn(&FileInfo) -> String) -> Vec<Item> {
    files
        .iter()
        .map(|f| Item { path: f.path.to_string_lossy().into_owned(), bytes: f.bytes, note: Some(note(f)), open: None })
        .collect()
}

fn make(id: &str, name: &str, what: &str, mut items: Vec<Item>) -> Option<Finding> {
    let bytes: u64 = items.iter().map(|i| i.bytes).sum();
    if bytes < 100 * MB {
        return None;
    }
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let plan = Plan::Recycle(items.iter().map(|i| PathBuf::from(&i.path)).collect());
    Some(Finding {
        id: id.into(),
        name: name.into(),
        category: "Downloads".into(),
        tier: Tier::Ask,
        what: what.into(),
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

fn collect(dir: &Path, depth: usize, files: &mut Vec<FileInfo>, dirs_seen: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if depth < 3 {
                dirs_seen.push(e.path());
                collect(&e.path(), depth + 1, files, dirs_seen);
            }
        } else if let Ok(meta) = e.metadata() {
            let days = meta.modified().map(size::days_since).unwrap_or(0);
            files.push(FileInfo { path: e.path(), bytes: size::size_of(&e.path()), days });
        }
    }
}

fn extension(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Letters and digits only, lowercased: "How.to.Fish v1.0" -> "howtofishv10".
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// True when a folder next to the archive looks like its unpacked contents,
/// e.g. `REPO.v0.3.2.rar` + `REPO.v0.3.2\` or `BOMBANANA!.v1.0.rar` + `BOMBANANA!\`.
pub fn has_extracted_sibling(archive: &Path) -> bool {
    let Some(parent) = archive.parent() else { return false };
    let mut stem = archive.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    if stem.to_lowercase().ends_with(".tar") {
        stem.truncate(stem.len() - 4);
    }
    let stem = norm(&stem);
    let Ok(rd) = fs::read_dir(parent) else { return false };
    rd.flatten().any(|e| {
        e.file_type().map(|t| t.is_dir()).unwrap_or(false) && {
            let d = norm(&e.file_name().to_string_lossy());
            d.len() >= 4 && stem.starts_with(&d)
        }
    })
}

fn looks_like_installer_folder(dir: &Path) -> bool {
    let Ok(rd) = fs::read_dir(dir) else { return false };
    let names: HashSet<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect();
    let has_setup = names.iter().any(|n| n.starts_with("setup") && n.ends_with(".exe"));
    let data_files = names.iter().filter(|n| n.ends_with(".bin")).count();
    has_setup && data_files >= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_unpacked_archives() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        fs::create_dir_all(p.join("BOMBANANA!")).unwrap();
        fs::write(p.join("BOMBANANA!.v1.0.0-OFME.rar"), "x").unwrap();
        fs::create_dir_all(p.join("REPO.v0.3.2-OFME")).unwrap();
        fs::write(p.join("REPO.v0.3.2-OFME.rar"), "x").unwrap();
        fs::write(p.join("lonely.zip"), "x").unwrap();
        fs::create_dir_all(p.join("abc")).unwrap(); // too short to count as a match
        fs::write(p.join("abcdef.zip"), "x").unwrap();
        assert!(has_extracted_sibling(&p.join("BOMBANANA!.v1.0.0-OFME.rar")));
        assert!(has_extracted_sibling(&p.join("REPO.v0.3.2-OFME.rar")));
        assert!(!has_extracted_sibling(&p.join("lonely.zip")));
        assert!(!has_extracted_sibling(&p.join("abcdef.zip")));
    }

    #[test]
    fn spots_installer_folders() {
        let d = tempfile::tempdir().unwrap();
        let game = d.path().join("Some Game [Repack]");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("setup.exe"), "x").unwrap();
        fs::write(game.join("data-1.bin"), "x").unwrap();
        assert!(looks_like_installer_folder(&game));
        assert!(!looks_like_installer_folder(d.path()));
    }
}
