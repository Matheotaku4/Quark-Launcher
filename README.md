# Quark

A **lightweight game launcher for Linux**, written in Rust (egui), meant as a sober
alternative to Steam: deep black (AMOLED), list or grid view, a game banner with a
**Play** button, playtime, ProtonDB rating and official artwork, with Windows games launched
through **Proton** — or through **umu-launcher**.

## Features

- **Two tabs**: `Store` (deliberately empty for now) and `Library`.
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
  marker), game pane on the right; banner on top, **Play** button with **playtime** right
  next to it, then the **Journal**, **ⓘ**, **Customize** and **settings** buttons in the top
  right. `Play` becomes `Stop` while the game is running.
- **Thumbnails that never move**: the list shows the game capsule, as Steam does. When a
  game has no icon, the tile is filled with the first letter of its title — leading
  articles ignored, so `The Witcher 3` → `W` and `Les Chevaliers de Baphomet` → `C`, while
  `7 Days to Die` keeps its `7` — over a colour assigned to that game, so every row keeps
  exactly the same geometry.
- **News under the banner**: the latest official announcements of the selected game
  (public Steam `ISteamNews` feed) — source, author, date, plain-text excerpt; clicking one
  opens the article in your desktop browser.
- **ProtonDB rating**: the compatibility tier (Platinum, Gold, Silver, Bronze, Borked) is
  read from ProtonDB's public API for the game's Steam AppID and shown as a coloured pill
  next to the playtime, plus a row in the information block. Clicking it opens
  `protondb.com/app/<id>`. Cached per AppID; games without an AppID or without a report
  simply show no rating.
- **List / Grid switch** from the left column (the grid widens the column and shows the
  2:3 covers).
- **Launch options as checkboxes, per game** — no command line to remember:
  - **gamemoderun** (CPU governor from GameMode);
  - **MangoHud** (`MANGOHUD=1`, no wrapper binary needed);
  - **gamescope** with resolution, **FSR** upscaling (`-F fsr`) and frame limit
    (`--fps-limit`), built as `gamescope -W … -H … -f … -- gamemoderun … `;
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
- **Stays in the notification area**: with the setting enabled, closing the window keeps
  Quark alive in a tray icon (StatusNotifierItem, no GTK dependency): clicking it brings the
  window back, and *Quit Quark* in its menu really exits. Disabled by default only if your
  desktop has no watcher — the settings page then says why, and the close button quits.
- **Playtime tracking**: every session is timed (relaunch, total time, last session).
- **Environment variables per game**: a `NAME=value` list (for example
  `WINEDLLOVERRIDES=winmm=n,b;steam_api64`) with a wide value field, plus default variables in
  the settings that are **copied into each game when you add it** — each game then owns its
  own copy, so editing one never touches the others.
- **"Defaults" settings page**: prefix, arguments, Proton runtime, anti-cheat and environment
  variables that are **pre-filled** in every new game. These are defaults, not global
  settings: changing one does not modify games already in the library.
- **Icons that follow their background**: icons drawn over a banner use a translucent panel
  and switch to a dark tint of the background colour on a light image (a white icon over a
  sky-blue banner would be invisible). Monograms follow the same rule.
- **Right-click a game** to open its settings. **Ctrl+click** marks several games and
  removes them in one go; `Esc` clears the selection.
- **Proton prefix used as-is**: the folder you pick (per game, or as the global default) *is*
  the prefix — Quark creates no sub-folder inside it. Only the automatic fallback keeps its
  own folder under `~/.local/share/quark/prefixes/<id>`.
- **Banners are cropped, never stretched**: the hero keeps its proportions at any window
  size, fullscreen included.
- **Customize** a game from its page (or by right-clicking it): each image — cover,
  banner, logo, icon — can be replaced by another online proposal or by a file from your
  disk, or removed to fall back to the built-in rendering.
- **Short, cheap transitions**: hover, selection, banner entrance, news cards, tabs and
  messages are animated. They all go through egui's animation manager, which only asks for
  a repaint while a value is actually moving, so an idle window stays idle and the launcher
  keeps its responsiveness.
- **AMOLED theme** (`#000000`) by default, plus a "Graphite" variant (softer black) in the
  settings.
