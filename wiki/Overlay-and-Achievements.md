# Overlay, screenshots and achievements

> [Home](Home) · [Stores](Stores-and-Saves) · **Overlay and achievements** ·
> [Settings](Settings) · [Troubleshooting](Troubleshooting)

## In-game overlay

Launch a game from Quark, then press **`Shift+Backspace`**. A full-screen panel
appears above the game with:

- the game name and store;
- the session timer;
- achievement progress;
- the achievement list, locked or not depending on the setting;
- rarity and progress when the store publishes them;
- an indicator for recent screenshots.

The same shortcut, `Escape` or the close button dismisses the overlay.

The overlay is not a window that constantly captures clicks: when closed, it
stays transparent to the mouse. It is only offered for a game launched by Quark,
because Quark must know the process and session.

### Settings

**Settings → In game**:

- enable or disable the overlay;
- choose the global shortcut;
- show or hide locked achievements;
- use compact rows without descriptions;
- show or hide progress and rarity.

A global X11 helper intercepts the shortcut. Proton games run through XWayland
and use the same path. On native Wayland, the compositor may forbid global
shortcuts: the overlay remains available through the close button and built-in
screenshots, but the shortcut may not work.

## Screenshots

**`F12`** takes a screenshot of the running game. The file is written to:

```text
~/.local/share/quark/screenshots/<game>/
```

- Epic and GOG screenshots play a short sound and show a desktop notification.
- Steam screenshots stay silent.
- The key can be changed in **Settings → In game**.
- Capture goes through X11/XWayland; on native Wayland the compositor may ask
  for permission or refuse.

Screenshots are never uploaded to a server. They remain local files.

## Epic achievements

Epic uses Legendary:

```bash
legendary achievements <app> --json
```

The parser accepts JSON surrounded by log lines, a null list and the older text
table (`Total achievements`, `Completed`, `In progress`). If the metadata cache
is incomplete, Quark runs `legendary info` and retries once. A missing session
or a game without achievements produces an explicit message instead of an
unreadable error.

## GOG achievements

GOG queries `gameplay.gog.com` with the gogdl session. Both sources are
normalized to the same rows: icon, name, description, unlock date, rarity and
progress. Icons are cached locally.

An achievement unlocked during the session can carry a **session** badge.

## PlayStation 2 and RetroAchievements

A PS2 game with a RetroAchievements ID shows its progress in the same overlay.
PCSX2 signs in and unlocks achievements itself; Quark reads the list through the
official Web API. Configure the username and Web API key in **Settings → Stores
& tools**, then link the game by title from its **PlayStation 2** tab. See
[PlayStation 2 and RetroAchievements](PlayStation-2-and-RetroAchievements).

## Local achievements API

A game or script can report an unlock through the small local server:

```bash
curl -X POST "$QUARK_ACHIEVEMENTS_URL" \
  -H "Authorization: Bearer $QUARK_ACHIEVEMENTS_TOKEN" \
  -d '{"game":"'"$QUARK_GAME_ID"'","achievement":"ACH_FIRST","name":"First step"}'
```

- The server listens only on `127.0.0.1`.
- The token changes on every launch and is masked in the session log.
- The launched game receives `QUARK_ACHIEVEMENTS_URL`,
  `QUARK_ACHIEVEMENTS_TOKEN` and `QUARK_GAME_ID`.
- The game must never log the token.

This API is for games that have no usable achievement source. It does not
replace official Steam achievements.

## Epic overlay (EOS)

In **Settings → Stores & tools → Epic overlay (EOS)**, Quark can download the EOS
overlay once and enable it in the game prefix. This is the mechanism Heroic
uses: the Epic overlay appears in-game with `Shift+F3`, independently of the
Quark overlay.

This feature depends on the game, prefix and EOS version. It is not required for
achievements: they are read through Legendary even without the EOS overlay.

## Steam achievements

Steam provides no readable achievement source without a Steam session. Quark
shows the Steam tab only when a usable source exists. Steam Cloud and Steam
achievements remain managed by the Steam client.

## Troubleshooting

| Problem | Check |
| --- | --- |
| The shortcut does nothing | Check that the game was launched by Quark, then the shortcut in **In game**. On native Wayland, try XWayland. |
| The overlay appears but not achievements | Check the Epic/GOG connection and metadata cache; refresh the game page. |
| `F12` does nothing | Check the key and X11/Wayland permissions; try a compositor screenshot. |
| Screenshots are not where expected | Check `~/.local/share/quark/screenshots/` and folder permissions. |
| A local achievement does not arrive | Check the URL, token and `QUARK_GAME_ID`; the server only listens on loopback. |
| EOS does not appear | Check the active prefix, reinstall the EOS overlay and test the game without the Quark overlay. |
| PS2 achievements are missing | Check the RA ID, the username and Web API key in **Stores & tools**. |

## See also

- [Stores and saves](Stores-and-Saves)
- [PlayStation 2 and RetroAchievements](PlayStation-2-and-RetroAchievements)
- [Proton, Wine and options](Proton-Wine-and-Launch-Options)
- [Settings](Settings)
- [Troubleshooting](Troubleshooting)

---

[Home](Home) · [Stores](Stores-and-Saves) · [PlayStation 2](PlayStation-2-and-RetroAchievements) · [Settings](Settings)
