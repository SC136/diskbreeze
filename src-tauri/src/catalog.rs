use crate::model::{Finding, Item, Plan, Tier};
use crate::{paths, size};
use rayon::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;

pub const WINDOWS: &str = include_str!("../../catalog/windows.toml");

const MB: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct CatalogFile {
    item: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub tier: Tier,
    #[serde(default)]
    pub paths: Vec<String>,
    pub what: String,
    pub after: Option<String>,
    pub how: Option<String>,
    pub open: Option<String>,
    pub action: String,
    pub command: Option<String>,
    pub fallback: Option<String>,
    /// For action = "admin": the PowerShell script that runs after Windows' permission prompt.
    pub script: Option<String>,
    /// For action = "admin": finishes the sentence "Windows will ask for administrator permission to …".
    pub label: Option<String>,
    /// The saving can't be measured up front (e.g. the Windows component store): show "varies".
    #[serde(default)]
    pub size_unknown: bool,
    /// The listed size is an upper bound (files can share disk space): show "up to".
    #[serde(default)]
    pub estimate: bool,
    #[serde(default)]
    pub min_age_days: u32,
    #[serde(default = "default_min_mb")]
    pub min_mb: u64,
}

fn default_min_mb() -> u64 {
    50
}

pub fn parse(src: &str) -> Result<Vec<Entry>, String> {
    let file: CatalogFile = toml::from_str(src).map_err(|e| e.to_string())?;
    for e in &file.item {
        validate(e)?;
    }
    Ok(file.item)
}

fn validate(e: &Entry) -> Result<(), String> {
    let err = |msg: &str| Err(format!("catalog entry `{}`: {msg}", e.id));
    match e.action.as_str() {
        "delete" | "contents" | "recycle" | "manual" => {}
        "command" if e.command.is_none() => return err("action = \"command\" needs a `command`"),
        "command" => {}
        "admin" if e.script.is_none() || e.label.is_none() => return err("action = \"admin\" needs a `script` and a `label`"),
        "admin" if e.tier != Tier::Ask => return err("admin actions must be tier = \"ask\": the user decides, never pre-ticked"),
        "admin" => {}
        other => return err(&format!("unknown action `{other}`")),
    }
    if let Some(f) = &e.fallback {
        if !matches!(f.as_str(), "delete" | "contents") {
            return err("fallback must be \"delete\" or \"contents\"");
        }
    }
    if e.paths.is_empty() {
        return err("needs at least one path");
    }
    if e.tier == Tier::Manual && e.action != "manual" {
        return err("manual tier must use action = \"manual\"");
    }
    if e.action == "recycle" && e.tier == Tier::Safe {
        return err("recycled items are personal, so they can't be tier = \"safe\"");
    }
    Ok(())
}

fn plan_for(action: &str, targets: &[PathBuf], min_age_days: u32) -> Plan {
    match action {
        "delete" => Plan::Delete(targets.to_vec()),
        "contents" => Plan::DeleteContents { dirs: targets.to_vec(), min_age_days },
        "recycle" => Plan::Recycle(targets.to_vec()),
        _ => Plan::Manual,
    }
}

/// Resolve every catalog entry against this machine and keep the ones big
/// enough to matter.
pub fn findings(src: &str) -> Vec<Finding> {
    let entries = match parse(src) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("catalog error: {e}");
            return vec![];
        }
    };
    entries.par_iter().filter_map(resolve).collect()
}

