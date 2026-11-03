## Quark 1.1.4 — automatic updates at startup

### Updates install themselves

- Quark can now check the latest GitHub release when it starts, download the
  verified archive, replace the executable it was launched from and relaunch
  itself. The option is enabled by default and can be turned off under
  **Settings → About**.
- Before replacing anything, Quark verifies the published SHA-256, extracts
  only `quark/quark`, checks that it is an x86_64 ELF binary and confirms its
  `--help` version matches the release tag.
- The previous executable is renamed next to the new one, so a failed `exec`
  restores it automatically. The backup is removed after the next successful
  start. Read-only installs, development builds and `QUARK_NO_AUTO_UPDATE=1`
  never update automatically, and a running game blocks the install.
- The About page now describes the manual update flow and the automatic
  startup option; the archive remains available for a manual fallback.
- This first 1.1.4 install is still manual: 1.1.3 and earlier do not contain
  the updater. Once 1.1.4 is running, later releases install automatically.
