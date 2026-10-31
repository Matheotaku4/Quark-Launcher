# Quark

A **lightweight game launcher for Linux**, written in Rust (egui), meant as a sober
alternative to Steam: deep black (AMOLED), list or grid view, a game banner with a
**Play** button, playtime, ProtonDB rating and official artwork, with Windows games launched
through **Proton** — or through **umu-launcher**. Three tabs: a Steam store page, your
library, and **Mes jeux** — the Steam and Epic titles you own or have installed, in one list,
with save snapshots through Ludusavi.

## Features

- **Three tabs**: `Store` (Steam discovery), `Library`, and **Mes jeux** (owned and
  installed games across stores), plus a **Home** page reachable from the button above the
  game list. `Ctrl+Q` quits completely (tray icon included); the title bar and the executable
  carry the Quark icon (`assets/quark.png`, `assets/quark.desktop`).
- **Home page**: a carousel of the games you played last — the banner as background with
  rounded corners, the game's logo (or title), a **Continue** button (it becomes
  **Stop** while the game runs), playtime and last session next to it, the ProtonDB score
  and a *Linux native* pill in the top-left corner, arrows and dots to move between the five
  most recent games. Below it, the whole library as rounded covers, sorted by **last
  session**, **date added** or **hours played** — the choice is remembered. The carousel can
  advance **by itself** (Appearance → Home, 3–60 s), with a thin progress line under the
  banner; filters above the grid narrow the library by **status**, **Steam genre** or
  **personal tag**.
- **Personal library data**: a status per game (*To play*, *Playing*, *Finished*,
  *Dropped*), free-form **notes**, and **tags** (`coop`, `relaxing`…) with a real tag editor
  (add/remove chips). Statuses and tags become filter chips on the home page and in the left
  column — like Steam categories — and the search box also matches tags, genres and
  developer.
- **Steam metadata**: developer, genres, release date and short description come from the
  same `appdetails` endpoint already used as an artwork fallback, fetched once when a game
  is added (per-game *Fetch from Steam* button to refresh). Genres feed the genre filter.
- **Grid view shows the grid, nothing else**: the game page opens **as an overlay** with a
  close button, a click next to it, or `Esc`. The page background is the game's own banner,
  blurred, and the bottom third of the banner **fades into it** instead of ending on a hard
  edge; a graduated dark veil keeps the text readable all the way down. The four action
  buttons are colour-coded small icons — **red bin** (remove), **green pencil** (customize),
  **blue gear** (configure) — each on a dark disc with a coloured ring so it stays legible on
  a pale banner too.
- **Missing executable**: launching a game whose `.exe` no longer exists is refused with a
  clear message, and the game stays **highlighted in red** in the list, the grid and its page
  until the path is fixed.
- **Memory that stays flat**: artwork textures are decoded once and cached, but the cache is
  capped at 96 entries — the least recently used are freed first — so browsing a large
  library no longer makes memory climb indefinitely. Settings → About shows the resident
  memory and the number of textures currently held.
- **Adding a game in two steps**:
  1. type the game name — Quark searches **Steam** (public `storesearch` API) then
     **SteamGridDB** and fetches the **original artwork** (banner `library_hero`,
     logo, `library_600x900` cover, capsule icon);
  2. point at the **executable** (text field, native system file picker, or drag and drop).
- **Add from a folder**: pick a folder, Quark finds the executables, proposes a title
  matched by the search, and you can fix the title, answer "it's a game" or "it's not a
  game", then add everything at once.
- **Pick the artwork yourself**: once the game is recognised, a step lists the proposed
  banners, logos, covers and icons (SteamGridDB, then the official Steam assets) with a
  preview — you choose the one you want. The other proposals stay in the list as a
  fallback if a download fails or the format is wrong.
- **Checked artwork**: every downloaded image is verified (real dimensions) so a wrongly
  shaped thumbnail is rejected, with a fallback to Steam's `appdetails` page;
  `library_600x900_2x` cover, `library_hero` banner, transparent logo and small capsule.
