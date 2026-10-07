use crate::detect::{docker, onedrive};
use crate::model::{CleanOutcome, Finding, Plan};
use crate::{admin, paths, scan, size};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Clean the given findings. The Recycle Bin always goes last so files we just
/// moved there are emptied too only if the user selected it.
pub fn run(findings: &[&Finding]) -> Vec<CleanOutcome> {
    let mut ordered: Vec<&Finding> = findings.to_vec();
    ordered.sort_by_key(|f| matches!(f.plan, Plan::EmptyRecycleBin(_)));
    ordered.iter().map(|f| clean_one(f)).collect()
}

fn clean_one(f: &Finding) -> CleanOutcome {
    let mut targets = targets_of(&f.plan);
    if targets.is_empty() && matches!(f.plan, Plan::Command { .. } | Plan::Admin { .. }) {
        // A tool or admin step cleans its own folders (npm, Windows Update…): measure the
        // folders we listed for it.
        targets = f.items.iter().map(|i| PathBuf::from(&i.path)).collect();
    }
    // When the saving can't be read off a folder (component store, WSL disks), compare the
    // drive's free space instead.
    let drive = f.items.first().and_then(|i| paths::drive_of(Path::new(&i.path))).unwrap_or_else(paths::profile_drive);
    let by_free_space = matches!(f.plan, Plan::Admin { .. }) && targets.is_empty();
    let free_before = scan::disk_info(&drive).free;
    let before: u64 = targets.iter().map(|p| size::size_of(p)).sum();
    let result = execute(&f.plan);
    let after: u64 = targets.iter().map(|p| size::size_of(p)).sum();
    let free_delta = || scan::disk_info(&drive).free.saturating_sub(free_before);
    let mut freed = match &f.plan {
        Plan::EmptyRecycleBin(_) => f.bytes,
        _ if by_free_space => free_delta(),
        // System files can share disk space with other files (hard links, compression), so what
        // they add up to can be more than the drive gets back. Claim only what was gained.
        Plan::Admin { .. } => reconcile(before.saturating_sub(after), free_delta()),
        _ => before.saturating_sub(after),
    };
    // WSL disk files are compacted by diskpart; free space is the honest measure there too.
    if f.id == "wsl-vhdx-compact" {
        freed = scan::disk_info(&drive).free.saturating_sub(free_before).max(before.saturating_sub(after));
    }
    // Docker prune frees space inside Docker's disk file, not on the Windows drive.
    let inside_vm = docker::NO_HOST_SAVINGS.contains(&f.id.as_str());
    let note = inside_vm.then(|| {
        format!("Freed inside Docker (up to {}). To get it back on Windows, also shrink Docker's disk file.", crate::fmt_bytes(f.bytes))
    });
    if inside_vm {
        freed = 0;
    }
    let paths: Vec<String> = if targets.is_empty() {
        f.items.iter().map(|i| i.path.clone()).collect()
    } else {
        targets.iter().map(|p| p.to_string_lossy().into_owned()).collect()
    };
    let (id, name) = (f.id.clone(), f.name.clone());
    match result {
        Ok(()) => CleanOutcome { id, name, ok: true, bytes: freed, recycled: f.plan.recycles(), message: note, paths },
        Err(e) => CleanOutcome { id, name, ok: false, bytes: freed, recycled: false, message: Some(e), paths },
    }
}

/// How much to claim as freed: never more than the files added up to, and never more than the
/// drive's free space actually grew by.
fn reconcile(files_removed: u64, free_space_gained: u64) -> u64 {
    files_removed.min(free_space_gained)
}

fn targets_of(plan: &Plan) -> Vec<PathBuf> {
    match plan {
        Plan::Delete(p) | Plan::Recycle(p) => p.clone(),
        Plan::DeleteContents { dirs, .. } => dirs.clone(),
        Plan::Command { fallback: Some(fb), .. } => targets_of(fb),
        _ => vec![],
    }
}

