# Library and views

> [Home](Home) · [Getting started](Getting-Started) · **Library** ·
> [Stores](Stores-and-Saves) · [Settings](Settings)

![Library in list view](https://raw.githubusercontent.com/Matheotaku4/Quark-Launcher/main/docs/images/library-fr.png)

## Two layouts

The library can be shown as a **list** or a **grid**. The choice is stored in
the configuration and can be changed with the two top-bar icons or in
**Settings → Appearance → Library layout**.

- **List**: one row per game, with a square icon, name, playtime, last use,
  status dot and selection marker. Groups can be collapsed.
- **Grid**: covers only, for browsing a large library. Clicking opens the game
  page as an overlay.

Games whose executable is missing are shown in red. A game marked as running
keeps that state visible after switching pages.

## Search

- `Ctrl+F` or `/` focuses the search field.
- `Escape` clears the search.
- Search ignores accents and compares the title, developer, tags and genres
  separately. A query cannot match by scattering its letters across fields.
- The **Filters** button opens one panel for status, Steam genre and personal
  tags. The number of active filters appears on the button, and **Clear all**
  removes them at once.

## Sort, filter, group

**Sort**: name, last session, date added or playtime.

**Filters**: the funnel button above the list groups status (`To play`,
`Playing`, `Completed`, `Dropped`), Steam genre and personal tag. Filters
combine, stay open while selecting and show their count on the button. **Clear
all** restores the full library. Free-text search stays in the top bar.

List **grouping**:

- by **status**;
- by **store**;
- by **genre**;
- by **release year**;
- or no group, for one continuous list.

Group headers show a count and can be collapsed. Grouping does not change the
order of games inside a group.

## Saved views

The **Views** menu saves the current state: search, filters, sort and grouping.
A view can be applied or deleted later.

Three starter views are created once: **To play**, **Playing** and
**Completed**. If you delete them, they are not recreated on the next start.
Empty views, duplicates and filters pointing to a deleted tag or genre are
cleaned on load and save. The list is limited to 32 views.

## Home

Home shows a carousel of recently played games, then the library as covers. The
carousel can advance automatically; the delay is set in
**Settings → Appearance → Home**.

The main button becomes **Stop** while the game is running. Playtime, last
session, ProtonDB rating and the native Linux badge are visible on the banner.

**Random hero background**: when enabled, Quark picks a banner from the library
at startup, blurs it and draws it behind Home and Library. The radius is set
from 2 to 30 px. The pick is made once per session; restarting Quark changes the
image. See [Themes and appearance](Themes-and-Appearance).

## Game page

The game page shows:

- the banner, logo or title and the ProtonDB/Linux badges;
- the **Play/Stop** button, playtime and last session;
- the **News**, **Achievements** and **History** tabs;
- the **Logs**, **Information**, **Customize** and **Game settings** actions.

**History** summarizes sessions, shows the last twelve weeks and lists recent
play sessions. Playtime published by Epic or GOG replaces local playtime when
the store provides it; Steam Cloud remains managed by Steam.

**Customize** replaces or removes the cover, hero, logo and icon from
SteamGridDB, Steam, the store or a local file.

## Several installations for one game

A game has shared data (name, artwork, notes, tags, status, statistics) and one
or more **installations**. Each installation carries its store, folder,
executable, prefix, cloud save folder and install state.

- Selecting an installation changes the executable and prefix used by **Play**
  without touching the other editions.
- When an older `~/.local/share/quark/prefixes/<game>` prefix already exists, it
  is reused so saves do not become invisible.
- A real second installation gets a `<game>-<store>` prefix.
- Ludusavi snapshots are separated by installation: two editions of the same
  game never share a snapshot.

## Multi-selection and removal

`Ctrl+click` marks several games. The delete button opens a confirmation for the
selection. Removing an entry does not delete the game files: it only removes the
library entry. Prefixes, saves and installations stay on disk unless you
explicitly remove them in the relevant tools.

## Command palette

`Ctrl+K` opens a palette that searches pages, actions and games.

- Type to filter; accents are ignored.
- `↑`/`↓` move the selection, `Enter` opens, `Escape` closes.
- Actions depend on context: open settings, switch page, launch a game or
  refresh a store.

## Library shortcuts

| Key | Action |
| --- | --- |
| `↑` / `↓` | Select the previous or next game |
| `Enter` | Launch the selected game |
| `Ctrl+F` or `/` | Search |
| `Escape` | Clear search or close a panel |
| `Ctrl+K` | Command palette |
| `Ctrl+Q` | Quit Quark |

## Going further

- [Stores and saves](Stores-and-Saves)
- [PlayStation 2 and RetroAchievements](PlayStation-2-and-RetroAchievements)
- [Proton, Wine and launch options](Proton-Wine-and-Launch-Options)
- [Settings](Settings)

---

[Home](Home) · [Getting started](Getting-Started) · [Stores](Stores-and-Saves) · [Settings](Settings)
