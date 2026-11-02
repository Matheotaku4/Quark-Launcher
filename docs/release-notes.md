## Quark 1.1.3 — stable prefixes, umu and safer defaults

### Proton prefixes stay stable

- The default Proton prefix is now copied into a game when it is added, and
  once into existing games during migration. Changing the default later no
  longer moves an existing game's prefix or hides its saves.
- A game without its own prefix now uses the prefix of its active installation
  before falling back to `~/.local/share/quark/prefixes/<game>`.
- umu-launcher uses `WINEPREFIX` directly, while Proton keeps its `pfx/`
  subdirectory. Save detection, the EOS overlay and `wineserver -k` now use the
  real Wine prefix for both launch modes.

### Launch reliability

- An anti-cheat runtime (BattlEye/EasyAntiCheat) can no longer be selected and
  executed as if it were a Proton runtime; the launch is refused with a clear
  message instead of trying to run a data directory.
- Automatic cloud-save synchronization only starts when the provider tool and
  account are usable, so a missing session no longer produces an error after
  every game session. The manual button still explains what is missing.

### Interface and packaging

- Arrow-key navigation in the grid follows the actual number of columns (2 to
  8) instead of assuming three.
- The release archive now contains `quark.desktop` and `quark.png` as the README
  promised, with installation instructions for the application menu.
- README corrections: settings are a horizontal tab bar, `--fetch` takes a
  title rather than an appid, and `--news` accepts an appid or a title.
