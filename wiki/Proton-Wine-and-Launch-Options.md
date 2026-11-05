# Proton, Wine and launch options

> [Home](Home) · [Getting started](Getting-Started) ·
> **Proton and Wine** · [Stores](Stores-and-Saves) · [Troubleshooting](Troubleshooting)

This page describes what Quark actually does when you click **Play**, the order
of the wrappers, every option on the game page and the useful environment
variables. Examples are starting points: a game may need a different setting.

## Wine, Proton and umu: the right words

| Term | Role |
| --- | --- |
| **Wine** | Compatibility layer that runs a Windows program on Linux. |
| **Proton** | Distribution of Wine, DXVK, VKD3D and patches designed for Steam. Quark runs the `proton` script of an installed runtime. |
| **Prefix** | Folder containing a fake Windows drive (`drive_c`), the registry and installed components. A game's saves and settings often live there. |
| **Runtime** | Proton version installed in Steam (`Proton 9.0`, `Proton Experimental`, a `compatibilitytools.d` build, Flatpak or Snap). |
| **umu-launcher** | Launcher that applies umu database patches and can use UMU-Proton without Steam. |
| **DXVK** | Translates Direct3D 9/10/11 to Vulkan. |
| **VKD3D-Proton** | Translates Direct3D 12 to Vulkan. |

Quark does **not** replace a system Wine. Normal mode uses a detected Proton
runtime or umu. A wrapper can call `wine` manually, but that is an advanced case:
Quark then manages neither the prefix, Proton variables nor anti-cheat runtimes.

## How Quark chooses the launch mode

1. **PlayStation 2**: the game has a PS2 configuration, so Quark launches PCSX2.
2. **Native**: the game is explicitly marked native.
3. **Specific Proton runtime**: the game uses that runtime.
4. **Automatic**: Quark looks at the executable extension. An `.exe`, `.bat` or
   `.msi` goes through Proton; a Linux binary stays native.

If **umu-launcher** is checked on a Windows game, Quark launches `umu-run`
instead of the Proton script. On a native game, umu is ignored with a warning.

### Command order

From outside to inside:

```text
user wrapper
  └─ gamescope (if checked)
      └─ gamemoderun (if checked)
          └─ proton run <game> <arguments>   (or umu-run)
```

The wrapper is a shell line: `%command%` is replaced by the complete command,
already wrapped. Without `%command%`, the command is simply prefixed.

Game arguments go after the executable, inside gamescope and GameMode. Store
parameters (Epic) are added after the game arguments and are not changed by
Quark.

## Game page options

Open the game page, then **Game settings** (or the gear icon).

### Runtime and prefix

- **Proton runtime**: choose the build. **Automatic** lets Quark decide.
- **Prefix**: folder used for this game. Quark uses it as-is and creates no
  subfolder. Empty creates `~/.local/share/quark/prefixes/<game>`.
- **umu-launcher**: launch through `umu-run`.
- **umu GAMEID**: umu patch identifier. Empty becomes `0`.
- **Anti-cheat**: BattlEye or EasyAntiCheat. Quark finds the matching Steam
  runtime and passes it to Proton.

Changing the prefix is like changing the "Windows machine": components, the
registry and sometimes saves do not follow. Back up before moving a prefix.

### Graphics and system options

| Option | What Quark runs | Effect |
| --- | --- | --- |
| **GameMode** | `gamemoderun <command>` | Asks the GameMode daemon to apply its performance profile. |
| **MangoHud** | `MANGOHUD=1` | Enables the MangoHud Vulkan/OpenGL layer; the package must be installed. |
| **gamescope** | `gamescope -W <width> -H <height> -f [ -F fsr ] [--framerate-limit N] -- <command>` | Game compositor, resizing, FSR and frame limit. |
| **gamescope resolution** | width × height | Window/compositor size. |
| **gamescope FSR** | `-F fsr` | Enables gamescope FSR upscaling. |
| **Frame limit** | `--framerate-limit N` | 0 disables the limit; the value is capped at 1000. |
| **PRIME** | NVIDIA/AMD variables below | Forces rendering on a hybrid laptop's discrete GPU. |
| **PROTON_LOG** | `PROTON_LOG=1`, `PROTON_LOG_DIR=...` | Writes a Proton log in the game log folder. |
| **Wrapper** | `sh -c "<wrapper>"` | Free command around the game. |

**GameMode** and **MangoHud** are independent: GameMode sets performance, MangoHud
shows measurements. **gamescope** forces fullscreen with `-f`; for a window, do
not use the checkbox and use a custom gamescope wrapper.

## Variables Quark sets automatically

You normally do not need to add these. If you also put them in your variables,
the values Quark requires are appended after yours and win.