fn resolve(e: &Entry) -> Option<Finding> {
    let mut targets: Vec<PathBuf> = e.paths.iter().flat_map(|p| paths::expand(p)).collect();
    targets.sort();
    targets.dedup_by(|a, b| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()));
    if targets.is_empty() {
        return None;
    }
    // Sizes we can't know up front (the component store counts hard links twice) are skipped
    // entirely: no walk, no size, shown as "varies".
    let (mut items, bytes): (Vec<Item>, u64) = if e.size_unknown {
        (vec![], 0)
    } else {
        let items: Vec<Item> = targets
            .par_iter()
            .map(|p| Item {
                path: p.to_string_lossy().into_owned(),
                bytes: size::size_of(p),
                note: None,
                open: None,
            })
            .collect();
        let bytes = items.iter().map(|i| i.bytes).sum();
        (items, bytes)
    };
    if !e.size_unknown && bytes < e.min_mb * MB {
        return None;
    }
    items.retain(|i| i.bytes > 0);
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    let plan = match e.action.as_str() {
        "admin" => Plan::Admin {
            script: e.script.clone().unwrap_or_default(),
            label: e.label.clone().unwrap_or_default(),
        },
        "command" => Plan::Command {
            cmd: e.command.clone().unwrap_or_default(),
            fallback: e
                .fallback
                .as_deref()
                .map(|f| Box::new(plan_for(f, &targets, e.min_age_days))),
        },
        other => plan_for(other, &targets, e.min_age_days),
    };
    Some(Finding {
        id: e.id.clone(),
        name: e.name.clone(),
        category: e.category.clone(),
        tier: e.tier,
        what: e.what.clone(),
        after: e.after.clone(),
        how: e.how.clone(),
        open: e.open.clone(),
        bytes,
        selectable: plan.itemizable() && items.len() > 1,
        estimate: e.size_unknown || e.estimate,
        items,
        recycles: plan.recycles(),
        action: plan.summary(),
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn bundled_catalog_is_valid() {
        let entries = parse(WINDOWS).expect("catalog/windows.toml must parse");
        assert!(entries.len() > 30);
        let mut ids = HashSet::new();
        for e in &entries {
            assert!(ids.insert(e.id.clone()), "duplicate id {}", e.id);
            assert!(!e.what.trim().is_empty(), "{} needs a `what`", e.id);
            if e.tier == Tier::Manual {
                assert!(e.how.is_some(), "{} is manual so it needs a `how`", e.id);
            }
        }
    }

    #[test]
    fn nothing_in_the_catalog_resolves_to_a_protected_folder() {
        // Admin entries run their own reviewed script (see the test below), not the delete path.
        for e in parse(WINDOWS).unwrap().iter().filter(|e| e.action != "manual" && e.action != "admin") {
            for template in &e.paths {
                for p in paths::expand(template) {
                    assert!(!paths::is_protected(&p), "{} resolves to protected {}", e.id, p.display());
                }
            }
        }
    }

    /// Folders that hold installed programs or user data under a misleading "cache"-like name.
    /// Emptying them destroys things people installed, so no catalog entry may point at them.
    #[test]
    fn no_entry_points_at_installed_programs() {
        const FORBIDDEN: &[&str] = &[
            r"microsoft\winget\packages", // portable winget installs live here, not an installer cache
            r"microsoft\winget\links",
            r"microsoft\windowsapps",
            r"\program files",
        ];
        for e in parse(WINDOWS).unwrap() {
            for p in &e.paths {
                let lower = p.to_lowercase();
                for bad in FORBIDDEN {
                    assert!(!lower.contains(bad), "{} points at `{p}`, which holds installed programs", e.id);
                }
            }
        }
    }

    /// Anything that runs as administrator is reviewed here. The list of allowed building blocks is
    /// deliberately tiny: if a new admin entry needs something else, this test makes you stop and look.
    #[test]
    fn admin_scripts_only_use_reviewed_commands() {
        const ALLOWED: &[&str] = &[
            "powercfg.exe /hibernate off",
            "Dism.exe /Online /Cleanup-Image /StartComponentCleanup",
        ];
        for e in parse(WINDOWS).unwrap().iter().filter(|e| e.action == "admin") {
            assert_eq!(e.tier, Tier::Ask, "{}: admin actions are never pre-ticked", e.id);
            let script = e.script.as_deref().unwrap().trim();
            let reviewed_update_cleanup = e.id == "windows-update-leftovers"
                && script.contains(r"$env:windir\SoftwareDistribution\Download")
                && !script.contains("Invoke-")
                && !script.contains("http");
            assert!(
                ALLOWED.contains(&script) || reviewed_update_cleanup,
                "{}: admin script isn't on the reviewed list: {script}",
                e.id
            );
        }
    }

    #[test]
    fn rejects_bad_entries() {
        let bad = r#"
            [[item]]
            id = "x"
            name = "x"
            category = "x"
            tier = "safe"
            paths = ['C:\x']
            what = "x"
            action = "recycle"
        "#;
        assert!(parse(bad).unwrap_err().contains("can't be tier"));
        let typo = bad.replace("action = \"recycle\"", "action = \"nuke\"");
        assert!(parse(&typo).unwrap_err().contains("unknown action"));
    }
}
