//! Measures the current user's Recycle Bin on one drive by reading the small
//! `$I…` record Windows writes for every deleted item (it stores the original
//! size, so we never have to walk the deleted files themselves).

use super::MB;
use crate::model::{Finding, Plan, Tier};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

pub fn detect(drive: &str) -> Option<Finding> {
    let bin = PathBuf::from(format!("{drive}\\$Recycle.Bin"));
    let (mut bytes, mut count) = (0u64, 0u32);
    // Other users' folders aren't readable, so this only ever counts ours.
    for user_dir in fs::read_dir(&bin).ok()?.flatten() {
        let Ok(rd) = fs::read_dir(user_dir.path()) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with("$I") {
                continue;
            }
            let mut header = [0u8; 16];
            if fs::File::open(e.path()).and_then(|mut f| f.read_exact(&mut header)).is_ok() {
                bytes += u64::from_le_bytes(header[8..16].try_into().unwrap());
                count += 1;
            }
        }
    }
    if bytes < 50 * MB {
        return None;
    }
    let plan = Plan::EmptyRecycleBin(drive.to_uppercase());
    Some(Finding {
        id: "recycle-bin".into(),
        name: format!("Recycle Bin ({})", drive.to_uppercase()),
        category: "Recycle Bin".into(),
        tier: Tier::Ask,
        what: format!("{count} files and folders you deleted earlier from this drive. They still use space until the bin is emptied."),
        after: Some("They're gone for good. Open the bin first if you might want something back.".into()),
        how: None,
        open: Some("shell:RecycleBinFolder".into()),
        bytes,
        selectable: false,
        items: vec![],
        recycles: false,
        action: plan.summary(),
        plan,
    })
}
