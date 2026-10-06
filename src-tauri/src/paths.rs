use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Expand a catalog path template into the existing paths it matches.
///
/// Supports `%ENV%` variables, `{downloads}` style known folders, and `*` / `?`
/// wildcards inside a single path segment, e.g.
/// `%LOCALAPPDATA%\Google\Chrome\User Data\*\Cache`.
pub fn expand(template: &str) -> Vec<PathBuf> {
    let Some(concrete) = substitute(template) else {
        return vec![];
    };
    glob(&concrete)
}

fn substitute(template: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find(['%', '{']) {
        out.push_str(&rest[..start]);
        let close = if rest[start..].starts_with('%') { '%' } else { '}' };
        let after = &rest[start + 1..];
        let end = after.find(close)?;
        let key = &after[..end];
        let value = if close == '%' { env::var(key).ok() } else { known_folder(key) }?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out.replace('/', "\\"))
}

fn known_folder(key: &str) -> Option<String> {
    let p = match key {
        "home" => dirs::home_dir(),
        "downloads" => dirs::download_dir(),
        "videos" => dirs::video_dir(),
        "pictures" => dirs::picture_dir(),
        "steam" => steam_root(),
        _ => None,
    }?;
    Some(p.to_string_lossy().into_owned())
}

pub fn steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") {
            if let Ok(p) = key.get_value::<String, _>("SteamPath") {
                let p = PathBuf::from(p.replace('/', "\\"));
                if p.is_dir() {
                    return Some(p);
                }
            }
        }
    }
    let fallback = PathBuf::from(env::var("ProgramFiles(x86)").ok()?).join("Steam");
    fallback.is_dir().then_some(fallback)
}

fn glob(pattern: &str) -> Vec<PathBuf> {
    let mut parts = pattern.split('\\').filter(|s| !s.is_empty());
    let Some(first) = parts.next() else { return vec![] };
    // "C:" needs its separator back to mean the drive root.
    let mut current = vec![PathBuf::from(if first.ends_with(':') { format!("{first}\\") } else { first.to_string() })];
    for part in parts {
        let mut next = vec![];
        for base in &current {
            if part.contains(['*', '?']) {
                let Ok(rd) = fs::read_dir(base) else { continue };
                for e in rd.flatten() {
                    if wildcard_match(part, &e.file_name().to_string_lossy()) {
                        next.push(e.path());
                    }
                }
            } else {
                next.push(base.join(part));
            }
        }
        current = next;
    }
    current.into_iter().filter(|p| p.exists()).collect()
}

/// Case-insensitive `*` / `?` matching for one path segment.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// Last line of defence before anything is deleted or recycled. Refuses drive
/// roots, shallow paths, Windows itself, and the user's main folders. Things
/// *inside* those folders (e.g. one ISO in Downloads) are still allowed.
pub fn is_protected(path: &Path) -> bool {
    if !path.is_absolute() {
        return true;
    }
    let depth = path.components().filter(|c| matches!(c, Component::Normal(_))).count();
    if depth < 2 {
        return true;
    }
    let lower = path.to_string_lossy().to_lowercase();
    if let Ok(windir) = env::var("WINDIR") {
        let windir = windir.to_lowercase();
        if lower == windir || lower.starts_with(&format!("{windir}\\")) {
            return true;
        }
    }
    let mut protected: Vec<PathBuf> = [
        dirs::home_dir(),
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
        dirs::picture_dir(),
        dirs::video_dir(),
        dirs::audio_dir(),
        dirs::data_dir(),
        dirs::data_local_dir(),
        env::var("OneDrive").ok().map(PathBuf::from),
        env::var("ProgramFiles").ok().map(PathBuf::from),
        env::var("ProgramFiles(x86)").ok().map(PathBuf::from),
        env::var("ProgramData").ok().map(PathBuf::from),
        env::var("USERPROFILE").ok().map(|h| PathBuf::from(h).join("AppData")),
    ]
    .into_iter()
    .flatten()
    .collect();
    protected.retain(|p| !p.as_os_str().is_empty());
    let trimmed = lower.trim_end_matches('\\');
    protected
        .iter()
        .any(|p| p.to_string_lossy().to_lowercase().trim_end_matches('\\') == trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard_match("Profile *", "Profile 19"));
        assert!(wildcard_match("*", "Default"));
        assert!(wildcard_match("appmanifest_*.acf", "APPMANIFEST_730.acf"));
        assert!(wildcard_match("setup?.exe", "setup1.exe"));
        assert!(!wildcard_match("Profile *", "Default"));
        assert!(!wildcard_match("*.iso", "ubuntu.iso.part"));
    }

    #[test]
    fn substitutes_env_and_rejects_unknown() {
        env::set_var("DD_TEST_ROOT", r"C:\tmp");
        assert_eq!(substitute(r"%DD_TEST_ROOT%\x").as_deref(), Some(r"C:\tmp\x"));
        assert_eq!(substitute(r"%DD_DOES_NOT_EXIST%\x"), None);
        assert_eq!(substitute(r"{nope}\x"), None);
    }

    #[test]
    fn glob_expands_wildcard_segments() {
        let dir = tempfile::tempdir().unwrap();
        for p in ["Default/Cache", "Profile 1/Cache", "Profile 2/Other"] {
            fs::create_dir_all(dir.path().join(p)).unwrap();
        }
        let pattern = format!(r"{}\*\Cache", dir.path().display());
        let mut found = glob(&pattern);
        found.sort();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn protects_important_folders() {
        assert!(is_protected(Path::new(r"C:\")));
        assert!(is_protected(Path::new(r"C:\Users")));
        assert!(is_protected(Path::new(r"relative\path")));
        assert!(is_protected(Path::new(r"C:\Windows\Temp")));
        if let Some(home) = dirs::home_dir() {
            assert!(is_protected(&home));
            assert!(is_protected(&home.join("AppData")));
            assert!(!is_protected(&home.join("AppData").join("Local").join("npm-cache")));
        }
        if let Some(dl) = dirs::download_dir() {
            assert!(is_protected(&dl));
            assert!(!is_protected(&dl.join("ubuntu.iso")));
        }
    }
}
