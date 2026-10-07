//! Lists installed Steam games with their size and when they were last played.
//! Uninstalling is left to Steam (via steam://uninstall links), so this is
//! always a manual finding.

use super::{ago, GB};
use crate::model::{Finding, Item, Plan, Tier};
use crate::paths;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn detect(drive: &str) -> Option<Finding> {
    let root = paths::steam_root()?;
    let mut games = vec![];
    for lib in libraries(&root) {
        let apps = lib.join("steamapps");
        let Ok(rd) = fs::read_dir(&apps) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("appmanifest_") && name.ends_with(".acf") {
                if let Some(g) = parse_manifest(&e.path(), &apps) {
                    games.push(g);
                }
            }
        }
    }
    // Only games installed on the drive being scanned (libraries can live on any drive).
    games.retain(|g: &Item| g.bytes >= 2 * GB && paths::on_drive(Path::new(&g.path), drive));
    if games.is_empty() {
        return None;
    }
    games.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let bytes = games.iter().map(|g| g.bytes).sum();
    Some(Finding {
        id: "steam-games".into(),
        name: "Steam games".into(),
        category: "Games".into(),
        tier: Tier::Manual,
        what: "Installed games. Uninstalling frees the space, and you can reinstall any time. Games with Steam Cloud keep your saves.".into(),
        after: None,
        how: Some("Uninstall the ones you don't play anymore. Click a game to open Steam's uninstall prompt.".into()),
        open: Some("steam://open/games".into()),
        bytes,
        selectable: false,
        items: games,
        recycles: false,
        action: Plan::Manual.summary(),
        plan: Plan::Manual,
    })
}

/// Steam's own folder plus any extra libraries listed in libraryfolders.vdf.
fn libraries(root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![root.to_path_buf()];
    if let Ok(vdf) = fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")) {
        for line in vdf.lines() {
            let fields = quoted(line);
            if fields.len() == 2 && fields[0].eq_ignore_ascii_case("path") {
                libs.push(PathBuf::from(fields[1].replace(r"\\", r"\")));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    libs.retain(|p| p.is_dir() && seen.insert(p.to_string_lossy().to_lowercase()));
    libs
}

fn parse_manifest(path: &Path, apps: &Path) -> Option<Item> {
    let text = fs::read_to_string(path).ok()?;
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let f = quoted(line);
        if f.len() == 2 {
            // First occurrence wins: the top-level keys come before nested sections.
            kv.entry(f[0].to_lowercase()).or_insert_with(|| f[1].clone());
        }
    }
    let name = kv.get("name")?.clone();
    let appid = kv.get("appid")?.clone();
    let bytes: u64 = kv.get("sizeondisk").and_then(|s| s.parse().ok()).unwrap_or(0);
    let installdir = kv.get("installdir").cloned().unwrap_or_default();
    let last: u64 = kv.get("lastplayed").and_then(|s| s.parse().ok()).unwrap_or(0);
    let note = if last == 0 {
        format!("{name} · never played")
    } else {
        let t = UNIX_EPOCH + Duration::from_secs(last);
        let days = SystemTime::now().duration_since(t).map(|d| d.as_secs() / 86_400).unwrap_or(0);
        format!("{name} · last played {}", ago(days))
    };
    Some(Item {
        path: apps.join("common").join(installdir).to_string_lossy().into_owned(),
        bytes,
        note: Some(note),
        open: Some(format!("steam://uninstall/{appid}")),
    })
}

/// The quoted strings on a VDF line: `"name"  "Counter-Strike 2"` -> [name, Counter-Strike 2].
fn quoted(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut inside = false;
    let mut escaped = false;
    for c in line.chars() {
        if inside {
            if escaped {
                cur.push(c);
                escaped = false;
            } else if c == '\\' {
                cur.push(c);
                escaped = true;
            } else if c == '"' {
                out.push(std::mem::take(&mut cur));
                inside = false;
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            inside = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vdf_lines() {
        assert_eq!(quoted("\t\t\"path\"\t\t\"D:\\\\SteamLibrary\""), vec!["path", "D:\\\\SteamLibrary"]);
        assert_eq!(quoted("\"name\"  \"Counter-Strike 2\""), vec!["name", "Counter-Strike 2"]);
        assert!(quoted("{").is_empty());
    }

    #[test]
    fn reads_an_app_manifest() {
        let d = tempfile::tempdir().unwrap();
        let acf = d.path().join("appmanifest_730.acf");
        fs::write(
            &acf,
            "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n\t\"installdir\"\t\t\"Counter-Strike Global Offensive\"\n\t\"SizeOnDisk\"\t\t\"64000000000\"\n\t\"LastPlayed\"\t\t\"0\"\n}\n",
        )
        .unwrap();
        let item = parse_manifest(&acf, d.path()).unwrap();
        assert_eq!(item.bytes, 64_000_000_000);
        assert_eq!(item.open.as_deref(), Some("steam://uninstall/730"));
        assert!(item.note.unwrap().contains("never played"));
    }
}