- **Steam-like layout**: list on the left (thumbnail + title, the selected row has a blue
  marker), game pane on the right; a taller banner on top, **ProtonDB** and *Linux native*
  badges in its top-left corner, **Play** button with **playtime and last session** right
  next to it, then the **Journal**, **ⓘ**, **Customize** and **settings** buttons in the top
  right. `Play` becomes `Stop` while the game is running. Thumbnails, covers and the
  carousel use rounded corners.
- **Thumbnails that never move**: the list shows the game icon (1:1), as Steam does. When a
  game has no icon, the tile is filled with the first letter of its title — leading
  articles ignored, so `The Witcher 3` → `W` and `Les Chevaliers de Baphomet` → `C`, while
  `7 Days to Die` keeps its `7` — over a colour assigned to that game, so every row keeps
  exactly the same geometry. Icons come from SteamGridDB's `icons` route, then from Steam's
  `header.jpg`; small (16×16) and WebP icons are accepted, measured by real decoding rather
  than by the file header alone.
- **News under the banner**: the latest official announcements of the selected game
  (public Steam `ISteamNews` feed) — source, author, date, headline and a two-line excerpt,
  in plain text. Clicking a card expands the post **inside Quark** (up to 900 characters);
  *Lire sur Steam* opens the full post in your browser.
- **ProtonDB rating**: the compatibility tier (Platinum, Gold, Silver, Bronze, Borked) is
  read from ProtonDB's public API for the game's Steam AppID and shown as a coloured pill in
  the **top-left corner of the banner**, next to a *Linux native* pill when the executable is
  a Linux binary that runs without Proton, plus a row in the information block. Clicking it
  opens `protondb.com/app/<id>`. Cached per AppID; games without an AppID or without a report
  simply show no rating.
- **List / Grid switch**, only in the top-right corner of the top bar (no duplicate buttons
  in the left column or on the grid page); grid view shows only the grid, the game page being
  an overlay.
- **Status sections in list view**: games are grouped under their status — *Playing*, *To
  play*, *Finished*, *Dropped*, then *No status* — each header carrying its colour dot and
  game count, with a rule between sections. Clicking a header folds the section, as Steam's
  library columns do.
- **Launch options as checkboxes, per game** — no command line to remember:
  - **gamemoderun** (CPU governor from GameMode);
  - **MangoHud** (`MANGOHUD=1`, no wrapper binary needed);
  - **gamescope** with resolution, **FSR** upscaling (`-F fsr`) and frame limit
    (`--framerate-limit`), built as `gamescope -W … -H … -f … -- gamemoderun … `;
  - **PRIME offload** for hybrid laptops (`__NV_PRIME_RENDER_OFFLOAD`,
    `__GLX_VENDOR_LIBRARY_NAME`, `__VK_LAYER_NV_optimus`, `DRI_PRIME`);
  - a free **wrapper command** where `%command%` is replaced by the game command, exactly
    like Steam's launch options (`env FOO=1 %command%`); the line runs through `sh`, so
    `env`, pipes and redirections work. Without `%command%`, the wrapper is prepended.
  A missing tool never prevents the game from starting: the option is ignored (and the game
  still launches) and Quark says so. A “Tout réinitialiser” button clears them all.
- **Session log, readable in the app**: stdout and stderr of the game and of Wine are
  captured into `~/.local/share/quark/logs/<game>/session.log`, prefixed with the exact
  command, the prefix and every environment variable. The **Journal** button (left of
  *Customize*) shows the tail of that file, refreshes itself while the game runs, and can
  open the folder. Optional **`PROTON_LOG=1`** switch per game: Proton writes its own
  detailed log into the same folder (`PROTON_LOG_DIR`).
- **Stopping a game really stops it**: `Stop` runs `wineserver -k` on the game's prefix
  (closing the Windows processes the game spawned) and sends `SIGTERM` to the whole process
  group; after 4 seconds, the group is killed with `SIGKILL`.
