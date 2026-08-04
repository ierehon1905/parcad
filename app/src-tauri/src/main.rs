// Keep the console window off on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    parcad_app_lib::run()
}
