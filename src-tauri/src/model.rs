use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How much the user needs to think before cleaning something.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Regenerates by itself (caches, build output). Pre-selected.
    Safe,
    /// The user's own stuff or a big re-download. Never pre-selected.
    Ask,
    /// Needs admin rights or another app (Steam, Settings). We only explain.
    Manual,
}

/// What actually happens when a finding is cleaned. Never sent to the UI,
/// so the frontend can't ask us to delete arbitrary paths.
#[derive(Debug, Clone)]
pub enum Plan {
    /// Remove these files/folders entirely.
    Delete(Vec<PathBuf>),
    /// Empty these folders but keep the folders themselves.
    DeleteContents { dirs: Vec<PathBuf>, min_age_days: u32 },
    /// Move to the Recycle Bin (used for anything personal).
    Recycle(Vec<PathBuf>),
    /// Run the tool's own clean command; fall back to another plan if it fails.
    Command { cmd: String, fallback: Option<Box<Plan>> },
    /// Empty the Recycle Bin items that came from this drive (e.g. "D:"), and only that drive.
    EmptyRecycleBin(String),
    /// Run a PowerShell script as administrator (Windows shows its permission prompt). The
    /// script always comes from our own catalog or detectors, never from the UI.
    Admin { script: String, label: String },
    /// Make OneDrive files online-only ("free up space"): the cloud copy stays, the local copy goes.
    Dehydrate(Vec<PathBuf>),
    Manual,
}

impl Plan {
    pub fn summary(&self) -> String {
        match self {
            Plan::Delete(p) => format!("Deletes {} folder{}", p.len(), plural(p.len())),
            Plan::DeleteContents { dirs, min_age_days: 0 } => {
                format!("Empties {} folder{}", dirs.len(), plural(dirs.len()))
            }
            Plan::DeleteContents { min_age_days, .. } => {
                format!("Deletes files older than {min_age_days} days; skips anything in use")
            }
            Plan::Recycle(p) => format!(
                "Moves {} item{} to the Recycle Bin",
                p.len(),
                plural(p.len())
            ),
            Plan::Command { cmd, .. } => format!("Runs `{cmd}`"),
            Plan::EmptyRecycleBin(d) => format!("Empties the Recycle Bin on {d} permanently"),
            Plan::Admin { label, .. } => format!("Windows will ask for administrator permission to {label}"),
            Plan::Dehydrate(p) => format!(
                "Makes {} file{} online-only; the copy in OneDrive stays",
                p.len(),
                plural(p.len())
            ),
            Plan::Manual => "You do this one yourself".into(),
        }
    }

    pub fn recycles(&self) -> bool {
        matches!(self, Plan::Recycle(_))
    }

    /// True when the plan acts on a list of paths, so the user can pick which ones.
    pub fn itemizable(&self) -> bool {
        matches!(self, Plan::Delete(_) | Plan::Recycle(_) | Plan::Dehydrate(_) | Plan::DeleteContents { .. })
    }

    /// Keep only the paths in `keep` (lowercased path strings). Other plans are unchanged.
    pub fn restrict(&self, keep: &std::collections::HashSet<String>) -> Plan {
        let pick = |v: &Vec<PathBuf>| -> Vec<PathBuf> {
            v.iter().filter(|p| keep.contains(&p.to_string_lossy().to_lowercase())).cloned().collect()
        };
        match self {
            Plan::Delete(v) => Plan::Delete(pick(v)),
            Plan::Recycle(v) => Plan::Recycle(pick(v)),
            Plan::Dehydrate(v) => Plan::Dehydrate(pick(v)),
            Plan::DeleteContents { dirs, min_age_days } => {
                Plan::DeleteContents { dirs: pick(dirs), min_age_days: *min_age_days }
            }
            other => other.clone(),
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn restrict_keeps_only_chosen_paths() {
        let plan = Plan::Recycle(vec![PathBuf::from(r"C:\a\One.iso"), PathBuf::from(r"C:\a\two.iso")]);
        let keep: HashSet<String> = [r"c:\a\one.iso".to_string()].into();
        let Plan::Recycle(left) = plan.restrict(&keep) else { panic!("plan kind changed") };
        assert_eq!(left, vec![PathBuf::from(r"C:\a\One.iso")]);
        assert!(plan.itemizable());
        assert!(!Plan::EmptyRecycleBin("C:".into()).itemizable());
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub path: String,
    pub bytes: u64,
    pub note: Option<String>,
    /// Optional link for this item, e.g. steam://uninstall/271590
    pub open: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: String,
    pub name: String,
    pub category: String,
    pub tier: Tier,
    pub what: String,
    pub after: Option<String>,
    pub how: Option<String>,
    pub open: Option<String>,
    pub bytes: u64,
    /// The user can tick individual items instead of all-or-nothing.
    pub selectable: bool,
    /// `bytes` is an upper bound or unknown (0), so the UI shows "up to" / "varies" and keeps it
    /// out of the "can be cleaned" total.
    pub estimate: bool,
    pub items: Vec<Item>,
    pub recycles: bool,
    pub action: String,
    #[serde(skip)]
    pub plan: Plan,
}

/// What the UI asks to clean: a finding id, and optionally only some of its items.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Selection {
    pub id: String,
    pub paths: Option<Vec<String>>,
}

impl Finding {
    /// Paths this finding owns, used to validate "reveal in Explorer" requests.
    pub fn owns(&self, path: &str) -> bool {
        self.items.iter().any(|i| i.path.eq_ignore_ascii_case(path))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiskInfo {
    pub mount: String,
    pub total: u64,
    pub free: u64,
}

/// A drive the user can choose to scan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    /// "C:"
    pub letter: String,
    pub label: String,
    pub total: u64,
    pub free: u64,
    pub removable: bool,
    /// Holds the user's profile, so app caches and Downloads live here.
    pub has_profile: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub disk: DiskInfo,
    pub findings: Vec<Finding>,
    pub active_projects: u32,
    pub stale_projects: u32,
    pub took_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanOutcome {
    pub id: String,
    pub name: String,
    pub ok: bool,
    pub bytes: u64,
    pub recycled: bool,
    pub message: Option<String>,
    /// What was touched. Goes to the local history log, not to the UI.
    #[serde(skip)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub before: DiskInfo,
    pub after: DiskInfo,
    pub outcomes: Vec<CleanOutcome>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub detail: String,
}
