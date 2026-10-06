pub mod downloads;
pub mod projects;
pub mod recycle_bin;
pub mod steam;

pub const MB: u64 = 1024 * 1024;
pub const GB: u64 = 1024 * MB;

pub fn ago(days: u64) -> String {
    match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        2..=59 => format!("{days} days ago"),
        60..=729 => format!("{} months ago", days / 30),
        _ => format!("{} years ago", days / 365),
    }
}
