# Manual check: Ludusavi config merging and custom save folders

This check answers three questions about a machine with the real Ludusavi binary:

1. does Quark merge its custom save folder into an existing `config.yaml` without losing the
   settings already in that file, even when the file is YAML (not the JSON older Quark wrote)?
2. does Ludusavi accept and re-read the merged file?
3. does a backup then actually contain the files from that custom folder?

Unit tests cover the merge itself (`cargo test saves::tests`). This document covers the part
only the real tool can confirm.

## Prerequisites

- Linux x86_64, Quark built from this branch.
- Ludusavi installed through Quark's tool page, i.e. present at
  `~/.local/share/quark/tools/ludusavi` (unless `XDG_DATA_HOME` is set).
- One provider game linked to a Quark library entry (`Installé` in **Mes jeux**), so the save
  buttons are visible.
- A folder standing in for a save folder, outside Quark's data directory.

```bash
LUDUSAVI="$HOME/.local/share/quark/tools/ludusavi"
CFG="$HOME/.local/share/quark/ludusavi"
mkdir -p "$HOME/ludusavi-verify/custom-save"
printf 'check' > "$HOME/ludusavi-verify/custom-save/slot1.sav"
"$LUDUSAVI" --version    # prints the version Quark installed, e.g. 0.31.0
```

Every command below uses `--config "$CFG"`: Quark gives Ludusavi an isolated config
directory, it never touches `~/.config/ludusavi`.

## 1. Keep a copy of the current config

```bash
cp "$CFG/config.yaml" /tmp/ludusavi-config.before.yaml 2>/dev/null || echo "no config yet"
```

If there is no config yet, seed one that looks like a file written by Ludusavi, so the
"settings are preserved" part of the check is meaningful:

```bash
mkdir -p "$CFG"
cat > "$CFG/config.yaml" <<'YAML'
# Settings written by Ludusavi, not by Quark
manifest:
  enable: false
theme: dark
customGames:
  - name: Earlier Game
    files:
      - /tmp/earlier-game
    integration: extend
YAML
```

## 2. Let Quark merge its entry

In **Mes jeux**, on the installed game: **Dossiers de sauvegarde…** → pick
`~/ludusavi-verify/custom-save`, then click **Aperçu sauvegarde**.

Any save action prepares the backup vault, and preparing the vault is what merges the config,
so the preview is enough — no backup has to be written for step 2.

The merged file keeps everything and appends one entry (this is real output from that step,
only the title differs):

```yaml
manifest:
  enable: false
theme: dark
customGames:
- name: Earlier Game
  files:
  - /tmp/earlier-game
  integration: extend
- name: <game title as shown in Mes jeux>
  files:
  - /home/<you>/ludusavi-verify/custom-save
  integration: extend
```

## 3. Confirm Ludusavi reads the merged file

```bash
"$LUDUSAVI" --config "$CFG" config show --api \
  | python3 -c 'import json,sys;d=json.load(sys.stdin);print("manifest.enable =",d["manifest"]["enable"]);print("theme =",d["theme"]);print("customGames =",[(g["name"],g.get("files"),g.get("integration")) for g in d["customGames"]])'
```

Observed with 0.31.0 (`Quark Verification` was the test title):

```
manifest.enable = False
theme = dark
customGames = [('Earlier Game', ['/tmp/earlier-game'], 'extend'), ('Quark Verification', ['/tmp/quark-verify/custom-save'], 'extend')]
```

`manifest.enable` and `theme` are the two arbitrary settings from step 1: they must come back
unchanged.

## 4. Confirm the preview includes the custom folder

The title is the one **Mes jeux** shows (Quark passes the provider title verbatim):

```bash
TITLE="<game title as shown in Mes jeux>"
"$LUDUSAVI" --config "$CFG" --no-manifest-update backup --preview --api "$TITLE"
```

Expected: the custom file appears with `"change": "New"`, next to the saves Ludusavi knows
from its own manifest.

```json
{
  "overall": { "totalGames": 1, "totalBytes": 11, "processedGames": 1, "processedBytes": 11,
               "changedGames": { "new": 1, "different": 0, "same": 0 } },
  "games": {
    "Quark Verification": {
      "decision": "Processed",
      "change": "New",
      "files": {
        "/tmp/quark-verify/custom-save/slot1.sav": { "change": "New", "bytes": 11 }
      },
      "registry": {}
    }
  }
}
```

## 5. Confirm a backup contains the folder

Through Quark: **Aperçu sauvegarde** → **Continuer vers la confirmation** → **Créer la
sauvegarde**. Or with the exact command Quark runs, pointed at a stand-in vault:

```bash
BUCKET=$(mktemp -d)
"$LUDUSAVI" --config "$CFG" --no-manifest-update backup --api --force --no-cloud-sync \
  --path "$BUCKET" --full-limit 10 --differential-limit 0 "$TITLE"
find "$BUCKET" -name slot1.sav
"$LUDUSAVI" --config "$CFG" backups --api --path "$BUCKET" "$TITLE"
```

Expected: the file is stored under `drive-0/` with its full source path, the backup directory
is named `backup-<timestamp>`, and `backups --api` returns that ID:

```
/tmp/…/Quark Verification/backup-20261030T231757Z/drive-0/tmp/quark-verify/custom-save/slot1.sav
```

With Quark itself the vault is `~/.local/share/quark/save-backups/<32 hex chars>/`: the folder
name is a digest of the provider, the product ID and the install path, so each provider
installation gets its own bucket.

## 6. Optional: restore preview

This is the read-only step Quark performs, and validates, before any restore write:

```bash
"$LUDUSAVI" --config "$CFG" --no-manifest-update restore --preview --api --no-cloud-sync \
  --path "$BUCKET" --backup backup-20261030T231757Z "$TITLE"
```

Expected: the same file listed once, `"decision": "Processed"`.

## Pass / fail

| Check | Expected | If it fails |
| --- | --- | --- |
| Step 2 file | earlier settings and entries still present, one new entry added | merge lost settings: do not ship |
| Step 3 `config show` | settings unchanged, both `customGames` entries | Ludusavi cannot parse the merged file: Quark's YAML output is wrong |
| Step 4 preview | custom file listed as `New` | entry written but not applied: `name` does not match Ludusavi's title, or `files` is not a list |
| Step 5 backup | file inside the snapshot, ID in `backups --api` | the folder is unreadable or outside Ludusavi's scan |
| Step 6 restore preview | same paths, `Processed` | restore would be refused by Quark's path validation |

A `customGames` entry whose title differs by case or punctuation is a separate game for
Ludusavi: it will back up the custom folder but not the manifest saves. Use the title as
**Mes jeux** displays it.

## Known limitations

- Rewriting the file drops YAML comments and resolves anchors/aliases; values, structure and
  keys are preserved.
- Quark never removes a path from a `customGames` entry, because nothing distinguishes a path
  it wrote from one written by hand. Removing a folder in Quark only stops future additions.
- `integration` is set to `extend` so the manifest saves stay included; other keys of the same
  entry (`alias`, `registry`, `ignore`, …) are left as they are.
- The restore preview only proves the paths; Quark additionally re-checks them and requires an
  explicit confirmation before writing.

## Cleanup

```bash
rm -rf "$HOME/ludusavi-verify" "$BUCKET"
cp /tmp/ludusavi-config.before.yaml "$CFG/config.yaml"   # only if you replaced a real config
rm -f /tmp/ludusavi-config.before.yaml
```
