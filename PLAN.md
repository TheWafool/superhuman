# Superhuman Frontend — plan

A controller-first, couch-distance game launcher for every way you play on Linux
(and later Windows/macOS). One library, correct art, real metadata, and a launch
flow that never shows you a desktop.

---

## 1. Product goals

1. **Every game, one shelf.** Steam, Lutris, Heroic (Epic/GOG/Amazon), Bottles,
   Flatpak, native `.desktop` games, itch, Prism/Minecraft, emulated ROMs, and
   any manually added Wine/Proton exe.
2. **Art that looks right.** Portrait cover, wide hero, transparent logo and icon
   for every title — never a stretched icon or a blank tile.
3. **Seamless.** Cold start to interactive library < 400 ms from cache. Launching
   hands over to the game with no flashes; quitting the game returns you to
   exactly where you were.
4. **Controller is the primary input.** Everything reachable with a pad; mouse and
   keyboard work but are never required. No hidden desktop-only settings.

Non-goals (v1): a store, friends/chat, cloud saves of our own, achievements sync.

---

## 2. Tech stack

| Concern | Choice | Why |
|---|---|---|
| UI | **iced 0.14** (wgpu renderer) + custom widgets | Pure Rust, GPU, Elm-style state fits focus navigation; async tasks built in |
| Gamepad | **gilrs** | Xbox/PS/Switch/Steam Deck mappings, hotplug, rumble |
| Async / IO | **tokio**, **reqwest** (rustls) | Art + metadata fetches, all off the UI thread |
| Storage | **rusqlite** (bundled) | Library, metadata, playtime; single file in `$XDG_DATA_HOME` |
| Image pipeline | **image** + **fast_image_resize**, decoded to GPU textures | Pre-scaled to display size, cached on disk as WebP |
| Parsing | `keyvalues-parser` (Steam VDF), `serde_json`, `freedesktop-desktop-entry` | Source scanners |
| Process tracking | `systemd-run --user --scope` (Linux), job objects (Windows) | Reliable “is the game still running” + playtime even for launcher chains |
| Config | `serde` + TOML at `$XDG_CONFIG_HOME/superhuman/config.toml` | Data-driven, human-editable |

Alternative considered: Slint (nice declarative animations, weaker async/image
story). Bevy (great rendering, too much engine for a launcher). Revisit only if
iced animation proves limiting — the custom widget layer isolates that risk.

---

## 3. Architecture

```
crates/ (cargo workspace once past M1)
  core/        Game model, IDs, Store trait, SQLite schema, config
  sources/     One module per source, each implements `Source`
  art/         SteamGridDB + Steam CDN + local, cache, palette extraction
  meta/        IGDB, Steam store, ProtonDB, PCGamingWiki, HLTB
  launch/      Launch, track process tree, playtime, return-to-UI
  input/       gilrs → semantic Actions, repeat/accel, glyph sets
  ui/          iced app, screens, focus engine, widgets, theme
  app/         binary: wiring, session mode, CLI flags
```

### Core model
```rust
struct Game {
    id: GameId,              // stable hash of (source, source_id)
    source: SourceKind,      // Steam, Lutris, Heroic(Epic|Gog|Amazon|Sideload), Flatpak, ...
    source_id: String,       // appid, lutris slug, flatpak ref, rom path...
    title: String,
    sort_title: String,
    installed: bool,
    launch: LaunchSpec,      // how to start it (see §5)
    links: ExternalIds,      // steam_appid, igdb_id, sgdb_id, gog_id — used for art/meta
    playtime: Duration, last_played: Option<DateTime>,
    favorite: bool, hidden: bool, tags: Vec<String>,
}

trait Source {
    fn kind(&self) -> SourceKind;
    fn detect(&self) -> bool;                        // is this launcher present?
    fn scan(&self) -> Result<Vec<DiscoveredGame>>;   // fast, local only
    fn watch_paths(&self) -> Vec<PathBuf>;           // for inotify rescans
}
```

**Dedup:** the same game from two sources (Steam + Heroic sideload) merges into one
card with a source picker on the game page. Match by external IDs first, then
normalized title + year.

### Startup sequence (the “seamless” part)
1. Open SQLite, render the last library snapshot immediately (art from disk cache).
2. Background: run every `Source::scan` in parallel, diff against DB, animate in
   new/removed games — no blocking spinner.
3. Background: queue missing art/metadata, highest priority = what’s on screen.
4. inotify on each source’s `watch_paths` keeps it live (install a game in Steam,
   it appears).

---

## 4. Sources

