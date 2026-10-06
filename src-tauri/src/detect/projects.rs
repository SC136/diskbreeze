//! Finds dependency and build folders in code projects you haven't touched in
//! a while. A folder only counts when its project has the matching marker file
//! (node_modules needs a package.json next to it, target needs Cargo.toml…), so
//! a random folder that happens to be called "build" is never touched.

use super::{ago, MB};
use crate::model::{Finding, Item, Plan, Tier};
use crate::size;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const STALE_DAYS: u64 = 60;

struct Rule {
    dir: &'static str,
    markers: &'static [&'static str],
}

const JS: &[&str] = &["package.json"];
const PY: &[&str] = &["requirements.txt", "pyproject.toml", "setup.py", "Pipfile"];
const GRADLE: &[&str] = &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"];
const DOTNET: &[&str] = &["*.csproj", "*.fsproj", "*.vbproj"];

const RULES: &[Rule] = &[
    Rule { dir: "node_modules", markers: JS },
    Rule { dir: ".next", markers: JS },
    Rule { dir: ".nuxt", markers: JS },
    Rule { dir: ".svelte-kit", markers: JS },
    Rule { dir: ".turbo", markers: JS },
    Rule { dir: ".parcel-cache", markers: JS },
    Rule { dir: ".angular", markers: JS },
    Rule { dir: "target", markers: &["Cargo.toml", "pom.xml"] },
    Rule { dir: "build", markers: &["pubspec.yaml", "build.gradle", "build.gradle.kts", "CMakeLists.txt"] },
    Rule { dir: ".dart_tool", markers: &["pubspec.yaml"] },
    Rule { dir: ".gradle", markers: GRADLE },
    Rule { dir: ".cxx", markers: &["build.gradle", "build.gradle.kts"] },
    Rule { dir: ".venv", markers: PY },
    Rule { dir: "venv", markers: PY },
    Rule { dir: "Library", markers: &["ProjectSettings"] },
    Rule { dir: "obj", markers: DOTNET },
    Rule { dir: "bin", markers: DOTNET },
];

/// Folders that don't count as "you worked on this project" when we look for
/// the newest file.
const NOT_YOUR_WORK: &[&str] = &[
    ".git", "node_modules", ".next", ".nuxt", ".svelte-kit", ".turbo", ".parcel-cache", ".angular",
    "target", "build", ".dart_tool", ".gradle", ".cxx", ".venv", "venv", "Library", "obj", "bin",
    "dist", "out", "__pycache__",
];

pub struct ProjectScan {
    pub finding: Option<Finding>,
    pub active: u32,
    pub stale: u32,
}

/// Common places people keep code, on the system drive only so the numbers
/// match the drive gauge.
pub fn default_roots() -> Vec<PathBuf> {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let mut roots: Vec<PathBuf> = ["code", "dev", "projects", "src", "repos"]
        .iter()
        .map(|d| PathBuf::from(format!("{drive}\\{d}")))
        .collect();
    if let Some(home) = dirs::home_dir() {
        for d in [
            "code", "dev", "projects", "src", "repos", r"source\repos", r"Documents\GitHub",
            r"Documents\Projects", "AndroidStudioProjects", "StudioProjects", "Desktop", r"go\src",
        ] {
            roots.push(home.join(d));
        }
    }
    roots.retain(|p| p.is_dir());
    let mut lowered: Vec<(String, PathBuf)> =
        roots.into_iter().map(|p| (p.to_string_lossy().to_lowercase(), p)).collect();
    lowered.sort();
    lowered.dedup_by(|a, b| a.0 == b.0);
    // Drop roots nested inside another root so nothing is counted twice.
    let keys: Vec<String> = lowered.iter().map(|(k, _)| k.clone()).collect();
    lowered
        .into_iter()
        .filter(|(k, _)| !keys.iter().any(|other| other != k && k.starts_with(&format!("{other}\\"))))
        .map(|(_, p)| p)
        .collect()
}

