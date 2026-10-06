// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|a| a == "--scan-json") {
        println!("{}", disk_doctor_lib::scan_json());
        return;
    }
    disk_doctor_lib::run()
}