| Variable | When | Role |
| --- | --- | --- |
| `WINEPREFIX` | umu only | Prefix passed to umu. |
| `STEAM_COMPAT_DATA_PATH` | Proton | Compatibility folder (the prefix) used by the Proton script. |
| `STEAM_COMPAT_CLIENT_INSTALL_PATH` | Proton | Steam root, so Proton can find its components. |
| `GAMEID` | umu | umu identifier; `0` when empty. |
| `PROTONPATH` | umu with pinned runtime | Chosen Proton folder. |
| `PROTON_LOG` / `PROTON_LOG_DIR` | PROTON_LOG checkbox | Proton log in the Quark logs. |
| `MANGOHUD` | MangoHud checkbox | Enables MangoHud. |
| `PROTON_BATTLEYE_RUNTIME` | BattlEye anti-cheat | BattlEye runtime folder. |
| `PROTON_EAC_RUNTIME` | EasyAntiCheat anti-cheat | EAC runtime folder. |
| `__NV_PRIME_RENDER_OFFLOAD`, `__GLX_VENDOR_LIBRARY_NAME`, `__VK_LAYER_NV_optimus`, `DRI_PRIME` | PRIME checkbox | NVIDIA and AMD offload. |

User variables are deduplicated case-insensitively: the last valid value for a
name wins.

## Useful variables to add by hand

In **Game settings → Environment variables**, add one line per variable. Values
are case-sensitive. Do not wrap values in quotes inside Quark fields.

### Graphics and rendering

| Variable | Typical value | What it changes |
| --- | --- | --- |
| `DXVK_HUD` | `fps,frametimes,gpuload` | Shows a DXVK HUD: FPS, frame times and GPU load. `compiler` adds shader compilation, `memory` memory. |
| `DXVK_CONFIG_FILE` | `/path/dxvk.conf` | Loads a DXVK configuration file (per-game options). |
| `VKD3D_CONFIG` | `dxr` | Enables D3D12 ray tracing when the VKD3D build and driver support it. `dxr11` for the DXR 1.1 variant depending on the build. |
| `VKD3D_FEATURE_LEVEL` | `12_1` | Forces the exposed D3D12 feature level. Too high can prevent a game from starting. |
| `PROTON_USE_WINED3D` | `1` | Replaces DXVK with wined3d (OpenGL). Fallback compatibility, usually slower. |
| `WINE_FULLSCREEN_FSR` | `1` | Enables Wine's FSR upscaling for compatible fullscreen games. |
| `WINE_FULLSCREEN_FSR_STRENGTH` | `2` | FSR sharpness from 0 (max) to 5 (min). |
| `MANGOHUD_CONFIG` | `fps_only=1` | MangoHud options when `MANGOHUD=1` is active. |
| `mesa_glthread` | `true` | Enables Mesa OpenGL threading for OpenGL/wined3d games. |
| `__GL_SHADER_DISK_CACHE` | `1` | NVIDIA shader cache on disk. |
| `__GL_SHADER_DISK_CACHE_SKIP_CLEANUP` | `1` | Avoids cleaning the NVIDIA cache; useful for games with long shader compilation. |

### Synchronization and performance

| Variable | Value | What it changes |
| --- | --- | --- |
| `PROTON_NO_ESYNC` | `1` | Disables esync. Try it if the game hangs at startup. |
| `PROTON_NO_FSYNC` | `1` | Disables fsync. Same use, as a last resort. |
| `WINEESYNC` | `0` or `1` | Controls esync when the build supports it; Proton usually sets it itself. |
| `WINEFSYNC` | `0` or `1` | Controls fsync when the build supports it. |
| `PULSE_LATENCY_MSEC` | `60` | Raises PulseAudio latency to avoid crackling. |
| `PROTON_FORCE_LARGE_ADDRESS_AWARE` | `1` | Gives 32-bit games more address space; can fix memory issues, use carefully. |
| `SteamDeck` | `0` or `1` | Some games switch UI or profile based on this flag. `1` can enable an unwanted handheld profile on desktop. |

### Compatibility and diagnostics

| Variable | Value | What it changes |
| --- | --- | --- |
| `WINEDLLOVERRIDES` | `dll=n,b` | Forces the native/builtin order of a DLL. `n,b` loads the native DLL then Wine's; `b` forces Wine's. Incorrect syntax can prevent a game from starting. |
| `WINEDEBUG` | `-all` | Turns off Wine debug messages. Useful when logs are huge. |
| `WINEARCH` | `win64` | Architecture of a **new** prefix. Do not change it on an existing prefix: it can make it unusable. |
| `PROTON_LOG` | `0`/`1` | Same effect as the checkbox, but can be set in the wrapper. |
| `PROTON_LOG_DIR` | `/path` | Destination of the Proton log. |
| `PROTON_ENABLE_NVAPI` | `1` | Enables NVAPI on Proton builds that require it, for DLSS/Reflex. Useless or harmful depending on the version. |
| `PULSE_SERVER` | socket | Forces a specific PulseAudio/PipeWire server for unusual audio setups. |

### umu-launcher

| Variable | Value | What it changes |
| --- | --- | --- |
| `GAMEID` | `0` or `umu-<id>` | Identifier used by umu to choose patches. Leave `0` if the game is not in the database. |
| `PROTONPATH` | `/path/Proton` | Forces the Proton runtime used by umu. Quark sets it when a runtime is pinned. |
| `UMU_LOG` | `1` | Detailed umu log, useful when `umu-run` fails before Proton. |

