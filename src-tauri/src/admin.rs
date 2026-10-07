//! Runs a PowerShell script as administrator: Windows shows its own permission prompt (UAC)
//! and nothing runs unless the user allows it.
//!
//! Design notes, because elevation is where mistakes become security holes:
//! - Scripts only ever come from our own catalog or detectors, never from the UI.
//! - The script is passed on the command line (`-EncodedCommand`), not written to a file, so
//!   there is no file another program could swap between "written" and "run as admin".
//! - No log file is written by the elevated process, for the same reason. The only result we
//!   get back is the exit code.

use base64::Engine;
use std::process::Command;

/// UAC "cancelled by the user" (ERROR_CANCELLED).
const CANCELLED: i32 = 1223;

/// PowerShell wants UTF-16LE, base64 encoded.
pub fn encode(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Quote a value for a PowerShell single-quoted string ('it''s').
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn run_script(script: &str) -> Result<(), String> {
    let encoded = encode(&format!("$ErrorActionPreference = 'Continue'\n{script}\nexit 0"));
    // The outer (non-elevated) PowerShell asks Windows to start the inner one as admin and waits.
    let outer = format!(
        "try {{ $p = Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand','{encoded}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; exit $p.ExitCode }} catch {{ exit {CANCELLED} }}"
    );
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &outer]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let status = cmd.status().map_err(|e| format!("couldn't ask Windows for permission: {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(CANCELLED) => Err("You didn't allow the Windows permission prompt, so nothing was changed.".into()),
        Some(c) => Err(format!("Windows reported error {c} while running it as administrator.")),
        None => Err("The administrator step was interrupted.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_as_utf16_base64() {
        // "A" in UTF-16LE is 41 00 -> "QQA="
        assert_eq!(encode("A"), "QQA=");
        let decoded = base64::engine::general_purpose::STANDARD.decode(encode("powercfg /h off")).unwrap();
        let units: Vec<u16> = decoded.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "powercfg /h off");
    }

    /// Needs a human: it shows a real Windows permission prompt.
    /// Run: cargo test elevated -- --ignored --nocapture   (click Yes)
    #[test]
    #[ignore = "shows a real Windows permission prompt; click Yes"]
    fn elevated_script_really_runs_as_administrator() {
        let marker = std::env::temp_dir().join("diskbreeze-elevation-test.txt");
        let _ = std::fs::remove_file(&marker);
        // S-1-16-12288 is the "High" integrity level, which only an elevated process has.
        let script = format!(
            "whoami /groups | Select-String 'S-1-16-12288' | Out-File -LiteralPath {} -Encoding ascii",
            ps_quote(&marker.to_string_lossy())
        );
        run_script(&script).expect("the script should run once the prompt is allowed");
        let text = std::fs::read_to_string(&marker).expect("the elevated script should have written the marker");
        assert!(text.contains("S-1-16-12288"), "the script did not run elevated");
        std::fs::remove_file(&marker).unwrap();
    }

    /// Run: cargo test declining -- --ignored --nocapture   (click No)
    #[test]
    #[ignore = "shows a real Windows permission prompt; click No"]
    fn declining_the_prompt_changes_nothing() {
        let marker = std::env::temp_dir().join("diskbreeze-declined-test.txt");
        let _ = std::fs::remove_file(&marker);
        let script = format!("'ran' | Out-File -LiteralPath {}", ps_quote(&marker.to_string_lossy()));
        let err = run_script(&script).expect_err("declining must be an error");
        assert!(err.contains("didn't allow"), "unexpected message: {err}");
        assert!(!marker.exists(), "the script must not run when the prompt is declined");
    }

    #[test]
    fn quotes_single_quotes() {
        assert_eq!(ps_quote(r"C:\Users\O'Brien\x.vhdx"), r"'C:\Users\O''Brien\x.vhdx'");
    }
}
