# Quark

A lightweight game launcher for Linux, written in Rust with egui. Quark keeps a
single library for local games and for the Steam, Epic Games and GOG titles you
own, launches Windows games through Proton (or umu-launcher), and stays out of
the way with an AMOLED interface, a store page, save snapshots and an in-game
overlay.

This is the **1.3.0** documentation. Older release notes are kept on the GitHub
releases page; `docs/release-notes.md` only describes the release currently
being published.

**Documentation:** [GitHub Wiki](https://github.com/Matheotaku4/Quark-Launcher/wiki) ·
[Getting started](https://github.com/Matheotaku4/Quark-Launcher/wiki/Getting-Started) ·
[Proton and Wine options](https://github.com/Matheotaku4/Quark-Launcher/wiki/Proton-Wine-and-Launch-Options) ·
[Troubleshooting](https://github.com/Matheotaku4/Quark-Launcher/wiki/Troubleshooting).
The wiki is published from the versioned source in [`wiki/`](wiki/) by the
`.github/workflows/wiki.yml` workflow.

![Quark home](docs/images/home-fr.png)

| Library | Settings |
| --- | --- |
| ![Library](docs/images/library-fr.png) | ![Settings](docs/images/settings-fr.png) |

| Welcome guide | What's new |
| --- | --- |
| ![Onboarding](docs/images/onboarding-fr.png) | ![What's new](docs/images/whats-new-fr.png) |

## What's new in 1.3.0

- **PlayStation 2 with PCSX2.** Add an `.iso`, `.chd`, `.cso`, `.bin/.cue` or `.gz`
  image as a game; Quark detects PCSX2, reads the serial from uncompressed ISO
  files, launches the emulator without Proton and fetches covers from
  [xlenore/ps2-covers](https://github.com/xlenore/ps2-covers) by serial number.
- **RetroAchievements progress.** Set your username and Web API key in Settings,
  then open the PlayStation 2 tab of a game to find its RetroAchievements entry
  by title and see progress in the game page and the in-game overlay. PCSX2
  still signs in and unlocks achievements itself; Quark only reads them.
- **A DLL overrides tab for every game.** Pick a source folder, select its DLLs
  and choose **n** (native), **b** (builtin) or both. Existing
  `WINEDLLOVERRIDES` entries are loaded, preserved and written back.
- **Grey placeholder suggestions.** The proposed values in Tags and Arguments
  now use the faint text colour instead of looking like typed input.
- **An English wiki.** Every wiki page, including the new PlayStation 2 guide,
  is now written in English.

| PlayStation 2 |
| --- |
| ![PlayStation 2 settings](docs/images/ps2-tab-en.png) |

## Table of contents

1. [What's new in 1.3.0](#whats-new-in-130)
2. [What Quark does](#what-quark-does)
3. [Installation](#installation)
4. [First launch](#first-launch)
5. [Interface](#interface)
6. [One game, several installations](#one-game-several-installations)
7. [Stores, accounts and cloud saves](#stores-accounts-and-cloud-saves)
8. [Achievements and the in-game overlay](#achievements-and-the-in-game-overlay)
9. [PlayStation 2 with PCSX2](#playstation-2-with-pcsx2)
10. [Proton, umu and launch options](#proton-umu-and-launch-options)
11. [Local saves and snapshots](#local-saves-and-snapshots)
12. [Settings](#settings)
13. [Command line](#command-line)
14. [Files, data and privacy](#files-data-and-privacy)
15. [Project layout](#project-layout)
16. [Development and tests](#development-and-tests)
17. [Known limitations](#known-limitations)
18. [Licence](#licence)

## What Quark does

- **One library.** Local games and store games share covers, metadata, notes,
  tags, playtime and statistics. A game can have several installed editions
  without becoming two library entries.
- **Steam, Epic Games and GOG.** Steam is read from the local Steam libraries.
  Epic is managed with Legendary and GOG with gogdl. Accounts are connected
  from the settings; passwords never pass through Quark.
- **Proton without command lines.** Per-game runtime, prefix, GameMode,
  MangoHud, gamescope, PRIME offload, a free wrapper command, environment
  variables, BattlEye/EasyAntiCheat runtimes and umu-launcher.
- **PlayStation 2 through PCSX2.** Disc images become ordinary library entries:
  serial detection, community covers and RetroAchievements progress, without
  creating a Wine prefix for the emulator.
- **A real in-game overlay.** Press `Shift+Backspace` to open a Steam-like
  panel over the game and read its achievements. `F12` takes a screenshot.
- **Cloud saves for Epic and GOG.** The account save is fetched after an
  installation, then synchronized again when the game exits, using the same
  upstream data as the official clients.
- **Local save snapshots.** Ludusavi creates verified snapshots before a launch
  and after a session, with preview, inventory and restore.
- **A sober, configurable interface.** Thirteen dark themes, French or English,
  list or grid, a home carousel, a random blurred hero background, status
  columns, tags, filters, a Ctrl+K command palette and an embedded Archivo font
  with Lucide icons.

Quark is not a store: it never buys anything, and it never downloads or
extracts games from third-party download sites. Store search results open in
your browser.

## Installation

### Release archive

Download `quark-<version>-linux-x86_64.tar.gz` and its `.sha256` file from the
[latest release](https://github.com/Matheotaku4/Quark-Launcher/releases/latest),
then verify and unpack it:

```bash
sha256sum -c quark-1.3.0-linux-x86_64.tar.gz.sha256
tar -xzf quark-1.3.0-linux-x86_64.tar.gz
./quark/quark
```

The archive contains `quark/quark`, `quark/README.md`, `quark/quark.desktop`
and `quark/quark.png`. To add Quark to the applications menu, install them for
the current user:

```bash
install -Dm755 quark/quark ~/.local/bin/quark
install -Dm644 quark/quark.desktop ~/.local/share/applications/quark.desktop
install -Dm644 quark/quark.png ~/.local/share/icons/hicolor/512x512/apps/quark.png
update-desktop-database ~/.local/share/applications 2>/dev/null || true
```

`~/.local/bin` must be in your `PATH` for the `Exec=quark` entry to work; if it
is not, use the absolute path of the binary in the desktop file. Quark needs
X11 or Wayland, OpenGL and the usual font/GL libraries of your distribution.
`python3` is required by Legendary and gogdl (both are Python zipapps).

### Build from source

```bash
git clone https://github.com/Matheotaku4/Quark-Launcher.git
cd Quark-Launcher
cargo build --release --locked
./target/release/quark
```

On Debian/Ubuntu, the CI installs:

```bash
sudo apt-get install build-essential pkg-config libx11-dev libxkbcommon-dev \
  libwayland-dev libgl1-mesa-dev
```

## First launch

A new configuration opens a three-step welcome guide. It is not blocking:
**Skip** closes it. The guide changes the interface language, startup page,
theme and hero background live, then offers to add a first game. Existing
configurations are marked as already welcomed, so updating from 1.1.4 never
shows it unexpectedly; they still see the 1.3.0 summary once.

1. Open **Settings → Stores & tools** and install Legendary and/or gogdl with
   **Install / update**. Quark verifies the SHA-256 digest published by GitHub
   before running a downloaded tool.
2. Connect Epic or GOG with the code copied from the browser page. The
   credentials are stored locally in a private file, never sent to a Quark
   server.
3. Add a game with the **+** button, import the installed Steam library, or open
   **Stores** to browse what you own.
4. Pick a Proton runtime in **Settings → New games** if the automatic choice is
   not what you want. Every game can override it.

## Interface

### Home

The home page is a carousel of recently played games. The banner is the game's
hero image, the logo (or title) sits above a **Continue/Stop** button, and the
playtime, last session, ProtonDB rating and native-Linux badge are visible at a
glance. Below the carousel, the whole library is shown as covers. Sort it by
last session, date added or hours played. The **Filters** button opens one menu
for status, Steam genre and personal tag; the button shows the number of active
filters and the menu includes **Clear all**. The carousel can advance
automatically; the delay is set in **Settings → Appearance**.

**Startup page.** Settings → Appearance → **Startup page** decides what Quark
opens at launch: **Home** (the default), **Library**, **Store** or **My games**.
The choice is applied once when the window opens and is saved with the library;
an older configuration without the setting starts on Home.

**Random hero background.** Appearance → Background can reuse a library banner
as a blurred backdrop behind Home and Library. One banner is picked per session;
the blur radius is adjustable from 2 to 30 px and is part of the blur cache key.

### Library

The list shows one row per game with a square icon, a status dot and a blue
selection marker. The grid shows covers and opens the game page as an overlay.
Games with a missing executable are highlighted in red. Right-click a game for
launch, customize, settings and remove; `Ctrl+click` marks several games for a
batch removal.

The **Views** menu stores the current search, filters, sort and grouping under
a name; apply or delete a view from the same menu. Three starter views (**À jouer**,
**En cours** and **Terminés**) are added once, then behave like any
other view. The **Filters** button groups status, Steam genre and tags in a
single popup instead of stacking chips above the list; active filters are
counted on the button and **Clear all** resets them. Group the list by status,
store, genre or release year, or show a continuous list; group headers can be
collapsed. Sort by name, last session, date added or play time. Library search
now ignores accents and compares the title, developer, tags and genres
separately, like the command palette.

![Filters menu](docs/images/filters-fr.png)

Keyboard shortcuts: arrows navigate, `Enter` launches, `Ctrl+F` or `/` focuses
the search field, `Esc` clears it and `Ctrl+Q` really quits Quark. `Ctrl+K`
opens the command palette from any tab: type to fuzzy-search pages, actions and
games, use `↑`/`↓` to move, `Enter` to open and `Esc` to close.

### Game page

The page shows the hero banner, ProtonDB/native badges, the **Play** button with
playtime, news, achievements and history tabs, and the action buttons for logs,
information, customization and per-game settings. **History** summarises the
game's sessions, shows the last twelve weeks and lists its latest play sessions.
**Customize** replaces or
removes the cover, hero, logo and icon; images can come from SteamGridDB,
Steam, the store itself, or a local file.

### Stores

The **Stores** tab is a Steam discovery page: featured deal, recommendations,
the most played games, top sellers, specials, new releases and upcoming titles.
Prices are regional and the store page opens in your browser. A game card also
offers a search on the download sites supported by Quark; results only open the
site in your browser.

### My games / Stores library

**My games** lists the titles you own or have installed on Steam, Epic and GOG,
one tab per store, sorted by name, with a search field and store covers. A left
or right click on an uninstalled game opens the install sheet; an installed game
opens its actions. Up to three Epic/GOG downloads run at once, each with its own
progress, speed, ETA and **Cancel** button. The download icon in the top bar
lists every transfer.

## One game, several installations

Quark 1.1 introduces a `Game → Installations` model. A `Game` carries the
shared data (name, artwork, notes, tags, status, playtime, sessions); each
`Installation` carries one edition:

- store or local source, product ID and install folder;
- executable and Proton prefix;
- cloud-save folder and GOG synchronization timestamp;
- installed state and last sync time.

The first launch migrates an existing configuration automatically: the old
`exe` field becomes a local installation, and every linked store record becomes
an edition of the same game. The migration is idempotent and runs before each
save, so an interrupted migration cannot duplicate entries.

The game information panel lists the editions. Selecting one changes the
executable and prefix used by **Play**, without touching the other edition.
When the historical `~/.local/share/quark/prefixes/<game>` prefix already exists,
it is reused so that existing saves stay visible after switching from a local
copy to a store edition. A genuinely new second installation gets its own
`<game>-<store>` prefix instead. This keeps a Steam copy and a GOG copy of the
same title usable with separate launch state while preserving the historical
saves.

The legacy fields (`exe`, `prefix`, `proton`, …) are still serialized for
compatibility and remain the values used by the launch resolver. The active
installation is the source of truth when it is selected.

## Stores, accounts and cloud saves

### Accounts

- **Steam** is read locally from `libraryfolders.vdf` and
  `appmanifest_*.acf`. A personal Steam Web API key and SteamID64 are optional
  and only improve recommendations.
- **Epic Games** uses Legendary. The authorization code is exchanged locally
  and the resulting `user.json` stays in Legendary's configuration directory.
- **GOG** uses gogdl. Its token file is separate from Quark's
  `credentials.json` and is written with private permissions.

The managed tools are downloaded from their official GitHub releases. Quark
refuses an asset without a published SHA-256 digest and never trusts a
mismatched binary.

### Cloud saves

Cloud synchronization is enabled by default in **Settings → In game**.

- **Epic:** Quark runs `legendary -y sync-saves <app> --accept-path`. Legendary
  compares the local save folder with the cloud manifest; the newer side wins.
  A custom save folder can be supplied with `--save-path`.
- **GOG:** Quark runs `gogdl save-sync <folder> <game-id> --ts <timestamp>`.
  gogdl requires a folder, so choose one in the game information panel or in
  **My games → Save folder**. The timestamp returned by gogdl is stored in the
  installation and reused for the next comparison.
- **Steam:** Steam Cloud stays inside the Steam client. Quark does not read or
  write Steam's cloud saves.

The synchronization runs after a successful Epic/GOG installation and after a
game launched by Quark exits. It is asynchronous, so it never blocks the
interface. The game information panel also has a manual **Synchronize cloud
saves** button and shows the last result. A save is never invented: when no
local or remote save exists, the panel says so.

## Achievements and the in-game overlay

### Achievements

Epic achievements are read with `legendary achievements <app> --json`. The
parser accepts log lines around the JSON, null lists and the older plain-text
table (`Total achievements`, `Completed`, `In progress`). If the metadata cache
is incomplete, Quark refreshes it with `legendary info` and retries once. A
missing session or a game without achievements now produces an explicit message
instead of an unreadable-response error.

GOG achievements come from `gameplay.gog.com` with the gogdl session. Both
sources are normalized to the same rows, with icons cached locally, rarity when
published, and a **session** badge for achievements reported during play.

A game can report an unlock through the local bridge:

```bash
curl -X POST "$QUARK_ACHIEVEMENTS_URL" \
  -H "Authorization: Bearer $QUARK_ACHIEVEMENTS_TOKEN" \
  -d '{"game":"'"$QUARK_GAME_ID"'","achievement":"ACH_FIRST","name":"First step"}'
```

The listener only binds `127.0.0.1`; the token changes on every launch and is
masked in the session log.

### Overlay

Press **`Shift+Backspace`** while a game launched by Quark is running. The
overlay appears above the game as a full-screen panel with the game name,
session clock, store and achievement progress. It is closed by the same
shortcut, `Esc`, or the close button.

`F12` takes a screenshot under
`~/.local/share/quark/screenshots/<game>/`. Epic and GOG captures also play a
short chime and raise a desktop notification; Steam captures stay silent. The
overlay remains mouse-transparent while closed, so it never steals a click from
the game.

Display options live in **Settings → In game**:

- show or hide locked achievements;
- compact rows without descriptions;
- show or hide progress and rarity;
- choose the overlay shortcut and the screenshot key.

The X11 global shortcut covers Proton games through XWayland. On native Wayland,
the desktop's global-shortcut policy may prevent a global key; the overlay still
works with the close button and the in-app screenshot path. The shortcut thread
is stopped when the game ends or when the key changes.

## PlayStation 2 with PCSX2

Quark treats a PS2 disc image as a native game entry: there is no Wine prefix
and no Proton runtime. Install PCSX2 (from your distribution, pcsx2.net or
Flathub) and Quark finds `pcsx2-qt`, `pcsx2` or the Flatpak export
`net.pcsx2.PCSX2`. If the emulator lives elsewhere, set its binary in
**Settings → Stores & tools**.

Use **+ → PlayStation 2 game (ISO)…** and pick an image. Supported formats are
`.iso`, `.chd`, `.cso`, `.bin/.cue` and `.gz`; the serial number is read
automatically from `SYSTEM.CNF` in uncompressed `.iso` files. For other formats,
open the game's **PlayStation 2** tab and enter the serial by hand
(`SLUS-20062`). The tab can also re-read the ISO, reload the cover and manage
the RetroAchievements entry.

Covers come from the community repository
[xlenore/ps2-covers](https://github.com/xlenore/ps2-covers), at
`covers/default/<SERIAL>.jpg`. A missing cover only leaves the monogram; it
never blocks the launch.

### RetroAchievements

PCSX2 has its own RetroAchievements integration and remains responsible for
signing in and unlocking achievements. Quark uses the official Web API only to
read the same progress:

1. Create an account on [retroachievements.org](https://retroachievements.org)
   and copy the **Web API key** from your profile → **Keys**.
2. Enter the username and key in **Settings → Stores & tools**.
3. Open a PS2 game, then **PlayStation 2 → Search by title**. Quark queries the
   PlayStation 2 game list and proposes the closest title; correct the RA ID if
   needed (the API has no serial-number lookup).
4. **Show achievements** loads the list in the game page. The in-game overlay
   uses the same data the next time the game is launched.

The key is stored locally in `config.json`, like the SteamGridDB key, and is
only sent to `retroachievements.org`.

## Proton, umu and launch options

Quark detects Proton builds under `steamapps/common/Proton*` and
`compatibilitytools.d/`, including Flatpak and Snap installations. A runtime can
be selected per game or as a default. The selected prefix is used as-is; only
the automatic fallback creates `~/.local/share/quark/prefixes/<game>`.

Per-game launch options are checkboxes, not command lines:

- **GameMode** (`gamemoderun`);
- **MangoHud** (`MANGOHUD=1`);
- **gamescope** with resolution, FSR and frame limit;
- **PRIME offload** for hybrid laptops;
- a free **wrapper** command where `%command%` is replaced by the game command;
- `PROTON_LOG=1`;
- **umu-launcher** with `WINEPREFIX`, `GAMEID` and `PROTONPATH`.

A missing optional tool is reported and ignored; the game still starts. The
BattlEye and EasyAntiCheat runtimes are passed to Proton through
`PROTON_BATTLEYE_RUNTIME` and `PROTON_EAC_RUNTIME`.

## Local saves and snapshots

The save panel is shared by the library and **My games**. It supports custom
save folders, a Ludusavi preview, a full backup, a backup inventory and restore.
Snapshots are taken before a launch and after a session. The restore preview is
validated against the game's install roots; symlinks, `$HOME` as a root and
paths outside the allowed roots are refused. Each provider installation has its
own Ludusavi bucket, so two editions of the same title never share a snapshot.

Ludusavi is downloaded from its official release and verified before use.
Quark never replaces an unreadable Ludusavi configuration; it reports the error
and keeps the file.

## Settings

Settings occupy the whole window, with a left sidebar and sections separated by
accented headings. The content scrolls independently and `Esc` or the top-right
**×** closes the page.

- **Appearance:** interface language, 13 themes, random hero background,
  startup page, list/grid, carousel, notification icon.
- **In game:** overlay shortcut, screenshot key, achievement display and
  cloud-save toggle.
- **New games:** defaults copied into each newly added game.
- **Artworks:** image sources, SteamGridDB key and cache.
- **Stores & tools:** sync, accounts, managed tools and the EOS overlay.
- **Statistics** and **Disk space:** play history and storage measurements.
- **About:** version, updates, release link, guide and what's-new actions.

**Appearance** contains the startup page, interface language, 13-theme grid,
random hero background, default view, home carousel and notification icon. **In game** contains the overlay shortcut, screenshot key, achievement
display and cloud-save toggle. **New games** contains the values copied into
each newly added game (prefix, install folder, arguments, runtime, anti-cheat,
launch options, environment variables). These are defaults, not global
settings: changing one never modifies an existing game.

**Stores & tools** contains synchronization, accounts, managed tool
installation and the actions that require a store session. **Statistics**
shows total playtime, the week, the streak, twelve weeks of bars, a 26-week
heat map and the most played games. **Disk space** measures prefixes, artwork,
logs, updates and game folders in a background thread.

**About** contains the installed version and update settings. On startup,
Quark checks the latest GitHub release when **Install updates at startup** is
enabled (the default), verifies the SHA-256 and the new binary, replaces the
executable in place and relaunches itself. It keeps the previous executable
next to the new one until the next successful start. The automatic path is
skipped for development builds and read-only installs; a game running in Quark
blocks the install. Use **Check for updates** to do it manually. 1.1.4 and
later install subsequent releases automatically with the same archive format;
the manual archive remains available for read-only installs and for a recovery.

## Command line

```text
quark                     Open the launcher
quark --doctor            Check the local environment
quark --search <query>    Search Steam / SteamGridDB without opening the UI
quark --fetch <name>      Search a title and download its artwork
quark --news <appid|name> Read the Steam news feed
quark --scan <folder>     Scan a folder for games
quark --update            Check and download the latest release archive
quark --ui <file.png>     Render the interface off-screen (no display needed)
quark --tray              Run the notification-icon process
```

`--ui` accepts `--demo`, `--view`, `--group`, `--tab`, `--theme`, `--lang`,
`--select`, `--dialog`, `--hero-bg`, `--blur`, `--size`, `--frames`, `--hover`,
`--click` and `--key`; `--demo` uses a temporary configuration and never reads or
writes the real library. `--group` accepts `status`, `store`, `genre`, `year` or
`none`; `--lang` accepts `fr` or `en`; `--blur` is a 2–30 px radius.
`--dialog` includes `onboarding`, `onboarding-settings` and `whats-new` for the
new screens.

## Files, data and privacy

| Path | Contents |
| --- | --- |
| `~/.config/quark/config.json` | Settings, library, installations and play history |
| `~/.config/quark/credentials.json` | Optional Steam Web API key and SteamID64 |
| `~/.config/quark/legendary/user.json` | Epic session written by Legendary |
| `~/.local/share/quark/gogdl-auth.json` | GOG tokens written by gogdl |
| `~/.local/share/quark/artwork/` | Cached artwork and achievement icons |
| `~/.local/share/quark/prefixes/` | Automatic Proton prefixes |
| `~/.local/share/quark/logs/<game>/` | Session logs, mode `0600` |
| `~/.local/share/quark/save-backups/` | Ludusavi snapshots |
| `~/.local/share/quark/screenshots/` | Overlay screenshots |
| `~/.local/share/quark/updates/` | Downloaded update archives |

Library writes are atomic (`fsync` + rename) and retried when they fail. A
corrupt `config.json` is copied aside and the library becomes read-only instead
of being overwritten with an empty one. Session logs never contain environment
values, full command lines or the achievement token. Quark has no server and
does not send telemetry.

## Project layout

```text
src/
  app.rs              application state, jobs and background workers
  config.rs           library, settings, Game and Installation models
  i18n.rs             French/English catalogue and dynamic labels
  theme.rs            the 13 palettes
  ui_command.rs       Ctrl+K command palette
  ui_home.rs          home carousel and random hero background
  ui_library.rs       library, game page and installation selector
  ui_my_games.rs      owned/installed store games and install actions
  ui_dialogs.rs       settings, add-game, game settings and save dialogs
  ui_onboarding.rs    first-run guide and what's-new summary
  ui_providers.rs     store accounts and synchronization UI
  providers/          Steam, Epic (Legendary), GOG (gogdl), achievements,
                      cloud_saves, artwork and managed tools
  proton.rs           runtime detection, prefix resolution and launch options
  session.rs          process groups, logs and screenshots
  saves/              Ludusavi integration and snapshot validation
  overlay.rs          in-game overlay, global shortcuts and X11 capture
  snapshot.rs         off-screen PNG renderer for the CI previews
assets/i18n/en.json  English interface catalogue (French is the source)
docs/images/          README and wiki screenshots
docs/
  release-notes.md    notes for the current release only
wiki/                 GitHub Wiki source: guides, Proton variables, help
.github/workflows/
  rust.yml            tests and checks
  release.yml         build, previews and GitHub release
```

## Development and tests

```bash
cargo fmt --check
cargo test --offline
cargo build --release --locked --offline
```

The release workflow builds on Ubuntu 22.04, renders the interface off-screen
with `--ui --demo`, publishes the PNG previews as a workflow artifact and
creates the GitHub release from `docs/release-notes.md`. The release notes file
contains only the version currently being published; previous notes stay on
their existing GitHub releases.

## Known limitations

- Steam achievements are not readable without a Steam session, so Quark shows
  the Steam achievement tab only when the store exposes a readable source.
  Steam Cloud is left to the Steam client.
- GOG cloud saves need an explicit save folder; gogdl cannot infer it from the
  installation path. Epic can compute its path with Legendary.
- Real Epic/GOG accounts and a graphical X11 session are not available in the
  development environment. The parsers, argument construction, migration and
  error paths are covered by tests; a manual account and overlay test is still
  required before claiming a complete end-to-end validation.
- Native Wayland global shortcuts depend on the compositor. The overlay and
  screenshots have an X11/XWayland path and desktop-tool fallbacks.
- Cloud synchronization is deliberately opt-in by store capability. Quark never
  invents a remote save or marks a synchronization successful when the provider
  returned an error.

## Licence

Quark is distributed under the MIT licence. The embedded Archivo font is under
the SIL Open Font License; the Lucide icon font is under the ISC licence. See
`assets/fonts/` and the release notes for the exact files.