fn execute(plan: &Plan) -> Result<(), String> {
    match plan {
        Plan::Delete(ps) => {
            for p in ps {
                guard(p)?;
                remove_path(p).map_err(|e| format!("{}: {e}", p.display()))?;
            }
            Ok(())
        }
        Plan::DeleteContents { dirs, min_age_days } => {
            for d in dirs {
                guard(d)?;
                empty_dir(d, *min_age_days);
            }
            Ok(())
        }
        Plan::Recycle(ps) => {
            for p in ps {
                guard(p)?;
            }
            trash::delete_all(ps).map_err(|e| e.to_string())
        }
        Plan::Command { cmd, fallback } => match run_command(cmd) {
            Ok(()) => Ok(()),
            Err(e) => match fallback {
                Some(fb) => execute(fb),
                None => Err(e),
            },
        },
        Plan::EmptyRecycleBin(drive) => {
            // The bin lists items from every drive; only purge the ones from this drive.
            let items: Vec<_> = trash::os_limited::list()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|i| paths::on_drive(&i.original_parent, drive))
                .collect();
            trash::os_limited::purge_all(items).map_err(|e| e.to_string())
        }
        Plan::Admin { script, .. } => admin::run_script(script),
        Plan::Dehydrate(ps) => onedrive::dehydrate(ps),
        Plan::Manual => Err("This one has to be done by hand".into()),
    }
}

fn guard(p: &Path) -> Result<(), String> {
    if paths::is_protected(p) {
        Err(format!("Refusing to touch protected folder {}", p.display()))
    } else {
        Ok(())
    }
}

fn remove_path(p: &Path) -> std::io::Result<()> {
    let meta = match fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        fs::remove_file(p).or_else(|_| fs::remove_dir(p))
    } else {
        fs::remove_dir_all(p)
    }
}

/// Remove everything inside `dir`, keeping the folder. Items in use or
/// younger than `min_age_days` are skipped without complaint.
fn empty_dir(dir: &Path, min_age_days: u32) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if min_age_days > 0 {
            let newest = size::newest_mtime(&e.path(), &[], 6, 5_000)
                .or_else(|| e.metadata().and_then(|m| m.modified()).ok());
            if newest.map(size::days_since).unwrap_or(u64::MAX) < min_age_days as u64 {
                continue;
            }
        }
        let _ = remove_path(&e.path());
    }
}

fn run_command(cmd: &str) -> Result<(), String> {
    let mut c = Command::new("cmd");
    c.args(["/C", cmd]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    match c.output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!("`{cmd}` failed: {}", String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => Err(format!("couldn't run `{cmd}`: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_only_what_the_drive_really_gained() {
        // The real case: 7.4 GB of update files removed, free space grew by 2.4 GB.
        let gb = 1024u64 * 1024 * 1024;
        assert_eq!(reconcile(7 * gb + gb * 2 / 5, 2 * gb + gb * 2 / 5), 2 * gb + gb * 2 / 5);
        assert_eq!(reconcile(gb, 5 * gb), gb, "other deletions can't inflate it");
        assert_eq!(reconcile(gb, 0), 0);
    }

    #[test]
    fn empties_contents_but_keeps_folder() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("sub/deep")).unwrap();
        fs::write(d.path().join("a.txt"), "x").unwrap();
        fs::write(d.path().join("sub/deep/b.txt"), "x").unwrap();
        empty_dir(d.path(), 0);
        assert!(d.path().exists());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn age_filter_keeps_recent_files() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("fresh.txt"), "x").unwrap();
        empty_dir(d.path(), 2);
        assert!(d.path().join("fresh.txt").exists());
    }

    #[test]
    fn refuses_protected_paths() {
        assert!(execute(&Plan::Delete(vec![PathBuf::from(r"C:\Windows")])).is_err());
        assert!(execute(&Plan::Delete(vec![PathBuf::from(r"C:\")])).is_err());
    }

    #[test]
    fn command_falls_back() {
        let d = tempfile::tempdir().unwrap();
        let junk = d.path().join("cache");
        fs::create_dir_all(&junk).unwrap();
        fs::write(junk.join("x"), "x").unwrap();
        let plan = Plan::Command {
            cmd: "definitely-not-a-real-command-xyz".into(),
            fallback: Some(Box::new(Plan::DeleteContents { dirs: vec![junk.clone()], min_age_days: 0 })),
        };
        execute(&plan).unwrap();
        assert!(junk.exists() && fs::read_dir(&junk).unwrap().count() == 0);
    }
}
