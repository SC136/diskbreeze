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
    let mut items: Vec<Item> = targets
        .par_iter()
        .map(|p| Item {
            path: p.to_string_lossy().into_owned(),
            bytes: size::size_of(p),
            note: None,
            open: None,
        })
        .collect();
    let bytes: u64 = items.iter().map(|i| i.bytes).sum();
    if bytes < e.min_mb * MB {
        return None;
    }
    items.retain(|i| i.bytes > 0);
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    let plan = match e.action.as_str() {
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
        for e in parse(WINDOWS).unwrap().iter().filter(|e| e.action != "manual") {
            for template in &e.paths {
                for p in paths::expand(template) {
                    assert!(!paths::is_protected(&p), "{} resolves to protected {}", e.id, p.display());
                }
            }
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
