## Quark 1.1.0 — one library, cloud saves and a real overlay

### One game, several installations

Quark now models a game separately from the editions installed on disk. A
`Game` owns the shared data (artwork, notes, tags, status, playtime and
sessions); each `Installation` owns its store source, product ID, install
folder, executable, Proton prefix and save-sync state.

- Existing configurations migrate automatically: the old executable becomes a
  local installation, and every linked Steam/Epic/GOG record becomes an edition
  of the same game.
- The migration is idempotent and runs before each save, so an interrupted
  write cannot create duplicate editions.
- The game information panel lists the editions. Selecting one changes the
  executable and prefix used by **Play** without touching the other edition.
- A Steam copy and a GOG copy can now coexist with separate prefixes and saves,
  while sharing one cover, one notes page and one playtime history.
- Legacy configuration fields remain readable and writable for compatibility.

### Epic and GOG cloud saves

- **Epic:** `legendary -y sync-saves <app> --accept-path` compares local and
  remote saves and keeps the newer side. A custom folder is passed with
  `--save-path` when configured.
- **GOG:** `gogdl save-sync <folder> <game-id> --ts <timestamp>` is used with
  the folder selected in the game panel. The timestamp returned by gogdl is
  stored per installation and reused for the next comparison.
- Synchronization runs after a successful Epic/GOG installation, after a game
  launched by Quark exits, and from the manual **Synchronize cloud saves**
  button. It runs in the background and reports its result in the game panel.
- Steam Cloud remains managed by the Steam client; Quark does not fake a
  capability it cannot verify.
- The `cloud_saves` capability is now reported as supported for Epic and GOG,
  and the cloud-save toggle lives in **Settings → In game**.

### Achievements

- The Legendary parser now accepts the plain-text achievement table in addition
  to JSON. This fixes the **“réponse de Legendary illisible (aucun JSON)”**
  error on versions or games that do not return JSON.
- If the Epic metadata cache is incomplete, Quark refreshes it with
  `legendary info` and retries the achievements command once.
- Parser failures distinguish a missing session, a game without achievements
  and a genuinely malformed response instead of showing a generic unreadable
  response.
- JSON with log lines around it, null lists and rarity/progress fields remains
  supported.

### Steam-like in-game overlay

- The old passive HUD is replaced by a full-screen overlay opened with
  **`Shift+Backspace`** and closed with the same shortcut, `Esc` or the close
  button.
- The panel shows the game, store, session clock, achievement progress and the
  achievement list with icons, descriptions, rarity and progression.
- Display options are in **Settings → In game**: show locked achievements,
  compact rows, progress/rarity, overlay shortcut and screenshot key.
- While closed, the overlay stays mouse-transparent and only shows short
  achievement/capture toasts. `F12` still captures the screen.
- The X11 shortcut handles Shift, Ctrl, Alt and Super combinations, including
  Caps Lock and Num Lock states. Changing a shortcut releases the previous
  grab instead of leaking a thread.

### Settings redesign

- The settings window now uses a persistent sidebar with five groups:
  **Interface**, **Library**, **Accounts**, **Data** and **Quark**.
- Overlay, captures, achievements and cloud saves have their own **In game**
  page instead of being buried in Appearance.
- The content is width-limited, with clearer section headings and a fixed close
  action; the previous row of wrapping tabs is gone.

### Documentation

- The README is reorganized into chapters with a table of contents, installation
  instructions, a model overview, store/cloud-save details, overlay usage,
  settings reference, command line, file locations, project layout and known
  limitations.
- `docs/release-notes.md` now contains only the release being published, in
  English. Older notes remain on their existing GitHub releases instead of
  being repeated in every new release body.

### Verification

- `cargo test --offline`: **240 tests passed, 0 failed**.
- `cargo build --release --locked --offline`: passed.
- The settings window, library and demo library were rendered off-screen with
  `--ui --demo`; the new **In game** settings page has a CI preview.
- The migration, installation selection, Legendary text fallback, GOG timestamp
  parser and overlay shortcut parser have dedicated tests.
- Real Epic/GOG accounts, a real X11 session and actual game launches are not
  available in the development environment. Store credentials, cloud-save
  round trips and the global overlay shortcut still require a manual check on a
  desktop with accounts. Steam achievements remain unavailable without the
  Steam client.
