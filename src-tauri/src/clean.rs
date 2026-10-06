use crate::model::{CleanOutcome, Finding, Plan};
use crate::{paths, size};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Clean the given findings. The Recycle Bin always goes last so files we just
/// moved there are emptied too only if the user selected it.
pub fn run(findings: &[&Finding]) -> Vec<CleanOutcome> {
    let mut ordered: Vec<&Finding> = findings.to_vec();
    ordered.sort_by_key(|f| matches!(f.plan, Plan::EmptyRecycleBin));
    ordered.iter().map(|f| clean_one(f)).collect()
}

fn clean_one(f: &Finding) -> CleanOutcome {
    let targets = targets_of(&f.plan);
    let before: u64 = targets.iter().map(|p| size::size_of(p)).sum();
    let result = execute(&f.plan);
    let after: u64 = targets.iter().map(|p| size::size_of(p)).sum();
    let freed = match &f.plan {
        Plan::EmptyRecycleBin => f.bytes,
        _ => before.saturating_sub(after),
    };
    match result {
        Ok(()) => CleanOutcome { id: f.id.clone(), ok: true, bytes: freed, recycled: f.plan.recycles(), message: None },
        Err(e) => CleanOutcome { id: f.id.clone(), ok: false, bytes: freed, recycled: false, message: Some(e) },
    }
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
        Plan::EmptyRecycleBin => {
            let items = trash::os_limited::list().map_err(|e| e.to_string())?;
            trash::os_limited::purge_all(items).map_err(|e| e.to_string())
        }
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
