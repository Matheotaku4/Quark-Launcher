# Settings

> [Home](Home) · [Themes and appearance](Themes-and-Appearance) ·
> **Settings** · [Proton and Wine](Proton-Wine-and-Launch-Options)

![Settings](https://raw.githubusercontent.com/Matheotaku4/Quark-Launcher/main/docs/images/settings-en.png)

Since 1.2, settings fill the whole window, with side navigation and separate
sections. Every change is saved automatically; there is no **Apply** button.

## Appearance

- **Interface language**: French or English, applied immediately.
- **Theme**: thirteen palettes, applied immediately.
- **Random hero background** and **Blur**: a blurred banner behind Home and
  Library, picked once per session.
- **Startup**: startup page (Home, Library, Store, My games).
- **Library**: list or grid layout.
- **Home**: carousel sort, automatic advance and interval.
- **Notification area**: keep Quark in the tray icon instead of quitting.

Details and screenshots: [Themes and appearance](Themes-and-Appearance).

## In game

- **Overlay**: enable it, choose the global shortcut and show locked, compact
  achievements with progress and rarity.
- **Screenshots**: capture key and notifications.
- **Cloud saves**: Epic/GOG synchronization after installation and on exit.
- **Epic overlay (EOS)**: download and enable the EOS overlay in the game prefix
  when available.

The defaults are `Shift+Backspace` for the overlay and `F12` for screenshots.
See [Overlay and achievements](Overlay-and-Achievements).

## New games

This page does **not** contain global settings. These values are copied into a
game when it is added. Each game then keeps its own; changing a default does not
affect existing games, and changing one game does not affect the others.

- **Proton prefix**: folder used as-is. Empty = automatic prefix under
  `~/.local/share/quark/prefixes/<game>`.
- **Store installation folder**: parent proposed for Epic/GOG. Empty = `~/Games`.
- **Arguments**: default arguments passed after the executable.
- **Proton runtime**: preferred runtime; **Automatic** lets Quark choose.
- **Anti-cheat**: BattlEye and EasyAntiCheat, two independent options.
- **Launch options**: GameMode, MangoHud, gamescope, PRIME, wrapper, PROTON_LOG,
  umu.
- **Environment variables**: format `NAME=value`, copied into each new game.

Detected runtimes and their count are shown on this page, with **Detect again**.
See [Proton, Wine and options](Proton-Wine-and-Launch-Options) for what each
option and variable does.

## Artwork

- **Sources**: SteamGridDB, Steam, store and local file.
- **SteamGridDB key**: optional, stored in the local configuration.
- **Download and replace** covers, heroes, logos and icons.
- **Cache**: downloaded images are kept in `~/.local/share/quark/artwork/`.

A game page also offers **Customize** to choose a specific image, including a
local file.

## Stores & tools

- **Synchronization**: import or refresh the Steam, Epic and GOG libraries.
- **Accounts**: connect and disconnect Epic/GOG, optional Steam key.
- **Connections**: session state and synchronization actions.
- **Tools managed by Quark**: Legendary, gogdl, Ludusavi; install, update and
  SHA-256 verification.
- **Epic overlay (EOS)**: download and activation.
- **PlayStation 2 · PCSX2**: emulator binary, automatic detection and Flatpak
  export.
- **RetroAchievements**: username and Web API key used to read PS2 progress.

See [Stores and saves](Stores-and-Saves) and
[PlayStation 2 and RetroAchievements](PlayStation-2-and-RetroAchievements).

## Statistics

- Total playtime, playtime this week and day streak.
- Bars for the last twelve weeks.
- A 26-week heat map.
- Most played games.

Statistics come from the local session history. Playtime published by a store
can be shown on the game page, but the detailed history remains what Quark
observed.

## Disk space

Background measurement of:

- Proton prefixes;
- artwork cache;
- logs;
- update archives;
- game folders.

The measurement can take time on a large library. It deletes nothing and offers
no automatic cleanup: the folder button opens the location so you can decide
what to remove.

## About

- Installed version.
- **Check for updates**, last check state, archive size and installation.
- **Install updates on startup**.
- **Review the getting-started guide**: reopen the welcome guide.
- **Review what's new**: reopen the version summary.
- **Release page**, resources and data folders.

Updates verify the SHA-256 and refuse a binary that does not match the expected
tag. See [Installation](Installation).

## Close settings

The **×** button in the top right closes the page. `Escape` also closes panels.
Settings do not interrupt an already started download, disk scan or
synchronization.

## See also

- [Themes and appearance](Themes-and-Appearance)
- [Proton, Wine and launch options](Proton-Wine-and-Launch-Options)
- [Stores and saves](Stores-and-Saves)
- [PlayStation 2 and RetroAchievements](PlayStation-2-and-RetroAchievements)
- [Troubleshooting](Troubleshooting)

---

[Home](Home) · [Themes](Themes-and-Appearance) · [Proton](Proton-Wine-and-Launch-Options) · [Troubleshooting](Troubleshooting)
