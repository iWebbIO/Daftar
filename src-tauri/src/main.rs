// No console window behind the app on Windows; the phone does not care.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    daftar_lib::run()
}
