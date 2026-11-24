//! Wires the Slint window to install/launch work running on a Tokio runtime.

mod avatar;

use crate::game::launch;
use crate::install::{self, Progress};
use crate::net::Net;
use crate::pack::{CONFIG, Pack};
use crate::paths::Paths;
use crate::settings::{self, Resolution, Settings};
use anyhow::Result;
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, VecModel};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

slint::include_modules!();

/// Shared handles the UI callbacks need. Cheap to clone into tasks.
#[derive(Clone)]
struct Launcher {
    runtime: tokio::runtime::Handle,
    net: Net,
    paths: Paths,
    /// Source of truth for what is on disk; the UI shows a copy.
    settings: Arc<Mutex<Settings>>,
    window: slint::Weak<AppWindow>,
}

pub fn run() -> Result<()> {
    let config = &*CONFIG;
    let paths = Paths::new(&config.id)?;
    // Best effort: if it fails, the default pack is just downloaded again.
    paths.move_legacy_instance(&config.default_pack().id).ok();
    let mut saved = Settings::load(&paths.settings(), config);
    if !settings::is_valid_nick(&saved.nick) {
        saved.nick.clear();
    }
    let runtime = tokio::runtime::Runtime::new()?;

    let app = AppWindow::new()?;
    let store = app.global::<Store>();
    store.set_launcher_name(config.name.as_str().into());
    store.set_launcher_version(env!("CARGO_PKG_VERSION").into());
    let packs: Vec<PackInfo> = config
        .packs
        .iter()
        .map(|pack| pack_info(pack, &paths))
        .collect();
    store.set_packs(ModelRc::new(VecModel::from(packs)));
    store.set_min_ram_gb(settings::MIN_RAM_GB as i32);
    store.set_max_ram_gb(settings::MAX_RAM_GB as i32);
    show_settings(&app, &saved, &paths);
    // First start: ask for the nick while the download runs.
    store.set_nick_open(saved.nick.is_empty());

    let launcher = Launcher {
        runtime: runtime.handle().clone(),
        net: Net::new()?,
        paths,
        settings: Arc::new(Mutex::new(saved)),
        window: app.as_weak(),
    };

    store.on_nick_valid(|nick| settings::is_valid_nick(&nick));
    store.on_save_nick({
        let launcher = launcher.clone();
        move |nick| launcher.update_settings(|settings| settings.nick = nick.into())
    });
    store.on_save_profile({
        let launcher = launcher.clone();
        move |index, profile| launcher.save_profile(index as usize, profile.into())
    });
    store.on_create_profile({
        let launcher = launcher.clone();
        move |profile| launcher.create_profile(profile.into())
    });
    store.on_delete_profile({
        let launcher = launcher.clone();
        move |index| launcher.delete_profile(index as usize)
    });
    store.on_select_profile({
        let launcher = launcher.clone();
        move |index| launcher.select_profile(index as usize)
    });
    store.on_save_options({
        let launcher = launcher.clone();
        move |options| launcher.update_settings(|settings| settings.launcher = options.into())
    });
    store.on_retry({
        let launcher = launcher.clone();
        move || launcher.start_install()
    });
    store.on_play({
        let launcher = launcher.clone();
        move || launcher.play()
    });
    store.on_open_game_dir({
        let launcher = launcher.clone();
        move |index| {
            let settings = launcher.settings.lock().unwrap();
            if let Some(profile) = settings.profiles.get(index as usize) {
                let dir = launcher.paths.game_dir(profile);
                std::fs::create_dir_all(&dir).ok();
                open_path(&dir)
            }
        }
    });
    store.on_open_logs_dir({
        let dir = launcher.paths.logs();
        move || {
            std::fs::create_dir_all(&dir).ok();
            open_path(&dir)
        }
    });

    launcher.start_install();
    app.show()?;
    // The window hides while the game runs (if the player wants that), so
    // the event loop must outlive it; only an explicit close ends it.
    app.window().on_close_requested(|| {
        slint::quit_event_loop().ok();
        CloseRequestResponse::HideWindow
    });
    slint::run_event_loop_until_quit()?;
    Ok(())
}

impl Launcher {
    /// Runs `f` on the UI thread.
    fn ui(&self, f: impl FnOnce(AppWindow) + Send + 'static) {
        self.window.upgrade_in_event_loop(f).ok();
    }

