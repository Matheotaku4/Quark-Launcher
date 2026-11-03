## Quark 1.1.6 — start page, saved views and a steadier library

### Choose where Quark opens

- **Settings → Appearance → Page d’ouverture** now offers **Home**, **Library**,
  **Store** and **My games**. Home is the default, including for existing
  configurations that do not contain the new setting.
- The choice is read once when the window opens, then saved with the library;
  changing it does not move you away from the page you are currently using.

### Saved views and grouping

- The library has a **Views** menu: name the current search, filters, sort and
  grouping, then apply or delete it later. Three starter views (**À jouer**, **En cours**,
  **Terminés**) are seeded once and remain editable.
- The list can be grouped by **status**, **store**, **genre** or **release
  year**, or shown as one continuous list. Group headers keep their collapse
  state for the session.
- Sort the library by name, last session, date added or play time. Search now
  ignores accents and compares each field separately, so a query cannot match
  by scattering letters between the title and unrelated metadata.
- Saved views are cleaned on load and save: empty names, duplicates and filters
  pointing at removed tags or genres are dropped, and the list is capped at 32.

### More robust state

- Unknown values in the new `start_page`, `group_by` and `library_sort` fields
  fall back to Home, Status and Name instead of making the whole configuration
  unreadable.
- The three starter views are marked as seeded, so deleting them does not
  recreate them at the next launch.
- The library and the command palette now share the same accent-folding code,
  avoiding two search behaviours that could drift apart.

### Updates

- 1.1.5 installs this release automatically on its next start. The manual
  archive remains available as a fallback for read-only installs.
