# Troubleshooting

> [Home](Home) · [Installation](Installation) ·
> [Proton and Wine](Proton-Wine-and-Launch-Options) · **Troubleshooting**

Before changing a prefix or deleting a file, back up the folder involved. Most
problems are diagnosed with a log, not by resetting the library.

## Check the environment

```bash
quark --doctor
```

This command checks the local elements Quark needs: display, tools and paths. It
changes nothing.

For a launch outside the interface:

```bash
quark --search "Hollow Knight"
quark --news 367520
```

## A game does not start

1. Open the game page and check **Game settings**.
2. Check that the executable still exists. A moved or uninstalled game leaves an
   entry with a dead path; Quark shows it in red.
3. Check the Proton runtime. **Automatic** chooses an installed runtime; if none
   is detected, install Proton in Steam or place a build in
   `compatibilitytools.d/`.
4. Enable **PROTON_LOG** and launch again.
5. Read the log in `~/.local/share/quark/logs/<game>/`.
6. Try another Proton runtime before touching the prefix.

### Common errors

| Message | Likely cause | Action |
| --- | --- | --- |
| `Executable not found` | Path moved or game uninstalled | Fix the executable on the game page. |
| `Runtime ... not found` | Proton runtime deleted | Choose another runtime or reinstall Proton. |
| `umu-run not found` | umu-launcher missing | Install umu or uncheck the option. |
| `gamemoderun is not installed` | GameMode missing | Install GameMode or uncheck the option; the game still starts. |
| `gamescope is not installed` | gamescope missing | Install gamescope or uncheck the option. |
| Anti-cheat runtime not found | BattlEye/EAC runtime missing | Install the matching Steam runtime, then detect again. |

A missing graphics tool is reported and ignored: Quark continues without it
rather than failing. A missing Proton runtime, however, prevents the launch.

## Black screen, crash or return to desktop

- Enable `PROTON_LOG=1` and read the last lines of the log.
- Try another Proton runtime.
- Try `PROTON_USE_WINED3D=1` only as a compatibility test: if the game starts,
  the problem is probably DXVK/VKD3D or the driver.
- Update the graphics driver.
- Check whether the game needs an extra Windows DLL or component. Quark does not
  run `winetricks` automatically.
- Do not delete the prefix immediately: move it first so you can roll back.

## Controller, sound, mouse

- **Controller**: check udev/Steam Input, try without gamescope, read the log. A
  game that uses Steam Input may require launching through Steam.
- **Sound**: try `PULSE_LATENCY_MSEC=60`, check the system's default device, then
  test without a wrapper.
- **Captured mouse**: try the game's windowed mode, `-windowed` in the arguments,
  or disable gamescope.
- **Wrong GPU**: uncheck PRIME on a single-GPU desktop, or check `DRI_PRIME` on
  an AMD laptop.

## The interface does not open

- Check that `DISPLAY` or `WAYLAND_DISPLAY` is set.
- Try X11/XWayland if the Wayland compositor refuses to create the window.
- Check OpenGL and the drivers.
- Run `quark --doctor`.
- Read the terminal output: a font or graphics-context loading error appears
  there.

## Language problem

- The interface language is changed in **Settings → Appearance → Interface
  language**.
- The Steam page language is a separate setting in **Stores & tools**.
- Game names, tags and personal genres are not translated.
- If a text stays in French after switching to English, it is probably user data
  or a newly added string without a catalogue entry. Note the page and exact text
  to report it.

## The random hero background does not appear

- The library needs at least one banner artwork.
- The background is drawn behind Home and Library; it can be subtle under cards
  and the readability veil.
- The pick changes on restart, not on every navigation.
- Increase the blur or check that the option is enabled in **Appearance**.

## Settings

Since 1.2, settings fill the window and scroll. If the **Close** button is not
visible, `Escape` closes the page. If content looks cut off, resize the window:
the layout is responsive but some blocks need a minimum width.

## Legendary, gogdl and accounts

- **Legendary not found**: check `python3`, then reinstall the tool in **Stores &
  tools**. Quark verifies the SHA-256 before running it.
- **Epic login refused**: the authorization code is time-limited; start the
  connection again.
- **GOG session expired**: reconnect GOG; the token is in a separate file with
  private permissions.
- **A store game does not appear**: run a synchronization and check that the
  account is connected and the game is owned.
- **Download stuck**: cancel and start again; check disk space and the
  installation folder.

## Cloud saves

- **Epic**: `legendary sync-saves` compares local and cloud, the most recent
  wins. A custom folder may be required.
- **GOG**: gogdl requires an explicit save folder. Without it, synchronization
  cannot work.
- **Steam**: Steam Cloud remains managed by Steam; Quark does not synchronize it.
- **Conflict**: back up the local folder before letting the cloud overwrite
  anything. Quark never invents a missing save.

## Ludusavi snapshots

- **No game detected**: check the Ludusavi configuration and the save folder.
- **Restore refused**: symbolic links, `$HOME` as a root and paths outside
  allowed roots are deliberately refused.
- **Wrong snapshot restored**: each store installation has its own compartment;
  check the active installation.
- **Unreadable configuration**: Quark keeps it and reports the error; do not
  overwrite it with an empty configuration.

## Update blocked

- **Development binary**: automatic updates are ignored.
- **Read-only folder**: download the archive and replace the binary manually.
- **Game running**: close the game before installing.
- **Invalid checksum**: the archive is refused. Download the archive and
  `.sha256` again from the release.
- **Old binary kept**: Quark keeps the previous executable until the next
  successful start.

See [Installation](Installation) for the manual procedure.

## Configuration file

`~/.config/quark/config.json` contains games, installations, settings and
statistics. If it is corrupted:

- Quark copies the file to `config.json.corrompu-<timestamp>`;
- the library switches to read-only so data is not overwritten;
- an empty file is never written in its place.

Do not delete this file to solve a launch problem: the problem is almost always
in the prefix, runtime or game files, not in the library.

## Logs and privacy

Session logs are in `~/.local/share/quark/logs/<game>/`, mode `0600`. They do not
contain full environment values, complete command lines or the achievement
token. Before sharing a log, read it again: a game may write personal paths
there.

## Report a bug

Include:

- the Quark version (visible in **About**);
- the distribution, X11/Wayland session and driver version;
- the Proton runtime and enabled options;
- the last lines of the Proton log;
- reproducible steps.

Do not attach `credentials.json`, `user.json`, `gogdl-auth.json` or an API token.
Those files contain account secrets.

## See also

- [Installation](Installation)
- [PlayStation 2 and RetroAchievements](PlayStation-2-and-RetroAchievements)
- [Proton, Wine and launch options](Proton-Wine-and-Launch-Options)
- [Stores and saves](Stores-and-Saves)
- [Overlay and achievements](Overlay-and-Achievements)

---

[Home](Home) · [Installation](Installation) · [Proton](Proton-Wine-and-Launch-Options) · [Stores](Stores-and-Saves)
