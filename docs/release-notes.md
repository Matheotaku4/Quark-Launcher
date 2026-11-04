## Quark 1.2.0 — a guided start, two languages and thirteen themes

### A guided first launch

- A new configuration opens a three-step welcome guide: **Discover**,
  **Personalize**, **Get started**. It explains the library, stores, Proton and
  the settings before offering to add a first game.
- The guide is not blocking. **Skip** closes it immediately.
- Existing libraries are migrated as already welcomed. Updating from 1.1.4 never
  interrupts you with the first-run guide.
- **Settings → About** can reopen the guide or the release summary at any time.

### What's new, once per version

- After an update, Quark shows a short **What's new in 1.2.0** summary with the
  concrete changes, then remembers that this version was read.
- The button **View the release** opens the GitHub release page.

### French and English

- The whole interface can switch between **French** and **English** from
  **Settings → Appearance → Interface language**, without restarting.
- The interface language is separate from the language requested from the Steam
  API. Labels, buttons, hints, statuses, dialogs, onboarding and release notes
  are translated; game names, personal tags and genres remain your data.

### Thirteen dark themes

- **AMOLED**, **Graphite**, **Catppuccin Mocha**, **Catppuccin Macchiato**,
  **Catppuccin Frappé**, **Dracula**, **Nord**, **Gruvbox Dark**,
  **Tokyo Night**, **Rosé Pine**, **Everforest**, **One Dark** and
  **Kanagawa**.
- Each theme provides its own background, panels, surfaces, text, borders and
  accent colors. A theme applies immediately and is saved with the library.

### A random blurred hero background

- **Settings → Appearance → Background** can place a blurred library banner
  behind Home and Library.
- One banner is chosen per session, so restarting Quark picks a new one.
- The blur radius is adjustable from 2 to 30 px, and the blur cache keeps
  separate entries for each radius.
- A dark veil keeps text readable; if the library has no artwork, the normal
  theme background is used.

### Full-screen settings

- Settings now fill the window instead of appearing as a small dialog, with a
  left sidebar and clearly separated sections.
- The page scrolls independently and the top-right **×** or `Esc` closes it.
- Appearance gathers language, themes, background, startup page, library view,
  home carousel and notification icon in one place.

### Update and data safety

- Configuration files from **1.1.4** load without reset. New fields fall back to
  safe defaults, and the onboarding migration marks an existing library as
  already welcomed.
- Games, installations, views, playtime, prefixes, artwork, store sessions and
  local snapshots are preserved.
- Library writes remain atomic; a corrupt configuration is still copied aside
  and the library stays read-only rather than being overwritten.
- Automatic update keeps working for 1.1.4 and later. The archive and its
  SHA-256 are still available for a manual install or recovery.

### Documentation

- README now includes screenshots of Home, Library, Settings, the welcome guide
  and the release summary.
- A complete wiki source covers installation, first launch, library views,
  stores and saves, overlay, settings, troubleshooting and every Proton, Wine
  and umu launch variable.