| Source | Discover | Launch | Notes |
|---|---|---|---|
| Steam (native + Flatpak) | `libraryfolders.vdf`, `appmanifest_*.acf`; non-Steam via `userdata/*/config/shortcuts.vdf` | `steam -silent steam://rungameid/<id>` | Free appid → perfect art from Steam CDN. Filter tools/Proton/runtimes by appid list |
| Lutris | `~/.local/share/lutris/pga.db` (sqlite) + `games/*.yml` | `lutris lutris:rungameid/<id>` | Also yields Wine prefix and runner |
| Heroic | `~/.config/heroic/` — `legendaryConfig/legendary/installed.json`, `gog_store/installed.json`, `nile_config/…`, `sideload_apps/library.json` (+ Flatpak path) | `heroic://launch/<runner>/<appName>` | Use `--no-gui` style flags where available to avoid the window popping |
| Bottles | `bottles-cli list programs` / bottle `bottle.yml` | `bottles-cli run -b <bottle> -p <program>` | |
| Flatpak apps | `flatpak list --app --columns=…`, read exported `.desktop` for `Categories=Game` | `flatpak run <ref>` | |
| Native .desktop | XDG dirs, `Categories=Game` | `Exec=` with field codes stripped | Skip launchers themselves (Steam, Lutris…) |
| itch | `~/.config/itch/db/butler.db` | butler launch / direct exe | |
| Prism Launcher | `instances/*/instance.cfg` | `prismlauncher -l <instance>` | Each instance = a card |
| Emulation | user ROM folders → system by extension/folder, RetroArch cores or standalone emus (config-driven table) | template in `emulators.toml` | Art via SteamGridDB + libretro-thumbnails |
| Manual | user adds an exe / script | native, Wine, or Proton (umu-launcher) | Picks prefix + runner on the pad |
| Windows (M9) | Steam (registry `SteamPath`), Epic (`ProgramData\\Epic\\…\\Manifests\\*.item`), GOG Galaxy (`galaxy-2.0.db`), Xbox / Game Pass (`Get-AppxPackage` + `MicrosoftGame.config`), EA app, Ubisoft Connect, Battle.net, Amazon Games, itch, Heroic, Prism, emulators | protocol URIs (`steam://`, `com.epicgames.launcher://`, `goggalaxy://`, `uplay://`…) or the exe | Same `Source` trait, `cfg(windows)` |
| macOS (M9) | Steam (`~/Library/Application Support/Steam`), `/Applications` with `LSApplicationCategoryType` = games, Epic, GOG Galaxy, Heroic, Battle.net, CrossOver bottles, Prism, emulators | `open -a` / protocol URIs | `cfg(target_os = "macos")` |

Every scanner's paths come from a per-platform table, so one source (Steam, Heroic,
Prism, itch, emulators) works on all three systems with only its paths changing.

Emulator definitions live in `data/emulators.toml` (built in; a user `emulators.toml`
next to `config.toml` extends it). Hand-added games live in `games.toml` (see
`games.example.toml`); `.exe` games on Linux run through umu-launcher or Wine.

---

## 5. Launching

```rust
enum LaunchSpec {
    Uri(String),                              // steam://, heroic://, lutris:
    Command { program, args, env, cwd },
    Wine { exe, prefix, runner: WineRunner }, // wine, proton via umu, GE
    Flatpak { app_ref, args },
}
```
- Every launch goes through `systemd-run --user --scope --unit=superhuman-<id>`
  so the whole process tree is tracked — fixes the classic “launcher exits
  immediately, game keeps running” problem (Steam, Heroic, Lutris all do this).
- For URI launchers, wait for a child to appear in the scope or match the game
  window/process (Steam: `SteamAppId` env / `reaper` process) before showing
  “Playing”.
- Per-game launch options (data, editable on the pad): MangoHud, GameMode,
  gamescope flags, env vars, Proton version, pre/post scripts.
- **Transition:** focused cover scales into full-screen hero, logo centers,
  “Starting…” after 1.5 s only if the game hasn’t taken focus. Our window then
  goes idle (stop rendering, release GPU memory for textures not on screen).
- On exit: playtime written, window raised, focus restored to the same card.
- Guide button (or Home on Xbox pads) while in-game opens the quick menu overlay
  when running under our gamescope session (§8).

---

## 6. Art & metadata

### Art (per game: `cover` 600×900, `hero` 1920×620, `logo` transparent, `icon`)
Resolution order, stop at the first full hit per slot:
1. User override (picked in the art browser).
2. Steam CDN when a Steam appid is known: `library_600x900_2x.jpg`,
   `library_hero.jpg`, `logo.png`.
3. **SteamGridDB** (`/search/autocomplete` → `/grids?dimensions=600x900`,
   `/heroes`, `/logos`, `/icons`), prefer official/most-upvoted, no NSFW/humor by
   default. Uses an API key the user pastes once (QR code to the key page).
