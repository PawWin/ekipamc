//! Renders every launcher view headlessly to PNGs, for reviewing UI changes
//! without a display or a real install.
//!
//!     cargo run --example screenshots -- <output-dir>
//!
//! Needs ImageMagick (`magick`) to encode the PNGs.

#[path = "../src/ui/avatar.rs"]
mod avatar;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize, PlatformError};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

slint::include_modules!();

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

const NICK: &str = "Kotek_42";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "screenshots".into()),
    );
    std::fs::create_dir_all(&out)?;

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone())))?;
    window.set_size(PhysicalSize::new(1120, 700));

    let shoot = |name: &str, setup: &dyn Fn(&Store)| -> Result<(), Box<dyn std::error::Error>> {
        let app = AppWindow::new()?;
        let store = app.global::<Store>();
        fill(&store);
        setup(&store);
        app.show()?;
        save(&app, &out.join(format!("{name}.png")))?;
        app.hide()?;
        Ok(())
    };

    shoot("01-play", &|_| {})?;
    shoot("02-installing", &|store| {
        store.set_screen(Screen::Installing);
        store.set_stage("Downloading assets".into());
        store.set_detail("1873 / 3904 files".into());
        store.set_fraction(0.48);
    })?;
    shoot("03-first-start", &|store| {
        store.set_screen(Screen::Installing);
        store.set_stage("Installing NeoForge".into());
        store.set_detail("This can take a minute".into());
        set_nick(store, "");
        store.set_nick_open(true);
    })?;
    shoot("04-installations", &|store| {
        store.set_page(Page::Installations)
    })?;
    shoot("05-edit-installation", &|store| {
        store.set_page(Page::Edit);
        let mut profile = store.get_profile();
        profile.width = 1920;
        profile.height = 1080;
        profile.jvm_args = "-XX:+UseZGC".into();
        store.set_profile(profile);
    })?;
    shoot("06-settings", &|store| store.set_page(Page::Settings))?;
    shoot("07-running", &|store| store.set_screen(Screen::Running))?;
    shoot("08-error", &|store| {
        store.set_screen(Screen::Error);
        store.set_error_message(
            "Minecraft closed with an error (exit status: 1).\nSend ~/.local/share/ekipamc/logs/game-output.log to the server admin."
                .into(),
        );
        store.set_error_open(true);
    })?;
    shoot("09-change-nick", &|store| store.set_nick_open(true))?;
    Ok(())
}

/// The state a returning player sees: installed, nick set, default profile.
fn fill(store: &Store) {
    store.set_launcher_name("EkipaMC".into());
    store.set_pack_subtitle("All the Mods 10".into());
    store.set_pack_version("8.6".into());
    store.set_minecraft("1.21.1".into());
    store.set_neoforge("21.1.266".into());
    store.set_launcher_version(env!("CARGO_PKG_VERSION").into());
    store.set_game_dir("~/.local/share/ekipamc/instance".into());
    store.set_default_game_dir("~/.local/share/ekipamc/instance".into());
    store.set_min_ram_gb(4);
    store.set_max_ram_gb(16);
    store.set_screen(Screen::Ready);
    store.set_profile(Profile {
        name: "EkipaCraft".into(),
        ram_gb: 8,
        width: 0,
        height: 0,
        fullscreen: false,
        jvm_args: "".into(),
        java_path: "".into(),
        game_dir: "".into(),
    });
    store.set_options(LauncherOptions {
        keep_open: true,
        open_log: false,
    });
    store.on_nick_valid(|nick| (3..=16).contains(&nick.len()));
    set_nick(store, NICK);
}

fn set_nick(store: &Store, nick: &str) {
    store.set_nick(nick.into());
    store.set_avatar(avatar::face(nick));
}

fn save(app: &AppWindow, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let shot = app.window().take_snapshot()?;
    let raw = path.with_extension("rgba");
    std::fs::write(&raw, shot.as_bytes())?;
    let status = Command::new("magick")
        .args([
            "-size",
            &format!("{}x{}", shot.width(), shot.height()),
            "-depth",
            "8",
        ])
        .arg(format!("rgba:{}", raw.display()))
        .arg(path)
        .status()?;
    std::fs::remove_file(&raw)?;
    if !status.success() {
        return Err(format!("magick failed for {}", path.display()).into());
    }
    Ok(())
}
