// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Handy for testing and bug reports: `diskbreeze --scan-json [D:]` and `diskbreeze --list-drives`.
    if let Some(i) = args.iter().position(|a| a == "--scan-json") {
        println!("{}", diskbreeze_lib::scan_json(args.get(i + 1).map(String::as_str)));
        return;
    }
    if args.iter().any(|a| a == "--list-drives") {
        println!("{}", diskbreeze_lib::drives_json());
        return;
    }
    diskbreeze_lib::run()
}
