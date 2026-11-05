# Wiki source

This directory is the versioned source of the
[Quark GitHub Wiki](https://github.com/Matheotaku4/Quark-Launcher/wiki). All
pages are written in English and use GitHub Wiki links
(`[Installation](Installation)`).

- `Home.md` — entry point and navigation.
- `_Sidebar.md` — wiki sidebar.
- `_Footer.md` — wiki footer.
- one file per page, named after its wiki title, including
  `PlayStation-2-and-RetroAchievements.md`.

## Publishing

GitHub keeps a wiki in a separate Git repository:
`https://github.com/Matheotaku4/Quark-Launcher.wiki.git`.

The workflow [`.github/workflows/wiki.yml`](../.github/workflows/wiki.yml) copies
every `*.md` page except this README into that repository on each push touching
`wiki/`, and can also be started manually from the Actions tab. It uses the
workflow token by default. If the repository restricts access to `.wiki.git`,
add a repository secret named `WIKI_TOKEN` containing a PAT with wiki write
access; the workflow uses it automatically.

The wiki must keep at least one page so GitHub keeps the `.wiki.git` repository
initialized. `Home.md` serves that purpose. Do not put a token, a password or a
credential file in this directory.

## Manual fallback

```bash
git clone https://github.com/Matheotaku4/Quark-Launcher.wiki.git /tmp/quark-wiki
rsync -av --exclude README.md wiki/ /tmp/quark-wiki/
cd /tmp/quark-wiki
git add -A
git commit -m "Quark 1.3.0 documentation"
git push
```
