// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let _args: Vec<String> = std::env::args().collect();
    office_hub_lib::run();
}