- **umu-launcher support**: a per-game switch launches through `umu-run` instead of the
  Proton script, with `WINEPREFIX`, `GAMEID` (editable, `0` by default) and `PROTONPATH`
  when a runtime is selected. A missing `umu-run` is reported before launching.
- **Stays in the notification area**: with the setting enabled, closing the window really
  closes Quark — nothing is left in the task bar — while the icon stays in the notification
  area, held by a small dedicated process (`quark --tray`, StatusNotifierItem over D-Bus, no
  GTK dependency). One click on it starts Quark again; *Quit Quark* in its menu closes
  everything. This is deliberately not "hide the window": `set_visible` does not exist on
  Wayland, where the window would have stayed open. If your desktop has no watcher, the
  settings page says why and the close button quits.
- **Playtime tracking with history**: every session is timed and **stored** (start date and
  duration), not just summed.
- **Statistics page** (Settings → Statistics): total time, this week, current streak, hours
  per week for the last twelve weeks, a **heat-map calendar** of the last 26 weeks, and the
  most played games.
- **Disk space page** (Settings → Disk space): size of each Proton prefix, the artwork cache,
  session logs, downloaded update archives and each game folder, measured in a background
  thread so the window never freezes.
- **Steam library import** (Settings → Appearance → *Import the installed Steam library*):
  reads `appmanifest_*.acf` files from every Steam library (`libraryfolders.vdf`, Flatpak and
  Snap included), lists installed games with their AppID and detected executable, skips games
  already in Quark, and fetches artwork and metadata for each imported game. It lives in the
  settings rather than next to `+`, which is only for adding one game by hand.
- **Environment variables per game**: a `NAME=value` list (for example
  `WINEDLLOVERRIDES=winmm=n,b;steam_api64`) with a wide value field, plus default variables in
  the settings that are **copied into each game when you add it** — each game then owns its
  own copy, so editing one never touches the others.
- **"Defaults" settings page**: prefix, arguments, Proton runtime, anti-cheat, **launch
  options** (GameMode, MangoHud, gamescope, PRIME, wrapper, `PROTON_LOG`, umu) and environment
  variables that are **pre-filled** in every new game. These are defaults, not global
  settings: changing one does not modify games already in the library. Detected runtimes,
  the anti-cheat folders and a *Redetect* button sit right under the runtime choice; there is
  no separate “Proton” page any more, it only repeated these.
- **Icons that follow their background**: icons drawn over a banner use a translucent panel
  and switch to a dark tint of the background colour on a light image (a white icon over a
  sky-blue banner would be invisible). Monograms follow the same rule.
- **Right-click a game** for a menu with the same colour-coded icons as the game page:
  *Launch*, *Customize*, *Settings*, *Remove*. **Ctrl+click** marks several games and removes
  them in one go; `Esc` clears the selection.
- **Proton prefix used as-is**: the folder you pick (per game, or as the global default) *is*
  the prefix — Quark creates no sub-folder inside it. Only the automatic fallback keeps its
  own folder under `~/.local/share/quark/prefixes/<id>`.
- **Banners are cropped, never stretched**: the hero keeps its proportions at any window
  size, fullscreen included.
- **Customize** a game from its page (or by right-clicking it): each image — cover,
  banner, logo, icon — can be replaced by another online proposal or by a file from your
  disk, or removed to fall back to the built-in rendering.
- **Short, cheap transitions**: hover, selection, banner entrance, news cards, tabs, the home
  carousel (a 0.34 s slide-and-fade) and messages are animated. They all go through egui's
  animation manager, or a timestamped progression for the carousel, which only asks for a
  repaint while a value is actually moving, so an idle window stays idle and the launcher
  keeps its responsiveness.
- **AMOLED theme** (`#000000`) by default, plus a "Graphite" variant (softer black) in the
  settings.
- **Proton**: automatic runtime detection (`steamapps/common/Proton*` and
  `compatibilitytools.d/*`, including Flatpak/Snap), runtime chosen **per game** or as a
  default, a **default Proton prefix** in the settings and a **per-game override**.