    /// Installs the selected installation's pack. Callers make sure no
    /// install or game is running; the UI only allows switching when idle.
    fn start_install(&self) {
        let (pack, game_dir) = self.selected();
        self.update("Starting", "", None);
        self.ui(|app| app.global::<Store>().set_screen(Screen::Installing));
        let this = self.clone();
        self.runtime.spawn(async move {
            let result = install::install(&this.net, &this.paths, pack, &game_dir, &this).await;
            this.ui(move |app| match result {
                Ok(()) => app.global::<Store>().set_screen(Screen::Ready),
                Err(error) => show_error(&app, format!("{error:#}")),
            });
        });
    }

    /// Applies `change`, writes settings.toml and refreshes the UI.
    fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        let Some(app) = self.window.upgrade() else {
            return;
        };
        let mut settings = self.settings.lock().unwrap();
        change(&mut settings);
        show_settings(&app, &settings, &self.paths);
        if let Err(error) = settings.save(&self.paths.settings()) {
            show_error(&app, format!("Cannot save settings: {error:#}"));
        }
    }

    /// Saves the installation at `index`. When it is the selected one, a
    /// new game directory gets the modpack installed into it right away.
    fn save_profile(&self, index: usize, profile: settings::Profile) {
        let reinstall = {
            let settings = self.settings.lock().unwrap();
            let Some(old) = settings.profiles.get(index) else {
                return;
            };
            index == settings.selected && old.game_dir != profile.game_dir
        };
        self.update_settings(|settings| settings.profiles[index] = profile);
        if reinstall {
            self.start_install();
        }
    }

    /// Adds an installation and selects it, which downloads its pack.
    fn create_profile(&self, profile: settings::Profile) {
        self.update_settings(|settings| {
            settings.profiles.push(profile);
            settings.selected = settings.profiles.len() - 1;
        });
        self.start_install();
    }

    /// Removes the installation's entry; its files stay on disk.
    fn delete_profile(&self, index: usize) {
        let was_selected = {
            let settings = self.settings.lock().unwrap();
            if settings.profiles.len() < 2 || index >= settings.profiles.len() {
                return;
            }
            index == settings.selected
        };
        self.update_settings(|settings| {
            settings.profiles.remove(index);
            if was_selected {
                settings.selected = 0;
            } else if index < settings.selected {
                settings.selected -= 1;
            }
        });
        if was_selected {
            self.start_install();
        }
    }

    /// Makes Play start the installation at `index`, installing its pack
    /// first if needed (a no-op check when it is already there).
    fn select_profile(&self, index: usize) {
        if index >= self.settings.lock().unwrap().profiles.len() {
            return;
        }
        self.update_settings(|settings| settings.selected = index);
        self.start_install();
    }

    /// The selected installation's pack and game directory.
    fn selected(&self) -> (&'static Pack, PathBuf) {
        let settings = self.settings.lock().unwrap();
        let profile = settings.profile();
        let pack = CONFIG
            .pack(&profile.pack)
            .expect("Settings::load drops profiles of unknown packs");
        (pack, self.paths.game_dir(profile))
    }

    fn play(&self) {
        let settings = self.settings.lock().unwrap().clone();
        let (pack, _) = self.selected();
        self.ui(|app| app.global::<Store>().set_screen(Screen::Running));

        let this = self.clone();
        self.runtime.spawn(async move {
            let result = match launch::start(&this.paths, pack, settings.profile(), &settings.nick)
            {
                Ok(mut child) => {
                    if settings.launcher.open_log {
                        open_path(&this.log_file());
                    }
                    if !settings.launcher.keep_open {
                        this.ui(|app| {
                            app.hide().ok();
                        });
                    }
                    child.wait().await.map_err(anyhow::Error::from)
                }
                Err(error) => Err(error),
            };
            let log = this.log_file();
            this.ui(move |app| {
                app.show().ok();
                match result {
                    Ok(status) if status.success() => {
                        app.global::<Store>().set_screen(Screen::Ready)
                    }
                    Ok(status) => {
                        let file = log.file_name().unwrap_or_default().display();
                        show_error(
                            &app,
                            "Send this log from the logs folder to the server admin:".into(),
                        );
                        let store = app.global::<Store>();
                        store.set_error_file(file.to_string().into());
                        store.set_error_detail(format!("Minecraft stopped ({status}).").into());
                    }
                    Err(error) => show_error(&app, format!("{error:#}")),
                }
            });
        });
    }

    fn log_file(&self) -> PathBuf {
        self.paths.logs().join("game-output.log")
    }
}