4. Source-provided art (Heroic/Lutris caches, GOG/Epic images).
5. Generated fallback: title set in the display face over a gradient taken from
   the icon. Never a stretched icon.

Cache: `$XDG_CACHE_HOME/superhuman/art/<game_id>/<slot>.webp` at display size, plus
a dominant + accent colour pair extracted once (k-means on the hero) and stored
in SQLite — the UI’s ambient colour comes from this (§7).

The art browser lets you scroll alternatives from SteamGridDB on the pad and pin
one per slot.

### Metadata
| Field | Source |
|---|---|
| Description, genres, release date, developer/publisher, screenshots | IGDB (Twitch client creds), Steam store API for Steam titles |
| Linux/Deck compatibility | ProtonDB summary tier, Steam Deck verified status |
| Fixes, config notes | PCGamingWiki link |
| Time to beat | HowLongToBeat (optional, scraping — off by default) |

Matching: external IDs first; otherwise fuzzy title search, and if confidence is
low the game page shows “Wrong game? Fix match” instead of guessing silently.
All fetches are rate-limited, retried with backoff, and work fully offline from
cache.

---

## 7. Design

### Subject
A living-room launcher read from three metres away in a dark room, on a TV. The
audience holds a controller, not a mouse. The screen’s one job: show you your
games so beautifully that picking one is the easy part.

### Direction: “Marquee at night”
Not a generic dark dashboard. The UI has almost no colour of its own — it borrows
it. The **focused game’s hero art floods the background** (heavily blurred,
darkened), and the interface’s accent shifts to that game’s extracted colour. The
launcher’s own identity is the warm amber of an arcade marquee / standby LED,
used only for focus.

| Token | Hex | Role |
|---|---|---|
| `night` | `#16142A` | Base background before art loads; deep indigo, not black |
| `dusk` | `#262245` | Raised surfaces: quick menu, keyboard, dialogs |
| `haze` | `#9A94BF` | Secondary text, inactive source glyphs |
| `paper` | `#F1EDF8` | Primary text |
| `marquee` | `#FFB547` | Focus ring/glow, primary button — the brand colour |
| `ambient` | per game | Extracted from hero; tints scrims, progress bars, game-page chips |

Contrast: text always sits on a scrim of `night` at ≥ 70 % over art; `paper` on
`night` is 15:1, `haze` on `night` 6.5:1.

### Type
- **Display: Bricolage Grotesque** (variable, OFL) — game titles when there’s no
  logo, screen titles, the big clock. Its optical-size axis lets the same face be
  tight and characterful at 72 px and calm at 32 px.
- **Body/UI: Atkinson Hyperlegible Next** (OFL) — built for legibility, which is
  exactly the problem at couch distance.
- Scale for 1080p (×1.33 at 1440p, ×2 at 4K, from window height):
  `18 / 24 / 32 / 44 / 64 / 88`. Minimum on-screen text 18 px. Sentence case
  everywhere; no all-caps labels.
- Fonts bundled in the binary (`include_bytes!`), no system font lottery.

### Layout
Home — one hero, then shelves. Everything left-aligned to a single 96 px safe
margin (TV overscan-safe).

```
┌────────────────────────────────────────────────────────────────────┐
│ [blurred hero of focused game fills entire background]       9:41 🎮│
│                                                                    │
│   ███ LOGO ART ███                                                 │
│   Played 2 days ago · 41 h        ProtonDB Platinum                │
│   [ Play ]  [ Details ]                                            │
│                                                                    │
│   Continue playing                                                 │
│   ┏━━━━┓ ┌──┐ ┌──┐ ┌──┐ ┌──┐ ┌──┐ ┌──┐                            │
│   ┃    ┃ │  │ │  │ │  │ │  │ │  │ │  │  ← focused cover is 1.15×,  │
│   ┗━━━━┛ └──┘ └──┘ └──┘ └──┘ └──┘ └──┘     amber glow, others dim  │
│   Favourites                                                       │
│   ┌──┐ ┌──┐ ┌──┐ ┌──┐ ┌──┐ ┌──┐ ┌──┐ ┌──┐                          │
│                                                                    │
│ Ⓐ Play  Ⓧ Details  Ⓨ Search  ☰ Menu                                │
└────────────────────────────────────────────────────────────────────┘
```

Game page — hero top, logo bottom-left over it, then a two-column body:

```
┌────────────────────────────────────────────────────────────────────┐
│ ██████████████████  HERO (sharp, not blurred)  ███████████████████ │
│ ███ LOGO ███                                                       │
│ [ Play ]  [ Launch options ]  [ ★ ]  [ … ]      via Steam ▾        │
├────────────────────────────────────────────────────────────────────┤
│ Description (max 70ch)          │ 41 h played · last 2 days ago    │
│                                  │ ProtonDB Platinum · Deck verified│
│ Screenshots ▸▸▸                  │ Developer, release, genres       │
└────────────────────────────────────────────────────────────────────┘
```