- **Settings in tabs**: Appearance (theme, default view, notification icon), Defaults,
  Artworks, Statistics, Disk space, **Stores & tools** (accounts and managed tools) and About.
  Explanations live in small ⓘ buttons next to each setting instead of lines of grey text.
- **BattlEye and EasyAntiCheat runtimes** are detected next to Proton — including Steam's
  `Proton BattlEye Runtime` / `Proton EasyAntiCheat Runtime` folders, which carry no `proton`
  script — and each one can be enabled separately per game (both at once is allowed). Quark
  does not try to launch those folders: its keeps your Proton and passes them through
  `PROTON_BATTLEYE_RUNTIME` / `PROTON_EAC_RUNTIME`, the variables Proton actually reads.
- **Keyboard navigation**: arrows to move through the library (up/down and left/right in
  the grid), `Enter` to launch, `Ctrl+F` (or `/`) to jump to the search field, `Esc` to
  clear it. Shortcuts stay out of the way while a dialog or a text field has focus.
- **Update check**: the settings show the running version and query the latest GitHub
  release; the archive is downloaded to `~/.local/share/quark/updates/` and never installed
  behind your back. `quark --update` does the same without opening a window.
- **Command-line diagnostics**: `--doctor`, `--search`, `--fetch`, `--news`, `--scan`,
  `--update` and `--ui` (no screen required, `--home` renders the home page). `--tray` runs
  the notification-icon process and is started by Quark itself.
- **Steam store page**: a vertical, Steam-like discovery page — a featured deal, then
  *Recommandé pour vous*, *Populaires en ce moment* (with each game's peak concurrent
  players), *Meilleures ventes*, *Offres spéciales*, *Nouveautés* and *Prochainement* — in
  card grids that scroll down, never sideways. Prices are regional and the store page opens
  in your browser: Quark never buys anything. The catalogue is cached in
  `~/.local/share/quark/store-cache.json` and shown again, marked as potentially stale, when
  Steam is unreachable. Recommendations are ranked on your machine: from the titles in your
  library, or from your **owned Steam games** through `IPlayerService/GetOwnedGames` when a
  Steam Web API key and SteamID64 are configured — the section says which source was used,
  and your profile is never sent to Steam. These Steam endpoints are undocumented and
  handled as best-effort: a malformed response is reported, never cached as an empty store.
- **Mes jeux**: Steam (read from local `appmanifest_*.acf`) and Epic (through Legendary)
  games in one list — owned, installed, or both — with a filter per store, the detected
  install path, and what the store can actually do. The same game on two stores can be
  **merged** into one row and **split** again.
- **Installs are delegated, never faked**: *Installer via Steam* hands the AppID to the Steam
  client (`steam://install/<id>`) and then rescans; *Installer via Legendary* installs into
  the folder you choose and only marks the game **Installé** once a matching local directory
  has been found. A dialog closing or a process exiting is never taken as proof.
- **Save folders and snapshots from the same page**: custom save folders per installation,
  Ludusavi preview, backup, backup inventory and restore — see *Save files* below.
- **Deliberately absent**: achievements, cloud saves and the GOG library. No verified source
  is wired for them, so Quark shows nothing rather than empty results or invented
  capabilities.

## Build

Requirements: a recent Rust toolchain (`rustup`) and the X11/Wayland headers if your
distribution does not ship them already:

```bash
# Debian/Ubuntu
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev \
                 libwayland-dev libgl1-mesa-dev
# Arch
sudo pacman -S base-devel libx11 libxkbcommon wayland mesa
```

Then:

```bash
cargo run --release        # run Quark
cargo build --release      # binary: target/release/quark
```

### Main dependencies

- `eframe` / `egui` **0.32** (UI), `egui_extras` 0.32 (loading the HTTP thumbnails shown in
  the search results).
- `reqwest` (blocking, rustls) for Steam, SteamGridDB and ProtonDB, `serde` for
  `config.json`.
