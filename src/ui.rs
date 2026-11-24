//! Wires the Slint window to install/launch work running on a Tokio runtime.

use crate::game::launch;
use crate::install::{self, Progress};
use crate::net::Net;
use crate::pack::PACK;
use crate::paths::Paths;
use crate::settings::{self, Settings};
use anyhow::Result;
use slint::ComponentHandle;

slint::include_modules!();

/// Shared handles the UI callbacks need. Cheap to clone into tasks.
#[derive(Clone)]
struct Launcher {
    runtime: tokio::runtime::Handle,
    net: Net,
    paths: Paths,
    window: slint::Weak<AppWindow>,
}

pub fn run() -> Result<()> {
    let pack = &*PACK;
    let paths = Paths::new(&pack.id)?;
    let saved = Settings::load(&paths.settings());
    let runtime = tokio::runtime::Runtime::new()?;

    let app = AppWindow::new()?;
    app.set_pack_name(pack.name.as_str().into());
    app.set_pack_subtitle(pack.subtitle.as_str().into());
    let modpack = pack
        .modpack
        .as_ref()
        .map(|m| format!(" · pack {}", m.version))
        .unwrap_or_default();
    app.set_pack_versions(
        format!(
            "Minecraft {} · NeoForge {}{modpack}",
            pack.minecraft, pack.neoforge
        )
        .into(),
    );
    if settings::is_valid_nick(&saved.nick) {
        app.set_nick(saved.nick.as_str().into());
    }
    app.set_min_ram_gb(settings::MIN_RAM_GB as i32);
    app.set_max_ram_gb(settings::MAX_RAM_GB as i32);
    app.set_ram_gb(
        saved
            .ram_gb
            .clamp(settings::MIN_RAM_GB, settings::MAX_RAM_GB) as i32,
    );

    let launcher = Launcher {
        runtime: runtime.handle().clone(),
        net: Net::new()?,
        paths,
        window: app.as_weak(),
    };

    app.on_nick_valid(|nick| settings::is_valid_nick(&nick));
    app.on_retry({
        let launcher = launcher.clone();
        move || launcher.start_install()
    });
    app.on_save_nick({
        let launcher = launcher.clone();
        move |nick| launcher.save_nick(nick.into())
    });
    app.on_play({
        let launcher = launcher.clone();
        move || launcher.play()
    });
    app.on_open_folder({
        let root = launcher.paths.root.clone();
        move || open_folder(&root)
    });

    launcher.start_install();
    app.run()?;
    Ok(())
}

impl Launcher {
    /// Runs `f` on the UI thread.
    fn ui(&self, f: impl FnOnce(AppWindow) + Send + 'static) {
        self.window.upgrade_in_event_loop(f).ok();
    }

    fn start_install(&self) {
        self.update("Starting", "", None);
        self.ui(|app| app.set_screen(Screen::Installing));
        let this = self.clone();
        self.runtime.spawn(async move {
            let result = install::install(&this.net, &this.paths, &PACK, &this).await;
            this.ui(move |app| match result {
                Ok(()) if app.get_nick().is_empty() => app.set_screen(Screen::Nick),
                Ok(()) => app.set_screen(Screen::Ready),
                Err(error) => show_error(&app, format!("{error:#}")),
            });
        });
    }

    fn save_nick(&self, nick: String) {
        let Some(app) = self.window.upgrade() else {
            return;
        };
        app.set_nick(nick.as_str().into());
        match self.current_settings(&app).save(&self.paths.settings()) {
            Ok(()) => app.set_screen(Screen::Ready),
            Err(error) => show_error(&app, format!("Cannot save settings: {error:#}")),
        }
    }

    fn play(&self) {
        let Some(app) = self.window.upgrade() else {
            return;
        };
        let settings = self.current_settings(&app);
        // Losing the memory choice is not worth blocking the game over.
        settings.save(&self.paths.settings()).ok();
        app.set_screen(Screen::Running);

        let this = self.clone();
        self.runtime.spawn(async move {
            let result = launch::run(&this.paths, &PACK, &settings).await;
            let logs = this.paths.logs().join("game-output.log");
            this.ui(move |app| match result {
                Ok(status) if status.success() => app.set_screen(Screen::Ready),
                Ok(status) => show_error(
                    &app,
                    format!(
                        "Minecraft closed with an error ({status}).\nSend {} to the server admin.",
                        logs.display()
                    ),
                ),
                Err(error) => show_error(&app, format!("{error:#}")),
            });
        });
    }

    fn current_settings(&self, app: &AppWindow) -> Settings {
        Settings {
            nick: app.get_nick().into(),
            ram_gb: app.get_ram_gb() as u32,
        }
    }
}

impl Progress for Launcher {
    fn update(&self, stage: &str, detail: &str, fraction: Option<f32>) {
        let (stage, detail) = (stage.to_owned(), detail.to_owned());
        self.ui(move |app| {
            app.set_stage(stage.into());
            app.set_detail(detail.into());
            app.set_fraction(fraction.unwrap_or(-1.0));
        });
    }
}

fn show_error(app: &AppWindow, message: String) {
    app.set_error_message(message.into());
    app.set_screen(Screen::Error);
}

fn open_folder(path: &std::path::Path) {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(path).spawn().ok();
}