Other screens: Library grid (filter by source/installed/genre with shoulder
buttons switching tabs), Search (type-ahead with an on-screen keyboard laid out
for D-pad — two-row radial-free grid, letters ranked by next-letter frequency),
Quick menu (slides in from the left, `dusk` surface), Settings (all pad-editable),
Art browser.

### The one bold thing: focus
Focus is where all the boldness goes; everything else stays quiet.
- Focused cover lifts to 1.15×, gets a 3 px `marquee` ring with a soft outer glow,
  and tilts a few degrees following the right stick (parallax).
- Background crossfades (250 ms) to that game’s blurred hero; accent colour
  eases to its `ambient`.
- Unfocused covers sit at 70 % brightness so the eye has one place to land.
- Moving focus is spring-animated (critically damped, ~180 ms). Held D-pad
  accelerates after 400 ms. Scrolling keeps focus in a fixed column so the eye
  doesn’t chase it.
- Light rumble tick on shelf change (toggleable).

Other rules: no decorative motion that the player didn’t cause, except the
single boot reveal (logo art of last-played game fades in, shelves rise once).
Respect a “reduce motion” setting (crossfades only). Button glyphs match the
connected pad (Xbox / PlayStation / Nintendo / Deck), swap live on hotplug.

### Copy
Plain verbs: “Play”, “Install in Steam”, “Fix match”, “Add a game”. Empty
library: “No games found yet. Connect a launcher or add a folder of games.” with
the action right there. Errors say what failed and what to do: “Steam didn’t
start the game. Check it opens in Steam, then try again.”

---

## 8. Session mode (feels like a console)

- `superhuman --session`: ships a `superhuman-session.desktop` for display managers
  that runs `gamescope -e -f -- superhuman`, so the launcher is the whole session.
- In session mode we own the quick-menu overlay via gamescope’s focus handling;
  Steam’s own overlay keeps working for Steam games.
- Desktop mode: fullscreen borderless window, autostart optional, single
  instance (second launch focuses the first via a unix socket — also lets
  `superhuman --launch <id>` work from scripts).
- Power menu: suspend, restart, shut down, switch to desktop.

---

## 9. Milestones

Each one ends usable and committed.

1. **M1 — Skeleton:** iced window, theme tokens, bundled fonts, gilrs input →
   `Action`s, focus engine on a static grid of fake games. Proves the feel.
2. **M2 — Library:** SQLite, `Source` trait, Steam + Heroic + Lutris + Flatpak +
   `.desktop` scanners, dedup, instant start from cache.
3. **M3 — Art:** Steam CDN + SteamGridDB, disk cache, GPU texture pool, palette
   extraction, ambient background, generated fallback.
4. **M4 — Launch:** LaunchSpec, systemd scope tracking, playtime, transition,
   return-to-focus. **First daily-drivable build.**
5. **M5 — Game page & metadata:** IGDB/Steam/ProtonDB, fix-match flow, art
   browser, per-game launch options.
6. **M6 — Search, filters, on-screen keyboard, settings.**
7. **M7 — More sources:** Bottles, itch, Prism, emulators, manual Wine/Proton
   (umu-launcher).
8. **M8 — Session mode:** gamescope session, quick menu, power menu.
9. **M9 — Windows, macOS and every Linux distro:**
   - Windows and macOS sources from §4.
   - Launch tracking per platform behind one trait: systemd scope on Linux, with
     a process-group fallback for distros without systemd (Void, Alpine, Gentoo,
     Artix); Job Objects on Windows; process watching on macOS. `/proc` scans
     move to the `sysinfo` crate.
   - Packages: Flatpak, AppImage, AUR, .deb, .rpm; a Windows installer (winget);
     a signed macOS `.app` / `.dmg`.
   - Session mode (M8) stays Linux only; elsewhere the app runs fullscreen.

## 10. Quality bars

- 60 fps on focus moves on integrated graphics at 4K (measure frame time in a
  debug HUD from M1).
- Idle while a game runs: ~0 % CPU, textures evicted.
- Fully usable offline after first art fetch.
- Each source scanner has fixture-based tests (sample VDF/JSON/db files in
  `tests/fixtures/`).
- No source scan may block the UI; every one has a timeout.

## Open decisions
- iced vs Slint confirmed after M1 feel test.
- Whether to depend on HLTB scraping at all.
- IGDB needs the user’s own Twitch app credentials; fallback to Steam store data
  if they skip it.