pub fn detect(roots: &[PathBuf]) -> ProjectScan {
    let found = Mutex::new(Vec::<(PathBuf, PathBuf)>::new());
    roots.par_iter().for_each(|root| find_junk(root, root, 0, &found));
    let found = found.into_inner().unwrap();

    // Age per project, computed once per project root.
    let mut by_project: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for (junk, root) in found {
        by_project.entry(project_root(&junk, &root)).or_default().push(junk);
    }
    let projects: Vec<(PathBuf, Vec<PathBuf>, u64)> = by_project
        .into_par_iter()
        .map(|(project, junk)| {
            let days = size::newest_mtime(&project, NOT_YOUR_WORK, 4, 20_000)
                .map(size::days_since)
                .unwrap_or(u64::MAX);
            (project, junk, days)
        })
        .collect();

    let active = projects.iter().filter(|p| p.2 < STALE_DAYS).count() as u32;
    let stale: Vec<_> = projects.into_iter().filter(|p| p.2 >= STALE_DAYS).collect();
    let stale_count = stale.len() as u32;

    let mut items: Vec<Item> = stale
        .par_iter()
        .flat_map(|(_, junk, days)| {
            junk.par_iter().map(move |j| Item {
                path: j.to_string_lossy().into_owned(),
                bytes: size::size_of(j),
                note: Some(format!(
                    "{} · last edited {}",
                    j.file_name().unwrap_or_default().to_string_lossy(),
                    if *days == u64::MAX { "never".into() } else { ago(*days) }
                )),
                open: None,
            })
        })
        .collect();
    items.retain(|i| i.bytes > 0);
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let bytes: u64 = items.iter().map(|i| i.bytes).sum();

    let finding = (bytes >= 50 * MB).then(|| {
        let plan = Plan::Delete(items.iter().map(|i| PathBuf::from(&i.path)).collect());
        Finding {
            id: "old-project-builds".into(),
            name: "Build files in old projects".into(),
            category: "Old project builds".into(),
            tier: Tier::Safe,
            what: format!(
                "node_modules, build output and virtual environments in {stale_count} project{} you haven't edited in {STALE_DAYS}+ days. Your code isn't touched; these folders are rebuilt from it.",
                if stale_count == 1 { "" } else { "s" }
            ),
            after: Some(
                "When you go back to a project, run its install or build step again (npm install, flutter pub get, cargo build, pip install -r requirements.txt).".into(),
            ),
            how: None,
            open: None,
            bytes,
            items,
            recycles: false,
            action: plan.summary(),
            plan,
        }
    });
    ProjectScan { finding, active, stale: stale_count }
}

fn find_junk(dir: &Path, root: &Path, depth: usize, out: &Mutex<Vec<(PathBuf, PathBuf)>>) {
    if depth > 12 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let subdirs: Vec<PathBuf> = rd
        .flatten()
        .filter_map(|e| {
            let ft = e.file_type().ok()?;
            (ft.is_dir() && !ft.is_symlink()).then(|| e.path())
        })
        .collect();
    subdirs.par_iter().for_each(|sub| {
        let name = sub.file_name().unwrap_or_default().to_string_lossy();
        if name.eq_ignore_ascii_case(".git") {
            return;
        }
        if let Some(rule) = RULES.iter().find(|r| r.dir.eq_ignore_ascii_case(&name)) {
            if has_marker(dir, rule.markers) {
                out.lock().unwrap().push((sub.clone(), root.to_path_buf()));
                return;
            }
        }
        // A dependency folder without its marker still isn't anyone's source code.
        if name.eq_ignore_ascii_case("node_modules") {
            return;
        }
        find_junk(sub, root, depth + 1, out);
    });
}

fn has_marker(dir: &Path, markers: &[&str]) -> bool {
    markers.iter().any(|m| {
        if m.contains('*') {
            fs::read_dir(dir)
                .map(|rd| rd.flatten().any(|e| crate::paths::wildcard_match(m, &e.file_name().to_string_lossy())))
                .unwrap_or(false)
        } else {
            dir.join(m).exists()
        }
    })
}

/// The folder we judge "last edited" by: the nearest git repo, otherwise the
/// folder that holds the junk.
fn project_root(junk: &Path, scan_root: &Path) -> PathBuf {
    let parent = junk.parent().unwrap_or(junk);
    let mut cur = Some(parent);
    while let Some(c) = cur {
        if c.join(".git").exists() {
            return c.to_path_buf();
        }
        if c == scan_root {
            break;
        }
        cur = c.parent();
    }
    parent.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn age(path: &Path, days: u64) {
        let t = SystemTime::now() - Duration::from_secs(days * 86_400);
        let f = fs::File::options().write(true).open(path).unwrap();
        f.set_modified(t).unwrap();
    }

    #[test]
    fn finds_only_marked_junk_in_stale_projects() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        // Old JS project: should be found.
        fs::create_dir_all(r.join("old-web/node_modules/dep")).unwrap();
        fs::write(r.join("old-web/package.json"), "{}").unwrap();
        fs::write(r.join("old-web/node_modules/dep/index.js"), vec![1u8; 4096]).unwrap();
        age(&r.join("old-web/package.json"), 200);
        // Fresh JS project: must be left alone.
        fs::create_dir_all(r.join("new-web/node_modules")).unwrap();
        fs::write(r.join("new-web/package.json"), "{}").unwrap();
        fs::write(r.join("new-web/node_modules/x.js"), vec![1u8; 4096]).unwrap();
        // "build" folder with no marker next to it: must be left alone.
        fs::create_dir_all(r.join("notes/build")).unwrap();
        fs::write(r.join("notes/build/important.txt"), vec![1u8; 4096]).unwrap();
        fs::write(r.join("notes/readme.md"), "x").unwrap();
        age(&r.join("notes/readme.md"), 400);

        let scan = detect(&[r.to_path_buf()]);
        assert_eq!(scan.stale, 1);
        assert_eq!(scan.active, 1);
        // Below the 50 MB threshold no finding is produced, so check the walk directly.
        let found = Mutex::new(vec![]);
        find_junk(r, r, 0, &found);
        let found: Vec<String> = found.into_inner().unwrap().iter().map(|(j, _)| j.display().to_string()).collect();
        assert!(found.iter().any(|p| p.ends_with("old-web\\node_modules")));
        assert!(!found.iter().any(|p| p.contains("notes")), "unmarked build folder must be ignored");
    }
}
