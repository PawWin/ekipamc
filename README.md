<img src="assets/icon.png" width="64" height="64" alt="">

# EkipaMC

A one-button Minecraft launcher for our yearly server, written in Rust with a Slint UI. It runs on Linux and Windows.

The pack is baked in. A player opens the launcher, waits for the first download, types a nick and hits Play. They never see a mod folder, a Java installer or a Microsoft login.

## What happens on start

1. Download the Minecraft version file from Mojang.
2. Install the Java runtime Mojang ships for that version (Java 21 for 1.21.1).
3. Run the official NeoForge installer headlessly.
4. Download libraries, the client jar and assets (about 3900 small files).
5. Download the modpack zip, check its sha256 and unpack it into `instance/`.
6. Ask for a nick the first time, then show the Play screen.

Each step skips work that's already done. After the first run, start-up takes milliseconds and works offline.

Accounts are offline-mode. The nick becomes the player name, and the UUID is the same one an offline server computes (`OfflinePlayer:<nick>`), so inventories stay tied to the nick.

## Where files go

| OS      | Data directory                     |
| ------- | ---------------------------------- |
| Linux   | `~/.local/share/ekipamc`           |
| Windows | `%APPDATA%\ekipamc`                |

The game directory (saves, options, screenshots) is `instance/` inside it. If the game crashes, the launcher tells the player to send `logs/game-output.log`.

## Setting up a season

Everything a season needs lives in [`pack.toml`](pack.toml). It's compiled into the binary, so rebuild after editing it.

### 1. Build the modpack zip

Install the ATM10 release you want once, on your own machine, with the CurseForge app or Prism Launcher. Then zip the instance folder contents so `mods/`, `config/` and friends sit at the root of the zip:

```sh
cd path/to/instance/minecraft   # the folder that contains mods/
zip -r ~/atm10-8.6.zip mods config defaultconfigs kubejs shaderpacks resourcepacks datapacks local
sha256sum ~/atm10-8.6.zip
```

Add any other folders the pack ships (look at what's in the instance besides `saves`, `logs` and `crash-reports`). A `servers.dat` with our server in it is a nice touch, since it shows up in everyone's multiplayer list.

Leave `options.txt` out. On a pack update the launcher deletes `mods/` and overwrites every file the zip contains. Files the zip doesn't contain are left alone, so players keep their settings and worlds.

### 2. Host it as a release

The zip lives in its own GitHub release on this repo. Use a tag that doesn't start with `v`, so it doesn't trigger the launcher build:

```sh
gh release create pack-8.6 ~/atm10-8.6.zip --title "Modpack ATM10 8.6" --latest=false
```

`--latest=false` keeps the launcher release as the one friends see first on the Releases page. Release assets can be up to 2 GB, and the repo has to be public so launchers can download without logging in.

### 3. Fill in `pack.toml`

```toml
neoforge = "21.1.266"   # the version in the pack's manifest.json

[modpack]
version = "8.6"
url = "https://github.com/PawWin/ekipamc/releases/download/pack-8.6/atm10-8.6.zip"
sha256 = "<from sha256sum>"
```

`cargo test` checks that the file parses and the hash looks like a sha256.

### 4. Release the launcher

Commit the filled-in `pack.toml`, then push a `v*` tag. That's the only thing that runs CI: it builds `ekipamc-linux-x86_64` and `ekipamc-windows-x86_64.exe` and attaches them to the tag's release.

```sh
git tag v1.0.0 && git push origin v1.0.0
```

Windows SmartScreen will warn about an unsigned exe. Players click "More info", then "Run anyway". On Linux they need `chmod +x` first.

## The server

Players without a Microsoft account can only join if the server runs in offline mode. Set this in `server.properties`:

```properties
online-mode=false
white-list=true
```

Offline mode means anyone can claim any nick, so keep the whitelist on and only hand out the launcher to the group.

## Development

```sh
cargo run      # the launcher
cargo test     # unit tests
```

Linux builds need the fontconfig headers (`libfontconfig1-dev` on Debian/Ubuntu). Set `XDG_DATA_HOME` to a scratch folder to test a fresh install without touching your real one.

| Path                   | Purpose                                                     |
| ---------------------- | ----------------------------------------------------------- |
| `pack.toml`            | Baked season config                                         |
| `ui/app.slint`         | The window                                                  |
| `src/ui.rs`            | Connects UI callbacks to install/launch tasks               |
| `src/install/`         | Java, Minecraft files, NeoForge installer, modpack          |
| `src/game/version.rs`  | Version JSON parsing, inheritance and OS rules              |
| `src/game/launch.rs`   | Command line, offline UUID, starting Java                   |
| `src/net.rs`           | Parallel downloads with hash checks and retries             |
