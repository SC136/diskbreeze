// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|a| a == "--scan-json") {
        println!("{}", diskbreeze_lib::scan_json());
        return;
    }
    diskbreeze_lib::run()
}