impl Progress for Launcher {
    fn update(&self, stage: &str, detail: &str, fraction: Option<f32>) {
        let (stage, detail) = (stage.to_owned(), detail.to_owned());
        self.ui(move |app| {
            let store = app.global::<Store>();
            store.set_stage(stage.into());
            store.set_detail(detail.into());
            store.set_fraction(fraction.unwrap_or(-1.0));
        });
    }
}

/// Copies saved settings into the window.
fn show_settings(app: &AppWindow, settings: &Settings, paths: &Paths) {
    let store = app.global::<Store>();
    store.set_game_dir(short_path(&paths.game_dir(settings.profile())).into());
    store.set_nick(settings.nick.as_str().into());
    store.set_avatar(avatar::face(&settings.nick));
    let profiles: Vec<Profile> = settings.profiles.iter().map(Profile::from).collect();
    store.set_profiles(ModelRc::new(VecModel::from(profiles)));
    store.set_selected(settings.selected as i32);
    store.set_profile(settings.profile().into());
    store.set_options(LauncherOptions {
        keep_open: settings.launcher.keep_open,
        open_log: settings.launcher.open_log,
    });
}

/// Opens the error dialog. A crash additionally sets `error-file`.
fn show_error(app: &AppWindow, message: String) {
    let store = app.global::<Store>();
    store.set_error_message(message.into());
    store.set_error_file("".into());
    store.set_error_detail("".into());
    store.set_screen(Screen::Error);
    store.set_error_open(true);
}

/// `path` with the home directory written as `~` on Unix, so it fits on screen.
fn short_path(path: &Path) -> String {
    let home = dirs::home_dir().filter(|_| cfg!(unix));
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn pack_info(pack: &Pack, paths: &Paths) -> PackInfo {
    PackInfo {
        name: pack.name.as_str().into(),
        label: pack.label().into(),
        version: pack
            .modpack
            .as_ref()
            .map_or("", |modpack| modpack.version.as_str())
            .into(),
        minecraft: pack.minecraft.as_str().into(),
        neoforge: pack.neoforge.as_str().into(),
        game_dir: short_path(&paths.default_game_dir(&pack.id)).into(),
    }
}

/// Opens a folder or file with the system's default app.
fn open_path(path: &Path) {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(path).spawn().ok();
}

impl From<&settings::Profile> for Profile {
    fn from(profile: &settings::Profile) -> Self {
        Self {
            name: profile.name.as_str().into(),
            // Settings::load keeps only profiles of known packs.
            pack: CONFIG
                .packs
                .iter()
                .position(|pack| pack.id == profile.pack)
                .unwrap_or_default() as i32,
            ram_gb: profile.ram_gb as i32,
            width: profile.resolution.map_or(0, |r| r.width as i32),
            height: profile.resolution.map_or(0, |r| r.height as i32),
            fullscreen: profile.fullscreen,
            jvm_args: profile.jvm_args.as_str().into(),
            java_path: path_text(profile.java_path.as_deref()).into(),
            game_dir: path_text(profile.game_dir.as_deref()).into(),
        }
    }
}

impl From<Profile> for settings::Profile {
    fn from(profile: Profile) -> Self {
        let pack = CONFIG
            .packs
            .get(profile.pack.max(0) as usize)
            .unwrap_or_else(|| CONFIG.default_pack());
        Self {
            name: profile.name.trim().into(),
            pack: pack.id.clone(),
            ram_gb: (profile.ram_gb.max(0) as u32)
                .clamp(settings::MIN_RAM_GB, settings::MAX_RAM_GB),
            fullscreen: profile.fullscreen,
            jvm_args: profile.jvm_args.trim().into(),
            java_path: typed_path(&profile.java_path),
            game_dir: typed_path(&profile.game_dir),
            resolution: (profile.width > 0 && profile.height > 0).then_some(Resolution {
                width: profile.width as u32,
                height: profile.height as u32,
            }),
        }
    }
}

impl From<LauncherOptions> for settings::LauncherOptions {
    fn from(options: LauncherOptions) -> Self {
        Self {
            keep_open: options.keep_open,
            open_log: options.open_log,
        }
    }
}

fn path_text(path: Option<&Path>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_default()
}

/// A path typed into a field: empty means unset, `~/` means home.
fn typed_path(text: &str) -> Option<PathBuf> {
    let text = text.trim();
    match (text.strip_prefix("~/"), dirs::home_dir()) {
        _ if text.is_empty() => None,
        (Some(rest), Some(home)) => Some(home.join(rest)),
        _ => Some(text.into()),
    }
}
