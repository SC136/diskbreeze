//! Docker: asks a running Docker for what it could reclaim (`docker system df`) and offers the
//! matching prune commands. Pruning frees space *inside* Docker's disk file; Windows only gets
//! it back after that file is shrunk (see `wsl.rs`), so every finding here is an "up to".
//! Volumes are never touched: they hold people's data.

use super::MB;
use crate::model::{Finding, Plan, Tier};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Findings whose cleaning frees space inside Docker rather than on the Windows drive.
pub const NO_HOST_SAVINGS: &[&str] = &["docker-images", "docker-containers", "docker-build-cache"];

pub fn detect() -> Vec<Finding> {
    let Some(out) = run_docker(&["system", "df", "--format", "{{json .}}"]) else { return vec![] };
    let rows = parse_df(&out);
    let reclaim = |kind: &str| rows.iter().find(|(k, _)| k.eq_ignore_ascii_case(kind)).map(|r| r.1).unwrap_or(0);

    let mut out = vec![];
    let mut add = |id: &str, name: &str, tier: Tier, bytes: u64, what: &str, after: &str, cmd: &str| {
        if bytes < 100 * MB {
            return;
        }
        let plan = Plan::Command { cmd: cmd.into(), fallback: None };
        out.push(Finding {
            id: id.into(),
            name: name.into(),
            category: "Docker".into(),
            tier,
            what: format!("{what} This frees space inside Docker; to get it back on Windows, also shrink Docker's disk file (see \"Shrink WSL and Docker disk files\")."),
            after: Some(after.into()),
            how: None,
            open: None,
            bytes,
            selectable: false,
            estimate: true,
            items: vec![],
            recycles: false,
            action: plan.summary(),
            plan,
        });
    };
    add(
        "docker-build-cache",
        "Docker build cache",
        Tier::Safe,
        reclaim("Build Cache"),
        "Layers Docker kept to make image builds faster.",
        "The next image build is slower while it rebuilds the layers.",
        "docker builder prune -a -f",
    );
    add(
        "docker-images",
        "Docker images no container uses",
        Tier::Ask,
        reclaim("Images"),
        "Images that no container is using right now, including ones you pulled and may want again.",
        "Pulled again from the internet next time you need them.",
        "docker image prune -a -f",
    );
    add(
        "docker-containers",
        "Stopped Docker containers",
        Tier::Ask,
        reclaim("Containers"),
        "Containers that aren't running. Data stored inside a stopped container is lost; volumes are never touched.",
        "Anything saved only inside those containers is gone. Start a container first if you need what's in it.",
        "docker container prune -f",
    );
    out
}

/// Run docker with a time limit: if the Docker engine isn't running the CLI can hang for a while.
fn run_docker(args: &[&str]) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        let mut cmd = Command::new("docker");
        cmd.args(&args).stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let _ = tx.send(cmd.output());
    });
    match rx.recv_timeout(Duration::from_secs(8)) {
        Ok(Ok(o)) if o.status.success() => Some(String::from_utf8_lossy(&o.stdout).into_owned()),
        _ => None,
    }
}

/// `docker system df --format "{{json .}}"` prints one JSON object per line.
pub fn parse_df(output: &str) -> Vec<(String, u64)> {
    output
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .filter_map(|v| {
            let kind = v.get("Type")?.as_str()?.to_string();
            let rec = v.get("Reclaimable")?.as_str()?;
            Some((kind, parse_size(rec)))
        })
        .collect()
}

/// "3.2GB (45%)" -> bytes. Docker uses decimal units.
pub fn parse_size(s: &str) -> u64 {
    let s = s.split('(').next().unwrap_or("").trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.parse().unwrap_or(0.0);
    let mult = match unit.trim().to_lowercase().as_str() {
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => 1.0,
    };
    (n * mult) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("3.2GB (45%)"), 3_200_000_000);
        assert_eq!(parse_size("0B"), 0);
        assert_eq!(parse_size("512MB"), 512_000_000);
        assert_eq!(parse_size("1.5kB"), 1_500);
        assert_eq!(parse_size("nonsense"), 0);
    }

    #[test]
    fn parses_df_output() {
        let sample = r#"{"Active":"2","Reclaimable":"1.2GB (35%)","Size":"3.4GB","TotalCount":"5","Type":"Images"}
{"Active":"0","Reclaimable":"0B","Size":"0B","TotalCount":"0","Type":"Containers"}
{"Active":"1","Reclaimable":"2.5GB","Size":"4GB","TotalCount":"9","Type":"Build Cache"}"#;
        let rows = parse_df(sample);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], ("Images".to_string(), 1_200_000_000));
        assert_eq!(rows[2], ("Build Cache".to_string(), 2_500_000_000));
    }
}
