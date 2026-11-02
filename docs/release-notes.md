## Quark 1.1.1 — settings tabs, reliable local saves and the Epic overlay

### Settings tabs are back at the top

- The sidebar introduced in 1.1.0 is gone. Appearance, In game, New games,
  Artworks, Stores & tools, Statistics, Disk space and About are a single row
  again, with wrapping on narrow windows.
- The scroll area no longer runs underneath the **Close** button.

### Local saves with Ludusavi

- Quark now passes `--wine-prefix <prefix>/pfx` to `ludusavi backup` and
  `ludusavi backup --preview`. Without it, Ludusavi resolved
  `<winDocuments>` and `<winLocalAppData>` in the wrong prefix and reported no
  files for Epic/Proton games.
- The game title is resolved through `ludusavi find --api --normalized` before
  backing up. Edition names such as **Dying Light: Enhanced Edition** now map
  to the canonical **Dying Light** manifest entry.
- The internal `[steam:…]`/`[epic:…]`/`[gog:…]` suffix used to keep duplicate
  installations separate is removed only for the manifest lookup, so custom
  game entries keep working.
- A title that is genuinely absent from the manifest now says so and points to
  the **Local save folder…** action instead of silently skipping the backup.
- Epic's `save_path` (when Legendary has one) is kept as a custom Ludusavi path,
  which covers titles whose local save folder is already known.

### Epic authentication and the EOS overlay

- Epic games are no longer started with only their bare executable. Quark asks
  Legendary for the launch parameters (`legendary launch --json
  --skip-version-check`), then passes the game parameters and the ephemeral
  `-AUTH_*`/`-epicapp`/`-EpicPortal` arguments to the game. These are the
  parameters the Epic client expects, so **“impossible de se connecter à Epic
  Games”** no longer comes from launching a bare executable.
- Those parameters are added inside `gamescope`, `gamemoderun` and custom
  wrappers, not after them, so a wrapper cannot swallow the authentication.
  Failure output from Legendary is not copied into an error message because it
  can contain a session token.
- **Settings → Stores & tools → Overlay Epic (EOS)** installs the overlay once
  with `legendary eos-overlay install`. It is then enabled per game prefix with
  `legendary eos-overlay enable --prefix`, automatically before an Epic launch
  and on demand from the game panel. This mirrors Heroic's behaviour and makes
  the Epic overlay available in game instead of only Quark's overlay.
- The managed download never runs without an Epic session and reports its
  progress in the settings page.

### Verification

- `cargo test --offline`: **246 tests passed, 0 failed**.
- `cargo build --release --locked --offline`: passed.
- Settings pages were rendered off-screen at 1360×860: tabs stay on one row and
  the footer no longer overlaps the content.
- The new tests cover Epic launch parameters surrounded by log lines, EOS
  install/enable commands, normalized title resolution, duplicate-title suffix
  removal and `--wine-prefix` placement.
- Not verified against a real Epic account: downloading the EOS overlay,
  completing Epic sign-in inside a game and launching Dying Light still need a
  desktop check. The development environment has no Epic credentials and no X11
  session.
