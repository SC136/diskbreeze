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
    EmptyRecycleBin,
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
            Plan::EmptyRecycleBin => "Empties the Recycle Bin permanently".into(),
            Plan::Manual => "You do this one yourself".into(),
        }
    }

    pub fn recycles(&self) -> bool {
        matches!(self, Plan::Recycle(_))
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
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
    pub items: Vec<Item>,
    pub recycles: bool,
    pub action: String,
    #[serde(skip)]
    pub plan: Plan,
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
    pub ok: bool,
    pub bytes: u64,
    pub recycled: bool,
    pub message: Option<String>,
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