- **Proton**: automatic runtime detection (`steamapps/common/Proton*` and
  `compatibilitytools.d/*`, including Flatpak/Snap), runtime chosen **per game** or as a
  default, a **default Proton prefix** in the settings and a **per-game override**.
- **Settings in tabs**: Appearance (theme, default view, notification icon), Defaults,
  Proton, Artworks, About. Explanations live in small ⓘ buttons next to each setting instead
  of lines of grey text.
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
  `--update` and `--ui` (no screen required).

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

## Files

| Path | Contents |
| --- | --- |
| `~/.config/quark/config.json` | settings + games (readable, hand-editable JSON) |
| `~/.local/share/quark/artwork/<id>/` | downloaded artwork (no network request while displaying) |
| `~/.local/share/quark/prefixes/<id>/` | automatically created Proton prefixes |
| `~/.local/share/quark/logs/<id>/session.log` | last session: command, environment, game and Wine output |
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
tar -xzf quark-0.7.0-linux-x86_64.tar.gz
cd quark && ./quark
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

When **Settings → Appearance → Zone de notification** is enabled, closing the window no
longer exits: Quark keeps running with a tray icon
([StatusNotifierItem](https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/)).

- left click / *Ouvrir Quark* → the window comes back (it was minimised, playtime counting
  continues);
- *Quitter Quark* → the process really exits (running sessions are still credited).

The icon is drawn by Quark itself (no icon theme needed). If your desktop exposes no
StatusNotifier watcher — GNOME without the AppIndicator extension, a bare session — the
settings page shows the D-Bus error and the close button simply quits, so you can never get
stuck with a window you cannot reach.

## Rendering check (no screen, no GPU)

Quark can render its own interface off-screen and write a PNG of it: egui tessellates the
UI in memory and Quark rasterises the triangles in software. That is how the interface is
checked after every change, including on a machine without a display server.

```bash
target/release/quark --ui preview.png                        # 1360x860, current config
target/release/quark --ui grid.png --view grid --demo        # built-in demo library
target/release/quark --ui hover.png --demo --hover 160,340   # pointer over a row
target/release/quark --ui anim.png --demo --frames 3         # mid-transition frame
target/release/quark --ui dialog.png --demo --dialog settings
target/release/quark --ui logs.png --demo --dialog logs
```

Options: `--size WxH`, `--scale N`, `--frames N`, `--view list|grid`, `--tab library|store`,
`--theme amoled|graphite`, `--select <game>`, `--info`, `--running`,
`--dialog none|add|artwork|exe|scan|settings|settings-defaults|settings-proton|settings-artworks|game|logs|customize|remove|updates`, `--hover X,Y`,
`--click X,Y`, `--key up|down|left|right|enter|escape`, `--marked`, `--demo`.

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
src/app.rs          application state, egui loop, launching, playtime, toasts, ProtonDB, tray
src/ui_library.rs   Library tab: list/grid column + game pane (banner, Play, news, ProtonDB)
src/ui_store.rs     Store tab (placeholder)
src/ui_dialogs.rs   adding a game, global settings, game settings, session log, pickers
src/config.rs       data model, persistence (JSON), labels (playtime, dates, monogram)
src/artwork.rs      Steam/SteamGridDB search + artwork download and cache
src/scan.rs         game discovery inside a folder (batch adding)
src/news.rs         per-game news (public Steam feed) and opening links
src/protondb.rs     ProtonDB rating (public API), tier colours and labels
src/session.rs      launching, stdout/stderr capture, Stop (wineserver), log tail
src/tray.rs         notification-area icon (StatusNotifierItem via ksni)
src/images.rs       texture cache: Quark decodes artwork itself
src/proton.rs       Proton runtime detection + launch command (options, umu, anti-cheat)
src/update.rs       latest GitHub release, version comparison, archive download
src/theme.rs        AMOLED/Graphite palette, gradients, monogram colours
src/widgets.rs      hand-drawn vector icons and buttons (no icon font)
src/snapshot.rs     off-screen software renderer (--ui), for visual checks
```

## Possible next steps

- Fill the Store tab (search, game pages, prices).
- Extra views: full-screen grid, recently played, favourites.
- Automatic import of the Steam library (`appmanifest_*.acf`).
- A “verify prefix” action and per-game backup of save files.