- `rfd` for the **native** file pickers (XDG Desktop Portal: GTK or KDE dialog depending on
  the desktop, `zenity` as a fallback). No GTK build dependency.
- `ksni` (StatusNotifierItem over D-Bus, `blocking` + `async-io` features) for the
  notification-area icon. No GTK/AppIndicator build dependency either.
- `image` (PNG/JPEG/WebP codecs): Quark decodes artwork itself and hands egui plain
  textures, instead of relying on the `file://` loader from `egui_extras`, which is not
  compiled by default and used to show an error icon where banners should be.

Note: `egui` 0.37 reshuffles its API (panels / `App::ui`); Quark intentionally targets the
0.32 series, whose API is stable and sufficient here.

## Usage

1. **Library** tab → the **+** button (top of the left column).
2. Type the game name: the search starts on its own (Steam first, then SteamGridDB if an
   API key is set).
3. Click the right result, then fill in the executable → **Add game**.
   Artwork downloads in the background (progress shown at the bottom of the column).
   Arguments, Proton runtime and prefix come pre-filled from **Settings → Defaults**.
4. The game appears selected: banner, logo, **Play** button, playtime and its ProtonDB
   rating. The gear in the top right of the pane opens the **game settings**
   (executable, arguments, Proton runtime, per-game prefix, anti-cheat, launch options,
   per-game variables, removal); the page icon left of it opens the session log.

### SteamGridDB API key (optional but recommended)

