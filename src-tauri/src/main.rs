// The Windows release build is a GUI application, so it must not open a console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    forever_addons_lib::run()
}
