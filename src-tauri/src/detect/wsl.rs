//! WSL and Docker keep their data in virtual disk files (.vhdx) that grow but never shrink on
//! their own. Compacting them gives free space back to Windows. This needs administrator
//! permission and stops WSL (and with it Docker Desktop's engine) while it runs. It doesn't
//! delete anything inside the disks. How much comes back depends on how much free space is
//! inside, so this is an "up to" finding.

use crate::admin::ps_quote;
use crate::model::{Finding, Item, Plan, Tier};
use crate::{paths, size};
use std::path::PathBuf;

const GB: u64 = 1024 * 1024 * 1024;

pub fn vhdx_files() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = paths::expand(r"%LOCALAPPDATA%\Docker\wsl\*\*.vhdx");
    #[cfg(windows)]
    found.extend(distro_disks());
    found.retain(|p| p.is_file());
    found.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    found.dedup_by_key(|p| p.to_string_lossy().to_lowercase());
    found
}

/// Each installed WSL distro records where its disk lives.
#[cfg(windows)]
fn distro_disks() -> Vec<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let Ok(lxss) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Lxss") else {
        return vec![];
    };
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    lxss.enum_keys()
        .flatten()
        .filter_map(|k| lxss.open_subkey(k).ok())
        .filter_map(|d| {
            let base = d.get_value::<String, _>("BasePath").ok().map(|b| PathBuf::from(b.trim_start_matches(r"\\?\")));
            let packaged = d
                .get_value::<String, _>("PackageFamilyName")
                .ok()
                .map(|p| PathBuf::from(&local).join("Packages").join(p).join("LocalState"));
            base.or(packaged).map(|b| b.join("ext4.vhdx"))
        })
        .collect()
}

pub fn detect() -> Option<Finding> {
    let files = vhdx_files();
    let items: Vec<Item> = files
        .iter()
        .map(|p| Item { path: p.to_string_lossy().into_owned(), bytes: size::size_of(p), note: None, open: None })
        .collect();
    let bytes: u64 = items.iter().map(|i| i.bytes).sum();
    if bytes < GB {
        return None;
    }
    let script = compact_script(&files)?;
    let plan = Plan::Admin { script, label: "shrink WSL and Docker disk files".into() };
    Some(Finding {
        id: "wsl-vhdx-compact".into(),
        name: "Shrink WSL and Docker disk files".into(),
        category: "Docker & WSL".into(),
        tier: Tier::Ask,
        what: "WSL distributions and Docker Desktop keep their data in disk files that grow but never shrink by themselves. Shrinking them gives the unused space back to Windows. Nothing inside them is deleted.".into(),
        after: Some("WSL and Docker Desktop's engine are stopped while this runs, so save your work in them first. How much comes back depends on how much free space is inside each file (after a Docker prune it's usually the most); it can be small.".into()),
        how: None,
        open: None,
        bytes,
        selectable: false,
        estimate: true,
        items,
        recycles: false,
        action: plan.summary(),
        plan,
    })
}

/// The administrator script: stop WSL, then compact each disk with diskpart (read from stdin,
/// so no script file is written). Paths come from our own detection and are quoted.
pub fn compact_script(files: &[PathBuf]) -> Option<String> {
    let quoted: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        // A path with a quote or newline would break out of the diskpart line: skip it.
        .filter(|p| !p.contains(['"', '\n', '\r']))
        .map(|p| ps_quote(&p))
        .collect();
    if quoted.is_empty() {
        return None;
    }
    Some(format!(
        r#"$files = @({list})
wsl.exe --shutdown
Start-Sleep -Seconds 4
foreach ($f in $files) {{
  if (Test-Path -LiteralPath $f) {{
    "select vdisk file=`"$f`"`nattach vdisk readonly`ncompact vdisk`ndetach vdisk" | diskpart | Out-Null
  }}
}}"#,
        list = quoted.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_quotes_paths_and_skips_dangerous_ones() {
        let s = compact_script(&[PathBuf::from(r"C:\Users\O'Brien\docker_data.vhdx"), PathBuf::from("C:\\bad\"name.vhdx")]).unwrap();
        assert!(s.contains(r"'C:\Users\O''Brien\docker_data.vhdx'"));
        assert!(!s.contains("bad"), "a path with a quote must be left out");
        assert!(s.contains("wsl.exe --shutdown") && s.contains("compact vdisk"));
        assert!(compact_script(&[PathBuf::from("C:\\x\"y.vhdx")]).is_none());
    }
}
