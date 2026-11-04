# Wiki source

This directory is the versioned source of the GitHub Wiki for Quark. The pages
are written in French, like the default interface, and use GitHub Wiki links
(`[Installation](Installation)`).

- `Home.md` — entry point and navigation.
- `_Sidebar.md` — wiki sidebar.
- `_Footer.md` — wiki footer.
- one file per page, named after its wiki title.

## Publishing

GitHub keeps a wiki in a separate Git repository:
`https://github.com/Matheotaku4/Quark-Launcher.wiki.git`.

The wiki must already have a first page (GitHub creates the wiki repository when
you save a first page in the web interface). Then, from this repository:

```bash
git clone https://github.com/Matheotaku4/Quark-Launcher.wiki.git /tmp/quark-wiki
rsync -av --exclude README.md wiki/ /tmp/quark-wiki/
cd /tmp/quark-wiki
git add -A
git commit -m "Documentation Quark 1.2.1"
git push
```

Do not put a token, a password or a credential file in this directory. Use the
Git credential helper or GitHub Desktop/CLI for authentication.

The wiki is currently **not published** from this environment: the repository
has no initialized wiki and no GitHub authentication is available here. The
Markdown source, sidebar, links and screenshots references are ready so the
publication is a single sync once the wiki is initialized and authenticated.