Without a key, Quark only uses the official artwork served by Steam's CDN.
With a free key (<https://www.steamgriddb.com/profile/preferences/api>) it also picks the
best-rated community banners/logos/covers, including for games absent from Steam
(GOG, itch.io, emulation…). The key goes into **Settings → Artwork** and stays in your local
configuration file.

## Stores, tools and save files

Steam works without any account: installed games are read from local `appmanifest_*.acf`
files. Everything else is optional, and stays on this machine.

### Credentials

Steam owned games (**Settings → Stores & tools**) need a personal Steam Web API key and a
SteamID64. They are written to `~/.config/quark/credentials.json` with mode `0600`, and **that
file is not encrypted**: anyone who can read your home directory can read the key. Quark never
logs it, and redacts registered secrets from command output. `GetOwnedGames` still returns
nothing for a profile whose game details are private, so an empty answer is reported as
ambiguous — it never becomes a "verified empty library" that would erase the games you already
know. Epic and GOG have their own fields in the same file.

### Managed tools

Legendary, gogdl and Ludusavi are downloaded from their official GitHub releases into
`~/.local/share/quark/tools/`:

| Tool | Used for | State |
| --- | --- | --- |
| Legendary | Epic library, installs, login | owned + installed lists, installs into a folder you pick |
| Ludusavi | local save snapshots | preview, backup, inventory, restore |
| gogdl | GOG | login through a GOG code; no library or install flow yet |

The SHA-256 digest published in the release metadata is mandatory: a release without a
digest, a digest mismatch, a missing Linux x64 asset, or a `--version` that does not match the
tag is refused. The downloaded file is never executed before its digest matches, and the
verified binary then replaces the previous one atomically, so a failed download or update
leaves the last working tool in place. **Settings → Stores & tools** shows the installed state
and only downloads when you ask.

Legendary and gogdl are published as **Python zipapps** (`#!/usr/bin/env python3` followed by
a zip archive), not as native ELF binaries, so `python3` must be installed on the machine.
Quark checks that exact layout instead of expecting an ELF, and refuses anything else.

### Connecting Epic and GOG

**Settings → Stores & tools** connects the accounts without Quark ever seeing a password:

1. *Se connecter* opens the store's own login page in your browser.
2. Copy the code that page shows: the `authorizationCode` value of the JSON response for Epic
   (`https://legendary.gl/epiclogin`), or the `code` parameter of the return address for GOG
   (`https://auth.gog.com/auth?client_id=…&response_type=code`).
3. Paste it into Quark and press *Connecter*: the code is handed to Legendary or gogdl, which
   exchanges it and writes its own session file (`~/.config/legendary/user.json`,
   `~/.local/share/quark/gogdl-auth.json`).

Both tools print session tokens and even exit with status 0 when the store refuses the code,
so Quark never trusts the exit status: the connection only counts once the session file has
actually been created or rewritten. The code is passed as a single argument, discarded from
output, and never logged or displayed again.

### Save files (Ludusavi)

Saves are handled per provider installation, not per title:

- **Custom folders**: **Mes jeux → Dossiers de sauvegarde…** adds folders Ludusavi does not
  know for that installation. They are merged into Quark's isolated
  `~/.local/share/quark/ludusavi/config.yaml`; `--config` is always passed, so your own
  `~/.config/ludusavi` is never touched. An existing YAML file keeps its settings, its other
  `customGames` entries and their keys (`registry`, `alias`…), but rewriting it drops comments
  and inlines anchors, and a path is never removed automatically.
- **Preview, then confirmation**: **Aperçu sauvegarde** runs Ludusavi's preview and shows the
  changed and processed games plus the data size; nothing is written until **Créer la
  sauvegarde**. A Ludusavi error or a failed file blocks that step and is reported as a
  failure, never as a successful backup.
- **One vault per installation** (`~/.local/share/quark/save-backups/<hash>/`), keeping at most
  10 full backups and no differential ones.
- **Automatic snapshots**: for a game linked to a provider installation, a background backup
  starts when the game is launched and another one when it exits. They never block or cancel a
  launch: a failure is a message, not a dialog.
- **Restore** requires the backup inventory, a fresh preview of the destination paths and an
  explicit confirmation; every destination is re-validated right before writing and must stay
  inside your home directory, the installation folder or a declared save folder. The forced
  Ludusavi call exists only inside that confirmed flow.

Achievements and cloud saves are **not** offered: no verified source is wired for either, so
nothing is claimed or displayed as empty. GOG games cannot be synced or installed yet.

The full manual check — real Ludusavi binary, exact commands, expected JSON and pass/fail
criteria — is in [`docs/verification/ludusavi-config.md`](docs/verification/ludusavi-config.md).

## Files

| Path | Contents |
| --- | --- |
| `~/.config/quark/config.json` | settings + games (readable, hand-editable JSON) |
| `~/.config/quark/credentials.json` | optional Steam/Epic/GOG credentials, mode `0600`, not encrypted |
| `~/.local/share/quark/artwork/<id>/` | downloaded artwork (no network request while displaying) |
| `~/.local/share/quark/prefixes/<id>/` | automatically created Proton prefixes |
| `~/.local/share/quark/logs/<id>/session.log` | last session: command, environment, game and Wine output |
| `~/.local/share/quark/tools/<name>` | managed tools (Legendary, gogdl, Ludusavi), digest-checked |
| `~/.local/share/quark/ludusavi/config.yaml` | isolated Ludusavi config, custom save folders merged in |
| `~/.local/share/quark/save-backups/<hash>/` | one save vault per provider installation |
| `~/.local/share/quark/store-cache.json` | last Steam store catalogue, reused when Steam is unreachable |
| `~/.local/share/quark/updates/` | downloaded release archives (never installed automatically) |

## Releases

Every push to `main` triggers
[`.github/workflows/release.yml`](.github/workflows/release.yml): GitHub Actions builds
Quark, packages `quark-<version>-linux-x86_64.tar.gz` (+ SHA-256 checksum) and publishes a
release tagged `v<version>` (version read from `Cargo.toml`). It can also be started by hand
from the repository's **Actions** tab.

The same run also renders the interface off-screen (`quark --ui`, no display required) for
list, grid, settings, dialogs, hover and keyboard states, and attaches the PNGs to the build
as the **quark-ui-renders** artifact — so every push can be reviewed without installing
anything.

```bash
tar -xzf quark-0.9.2-linux-x86_64.tar.gz
cd quark && ./quark
```

To get the Quark icon in your dock and application menu (the window carries the image
itself, on X11 and Wayland alike), copy the bundled desktop entry and icon:

```bash
install -Dm644 assets/quark.desktop ~/.local/share/applications/quark.desktop
install -Dm644 assets/quark.png ~/.local/share/icons/hicolor/512x512/apps/quark.png
update-desktop-database ~/.local/share/applications 2>/dev/null || true
```

## Proton, concretely

- Detection: `steam_root/{steamapps/common,compatibilitytools.d}`, plus
  `~/.local/share/Steam`, `~/.steam/root`, Flatpak (`~/.var/app/com.valvesoftware.Steam`)
  and Snap.
- Launch: `proton run <exe>` with `STEAM_COMPAT_DATA_PATH` = effective prefix and
  `STEAM_COMPAT_CLIENT_INSTALL_PATH` = Steam root.
- **Effective prefix**: per-game prefix → otherwise the settings' *default prefix* →
  otherwise `~/.local/share/quark/prefixes/<id>`.
- **Launch mode**: `Automatic` (Proton only for a `.exe`, native otherwise), `Linux
  (native)` to force a direct launch, or a specific Proton runtime.
- A runtime from `compatibilitytools.d` (GE-Proton…) is preferred by default (most recent
  first) when no explicit choice was made.
- **Wrapping order**: user wrapper (outermost) → gamescope → gamemoderun → game. A missing
  `gamescope` or `gamemoderun` is skipped with a visible warning.

### umu-launcher

umu runs a Windows game with Proton without Steam and without its own copy of Proton. In the
game settings, **Lancer via umu-launcher** switches the launch to:

```sh
WINEPREFIX=<prefix> GAMEID=<id|0> [PROTONPATH=<runtime>] umu-run <exe> <args>
```

`GAMEID` is the game's ProtonDB/Steam AppID (it drives ProtonDB reports and per-game fixes);
`0` is fine for an unknown game. With a Proton runtime selected, its folder is passed as
`PROTONPATH`; otherwise umu uses its own build. If `umu-run` is not in `PATH`, Quark refuses
the launch and says so instead of failing silently.

## Notification icon

When **Settings → Appearance → Zone de notification** is enabled, closing the window really
closes Quark: the window and its task-bar entry disappear, playtime is saved, and the icon
keeps living in the notification area (a
[StatusNotifierItem](https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/)).

The icon is held by a separate process, `quark --tray`, which Quark starts by itself and
which stays behind after the window is gone:

- one click on the icon / *Ouvrir Quark* → a new Quark window starts;
- *Quitter Quark* → the icon process exits (and stops a running window).

Why a second process instead of a hidden window: hiding is impossible to do reliably —
`Window::set_visible` is *unsupported on Wayland*, so the window would have stayed open and
kept its task-bar entry, which is exactly the bug this replaces. With a separate process the
behaviour is the same on X11 and Wayland, and a single-instance lock stops a second window
from writing `config.json` at the same time.

The icon is drawn by Quark itself (no icon theme needed). If your desktop exposes no
StatusNotifier watcher — GNOME without the AppIndicator extension, a bare session — the
settings page shows the D-Bus error and the close button simply quits, so you can never get
stuck with an icon that does nothing.

## Rendering check (no screen, no GPU)

Quark can render its own interface off-screen and write a PNG of it: egui tessellates the
UI in memory and Quark rasterises the triangles in software. That is how the interface is
checked after every change, including on a machine without a display server.

```bash
target/release/quark --ui preview.png                        # 1360x860, current config
target/release/quark --ui grid.png --view grid --demo        # built-in demo library
target/release/quark --ui hover.png --demo --hover 160,340   # pointer over a row
target/release/quark --ui anim.png --demo --frames 3         # mid-transition frame
target/release/quark --ui home.png --demo --home             # home page and carousel
target/release/quark --ui dialog.png --demo --dialog settings
target/release/quark --ui logs.png --demo --dialog logs
```

Options: `--size WxH`, `--scale N`, `--frames N`, `--view list|grid`, `--tab library|store|my-games`,
`--theme amoled|graphite`, `--select <game>`, `--info`, `--running`,
`--dialog none|add|artwork|exe|scan|settings|settings-defaults|settings-env|settings-proton|settings-providers|settings-artworks|settings-stats|settings-disk|settings-about|game|logs|customize|remove|updates|steam|overlay`, `--hover X,Y`,
`--click X,Y`, `--button left|right`, `--key up|down|left|right|enter|escape`, `--marked`,
`--home`, `--demo`.

`--click` and `--key` send real input events, so a render also checks the interactions:
`--key down` really moves the selection, `--click X,Y` really presses a button.

`--demo` builds a library of fake games with generated artwork (complete games, partial ones
and games with no image at all), plus a fake ProtonDB rating and a fake session log, so every
fallback gets rendered. Without `--demo` the real configuration is read; nothing is ever
written, only the PNG is produced.

The rasteriser is an approximation (gamma-space compositing, no sRGB framebuffer), so
antialiased edges can differ very slightly from the OpenGL backend used on screen — the
layout, texts, colours and animations are the same.

## Command-line diagnostics (no screen)

```bash
target/release/quark --doctor          # folders, detected Proton runtimes, game status
target/release/quark --search portal   # Steam + SteamGridDB results and artwork plan
target/release/quark --fetch portal    # actually downloads the artwork into /tmp
target/release/quark --scan ~/Games    # games detected in a folder
target/release/quark --news portal     # latest Steam news for a game (appid or title)
target/release/quark --update          # latest GitHub release (and downloads the archive)
```

## Code structure

```
src/main.rs         entry point + CLI (--doctor/--search/--fetch/--news/--scan/--ui)
src/app.rs          application state, egui loop, launching, playtime, toasts, ProtonDB
src/ui_home.rs      Home page: recent-games carousel, Continue, sorted library grid
src/ui_library.rs   Library tab: list/grid column + game pane (banner, Play, news, ProtonDB)
src/ui_my_games.rs  Mes jeux: store sync, duplicate merge, install handoff, save folders
src/ui_store.rs     Store tab: specials, search, local recommendations, price display
src/steam_store.rs  Steam Store catalogue, regional prices, local cache, recommendation ranking
src/providers/      provider model, Steam/Epic/GOG adapters, managed tools, credentials, installs
src/saves/          Ludusavi: isolated config merge, preview, backup, inventory, restore
src/ui_dialogs.rs   adding a game, global settings, game settings, session log, pickers
src/config.rs       data model, persistence (JSON), labels (playtime, dates, monogram)
src/artwork.rs      Steam/SteamGridDB search + artwork download and cache
src/scan.rs         game discovery inside a folder (batch adding)
src/news.rs         per-game news (public Steam feed) and opening links
src/protondb.rs     ProtonDB rating (public API), tier colours and labels
src/session.rs      launching, stdout/stderr capture, Stop (wineserver), log tail
src/stats.rs        session history: hours per week, heat-map calendar, streak, top games
src/disk.rs         disk usage of prefixes, artwork, logs, updates and game folders
src/steamlib.rs     installed Steam library import (appmanifest_*.acf)
src/icon.rs         embedded Quark icon (window and executable)
src/tray.rs         notification-area icon (StatusNotifierItem via ksni) + `quark --tray`
src/images.rs       texture cache, rounded/cropped drawing, accent and luminance analysis
src/proton.rs       Proton runtime detection + launch command (options, umu, anti-cheat)
src/update.rs       latest GitHub release, version comparison, archive download
src/theme.rs        AMOLED/Graphite palette, gradients, monogram colours
src/widgets.rs      hand-drawn vector icons and buttons (no icon font)
src/snapshot.rs     off-screen software renderer (--ui), for visual checks
```

## Possible next steps

- Achievements and cloud saves, but only once a per-store source can be verified.
- Wire the gogdl adapter into **Mes jeux** once its library and install output are pinned.
- Favourites on the home page.
- A time-range selector on the statistics page (7 days, 30 days, this year).
- A “verify prefix” action.
