<p align="center"><img src="dist/superhuman.png" width="128" alt=""></p>

# Superhuman

A game launcher made for the couch and a controller. It finds the games you already have, from every launcher and store, and puts them on one shelf with proper art. Pick one and play, without ever seeing a desktop.

Works on Linux. Windows and macOS builds exist but are still untested.

## Where it finds games

- **Linux:** Steam, Heroic (Epic, GOG, Amazon), Lutris, Bottles, itch, Prism Launcher (Minecraft), Flatpak and desktop games, and emulated ROMs
- **Windows:** Steam, Epic, GOG, Xbox / Microsoft Store, Ubisoft Connect, EA app, Battle.net, Amazon Games, itch, and ROMs
- **macOS:** Steam, Epic, GOG, itch, game apps in /Applications, and ROMs
- **Anything else:** add it by hand in `games.toml` (see `games.example.toml`). On Linux, Windows `.exe` games run through Proton (umu-launcher) or Wine.

## Controls

| Controller | Keyboard | Does |
|---|---|---|
| A | Enter | Play / select |
| B | Esc | Back |
| X | I | Game details |
| Y | / | Search |
| Start | M | Menu (library, search, settings, power) |
| LB / RB | Q / E or Page Up / Down | Previous / next tab |

You can also use a mouse: click to focus, click again to open, scroll, and right-click to go back. The button prompts follow whatever you used last.

## Art

Steam games get their art from Steam. For everything else, add a free [SteamGridDB](https://www.steamgriddb.com/profile/preferences/api) API key in Settings, or in `~/.config/superhuman/config.toml` (see `config.example.toml`). The key stays on your computer.

## Build

```sh
cargo build --release
./target/release/superhuman
```

On Linux you need `libxkbcommon` and `libudev` (most desktops already have them). `--fullscreen` starts full screen. `dist/superhuman-session` runs it as a console-style login session inside gamescope.

Packaging files for deb, rpm, Arch (PKGBUILD), Flatpak, AppImage, a macOS `.dmg` and winget are in `dist/packaging/`.