### Anti-cheat

BattlEye/EasyAntiCheat runtimes are Steam folders, not complete Proton versions.
Quark passes them through `PROTON_BATTLEYE_RUNTIME` or `PROTON_EAC_RUNTIME`. A
protected game can still refuse to work: support depends on the game, server and
runtime, not only on Quark.

## Launch arguments

Arguments are added after the executable. They depend on the game:

- `-windowed`, `-fullscreen`, `-w 1920 -h 1080`;
- `-novid` to skip some videos;
- `-dx11`, `-force-d3d11`, `-d3d12`;
- `--launcher-skip` for some recent games.

An argument containing a space must be quoted in the Quark field, for example
`"-username" "Name"`. Arguments are not variables: `FOO=1` in this field will
probably be ignored by the game. Use the Variables section for the environment.

## Wrapper examples

The wrapper is a shell line run with `sh -c`. `%command%` represents the complete
command, already wrapped by gamescope/GameMode when those boxes are checked.

```bash
# Force a DXVK HUD
env DXVK_HUD=fps,frametimes %command%
```

```bash
# Fullscreen FSR sharpness
env WINE_FULLSCREEN_FSR=1 WINE_FULLSCREEN_FSR_STRENGTH=2 %command%
```

```bash
# Manual gamescope in a 1600x900 window
gamescope -W 1600 -H 900 -f -- %command%
```

```bash
# Redirect game output to a file
%command% > "$HOME/quark-game.log" 2>&1
```

**Do not combine** a `gamescope` wrapper with the gamescope checkbox: you would
get two nested compositors. Similarly, `gamemoderun` in the wrapper and the
GameMode checkbox stack.

`%command%` already contains potentially spaced arguments. If you build a
complex command, quote paths and test without a wrapper first.

## gamescope in detail

The gamescope checkbox produces approximately:

```bash
gamescope -W 2560 -H 1440 -f -F fsr --framerate-limit 60 -- <command>
```

- `-W`/`-H`: compositor logical size.
- `-f`: fullscreen.
- `-F fsr`: FSR upscaling.
- `--framerate-limit`: limits frames, useful on a laptop or to stabilize power
  use.

gamescope must be installed and available in `PATH`. If it is missing, Quark
reports it and continues without it rather than failing.

## PRIME and hybrid laptops

The PRIME checkbox sets:

```text
__NV_PRIME_RENDER_OFFLOAD=1
__GLX_VENDOR_LIBRARY_NAME=nvidia
__VK_LAYER_NV_optimus=NVIDIA_only
DRI_PRIME=1
```

It forces rendering on the discrete GPU. On some AMD laptops only
`DRI_PRIME=1` is needed; on a single-GPU desktop the checkbox can have no effect
or confuse driver selection. Uncheck it if the game starts on the wrong GPU.

## Reset a prefix without losing saves

A corrupt prefix can prevent every launch. Before deleting it:

1. Close the game and Quark.
2. Locate the prefix on the game page.
3. **Move it** instead of deleting it:

   ```bash
   mv "$PREFIX" "$PREFIX.before-reset-$(date +%Y%m%d)"
   ```

4. Start the game again: Proton recreates a clean prefix.
5. If saves were in the old prefix, copy only the required save folder, not all
   of `drive_c`.

Never delete a prefix "to save space" without checking where the game writes its
saves. For Epic/GOG games, cloud saves may also be in the installation's save
folder.

## Common symptoms

| Symptom | Try |
| --- | --- |
| Black screen then return to desktop | Launch with `PROTON_LOG=1`, read the log, try another Proton runtime. |
| The game does not see the controller | Check Steam Input/udev, try without gamescope, read the log. |
| No sound | Try `PULSE_LATENCY_MSEC=60`, check the default device, try without a wrapper. |
| Crash at startup after changing the prefix | Restore the old prefix or create a new one, then restore the save. |
| Low performance | Check PRIME, MangoHud, the gamescope limit, then try a newer runtime. |
| The game refuses to start with BattlEye/EAC | Check that the Steam anti-cheat runtime is installed and selected; not every game is supported. |
| The game starts but the mouse is captured | Try `-windowed`, or the borderless window mode offered by the game. |
| A Windows module is missing | `winetricks` can be used in the prefix, but Quark does not automate it. Back up the prefix first. |

## See also

- [Stores and saves](Stores-and-Saves) for prefixes and accounts.
- [PlayStation 2 and RetroAchievements](PlayStation-2-and-RetroAchievements):
  PS2 games run natively through PCSX2, without Wine.
- [Troubleshooting](Troubleshooting) for errors that do not come from the game.
- [Settings](Settings) for the defaults of new games.

---

[Home](Home) · [Stores](Stores-and-Saves) · [Overlay](Overlay-and-Achievements) · [Troubleshooting](Troubleshooting)
