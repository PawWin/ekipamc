// No console window next to the launcher on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod game;
mod install;
mod net;
mod pack;
mod paths;
mod settings;
mod ui;

fn main() -> anyhow::Result<()> {
    ui::run()
}
