# Quark 1.0 Store and Multi-Store Library Design

**Status:** Approved in conversation; awaiting user review of this specification
**Date:** 2026-10-29
**Target:** Linux x86_64 desktop application

## 1. Goals and non-goals

Quark 1.0 adds Steam Store discovery and a unified owned-games area for Steam, Epic Games, and GOG, integrated into the existing Rust/egui Linux launcher. Installations should lead to a usable Quark game entry. The design also includes achievements and cloud-save interactions where the store/game actually supports them, plus safe local save snapshots using Ludusavi.

All account credentials, library data, recommendations, and caches remain on the user's computer. Quark may contact the relevant storefronts and official tool release endpoints directly for catalogue, authentication, library, installation, achievements, and save operations; it must not send these data to a Quark-operated server. Recommendations are computed locally.

Out of scope for 1.0: payment processing or buying inside Quark, a Quark backend/account service, non-Linux-x86_64 release targets, and promising universal achievements or cloud-save coverage.

## 2. Current project context

The current project is a Rust 2024 eframe/egui Linux launcher. It already has:

- A local JSON config/library at `~/.config/quark/config.json` and application data under `~/.local/share/quark`.
- A `Game` model with Steam AppID, artwork, local executable, Proton configuration, playtime, status, tags, and metadata.
- Steam installed-game discovery through `appmanifest_*.acf` and `libraryfolders.vdf`.
- Steam search/appdetails and SteamGridDB artwork lookups, plus a placeholder Store tab.
- Background worker/channel patterns for network and filesystem work.
- An x86_64 Linux release workflow.

The implementation should extend these patterns and preserve backward-compatible loading of existing library files. Provider-specific functionality must be separated from UI code rather than accumulated in the existing Steam metadata helpers.

## 3. Product structure and user flows

### 3.1 Main areas

Keep the existing Quark Library and add:

1. **Store** — Steam catalogue discovery, featured content/deals and search, plus a separate local-recommendations section based on the user's Quark library and available metadata.
2. **My Games** — a unified list of owned/detected entries with provider filters for Steam, Epic, and GOG. It is not a replacement for the existing manually curated Quark library.

The Store and My Games should use the existing application palette, navigation conventions, and responsive layout. Store items show provider, title, price/discount when available, artwork and a detail view. **Buy** opens the official Steam page in a browser; Quark never handles payment. Store browsing can remain available while another provider is disconnected.

### 3.2 Install and launch

- Steam game installation is handed to the Steam client using the supported local client handoff selected during implementation. Quark must not download Steam content itself. Treat handoff as best effort, since Valve does not provide a stable documented desktop install/progress API for this integration; explain when Steam must be open and do not claim install completion until verified by local manifest detection.
- Epic installation, update, repair and launch are delegated to a Quark-managed Legendary executable.
- GOG installation, update, repair and launch are delegated to a Quark-managed gogdl executable.
- Install roots are configurable per provider. A per-install dialog uses that default and permits a one-time replacement.
- Operations run asynchronously, report progress/output/errors when the tool exposes them, and provide cancellation only where cancellation is implemented and safe. No unverified completion state is shown.
- After local detection confirms a successful installation, Quark automatically adds that provider installation to its unified game record and offers the existing Quark library game functionality.
- If the same title has multiple installed provider versions, prompt on launch unless a preferred provider was saved for that game. The preference is per game, not global.
- Exact title matches may be grouped automatically; ambiguous matches require explicit user confirmation. Provide manual merge and split actions. Keep provider IDs and installations distinct even when the presentation is merged.

### 3.3 Accounts and privacy

- Do not ask the user to enter a store password into Quark. Use the provider/tool browser-mediated login flows.
- Steam identity linking may use browser-based Steam OpenID if the implementation can validate the relying-party response correctly. OpenID identifies the account; it does not grant access to private owned-game details.
- Steam owned games are optional: use the documented Web API only with a user-provided personal key; do not embed a shared secret. Account privacy may still make owned-game data unavailable. Locally installed Steam games remain discoverable without that API.
- Epic uses Legendary authentication. GOG uses gogdl authentication. If Heroic is detected, Quark may offer (not silently perform) to reuse its existing configuration after explicit consent.
- Put credentials in a dedicated local file under Quark's config directory, with owner-only permissions (0600 on Unix). This file is not encrypted; state that limitation to the user. Avoid rewriting/overwriting unrelated config files and write credential updates atomically. Never put tokens, authorization codes, API keys or sensitive account data into logs, error messages, screenshots or diagnostics.
- No library, recommendation input, token, or save data is sent to Quark-operated services. Network requests go only to the relevant provider or official release source.

## 4. Provider adapters and local model

Define provider adapters with stable internal interfaces for:

- Store catalogue/search/details (Steam only in 1.0).
- Account state and library sync (Steam local manifests/API, Epic Legendary, GOG gogdl).
- Installation/update/repair/launch and cancellation/progress where supported.
- Achievements and cloud-save operations, independently advertised per provider/game.

A normalized game identity contains a Quark ID, display title, metadata, and zero or more provider installations. Each provider installation contains at least provider kind, provider-specific stable ID, install state/path, provider-selected executable/launch metadata when known, last sync time, and capability states. Do not use title strings as provider IDs or as filesystem paths.

Capabilities must represent at minimum `supported`, `unsupported`, `unknown`, and `requires_setup` (or equivalent), so the UI can keep unavailable features visible with a concise explanation. Provider/tool/network failure is distinct from an empty library or a game that lacks a feature.

Extend the existing JSON schema compatibly using defaults and explicit migrations. Preserve existing Quark entries, settings, and Steam IDs; a migration failure must preserve a recoverable copy and must not reset the library. Use atomic replacement for writes.

## 5. Store, catalogue, and recommendations

- Store Steam featured categories, search results and details are fetched on demand and cached locally with retrieval timestamps. Local cache remains readable offline and is marked stale; failed requests must not erase previously cached content.
- Storefront endpoints currently used for search/featured data are not documented stable Valve APIs. Encapsulate them, parse defensively, bound response size, validate URLs before opening, tolerate missing fields and provide a clear unavailable/stale state. Do not claim long-term compatibility.
- Respect configured language/country when the endpoint supports it. Prices and discount information must identify their region/currency and may be absent.
- Recommendations are computed locally from games/genres/tags/play history already stored by Quark and cached public store metadata. Do not upload this profile. Keep recommendations in a separate section from Steam's featured/deal catalogue; allow dismissing a recommendation without deleting game data.

## 6. Achievements and cloud saves

- Show achievements/progress only where a provider exposes usable data and the user/game is eligible. Epic Legendary has achievement commands in current releases, but availability and completeness are title-dependent. Steam/GOG capabilities must be verified for the specific API/tool path; do not imply that OpenID or gogdl provides arbitrary achievement access.
- Notifications for a newly unlocked achievement are allowed only when Quark can observe a reliable state change by polling or a provider event. Where supported, refresh achievement state after a session and notify for newly observed unlocks. Avoid claiming real-time notifications where the provider has no event feed.
- Store cloud-save support is a per-title capability. Steam's client remains responsible for its own cloud saves. For Epic/Legendary and GOG/gogdl, automatically request cloud sync before launch and after Quark detects process exit whenever the provider and title support it and setup is complete. These operations are best effort, game/path-dependent and may require initial path setup.
- Never silently overwrite a conflicting save. Show which local/cloud versions are detected and require an explicit user choice before destructive sync/restore. If a conflict blocks automatic synchronization, pause that sync, notify the user, and resume only after an explicit choice. Make provider tool limitations clear.

## 7. Local save safety with Ludusavi

- Quark manages an official Ludusavi release for Linux x86_64, checks its version and release digest before execution, and updates it without overwriting user data. A digest fetched from the same release service is integrity checking, not an independent publisher signature; describe the trust model accurately.
- Invoke Ludusavi as a separate process using argument arrays (never shell interpolation) and structured CLI/API output where available. Pin compatible tested versions, capture exit status/output, and redact secrets/paths where necessary.
- Use Ludusavi preview/discovery. For an unrecognized game, let the user choose the save folder; do not assume discovery succeeded.
- Create a local backup before launch and after Quark detects process exit. Preserve the last 10 backups **per provider installation**, so Steam/Epic/GOG versions do not collide. Use isolated Ludusavi backup destinations/identities per installation and verify the selected CLI strategy with tests before relying on it. Retention must not remove a backup being created, restored, or explicitly protected.
- Restoration is always manual and confirmation-gated. Preview affected paths and require confirmation before overwriting files. Validate paths and prevent traversal/symlink escapes; do not trust manifests, provider titles or archive paths as filesystem paths.
- If the user cancels a backup, Ludusavi is unavailable, or a game path is unknown, do not block normal game launch; explain that a backup was skipped. Never delete the user's source saves as part of retention.

## 8. Managed tools and supply chain

- Manage Legendary, gogdl, and Ludusavi as separate external executables, downloaded only from their official project releases, targeted at Linux x86_64.
- Use release metadata to select a compatible asset, verify the published digest when available, check executable architecture and version, and atomically install into a Quark-owned tools directory. Do not execute incomplete, mismatched, or unverifiable downloads. Keep the previous known-good executable until the new one passes validation.
- Pin/test a known version for each adapter and handle future release schema changes. Show source, version, update status and error reason in Settings. Do not silently update a tool during an active install/save operation.
- Use `std::process::Command` with explicit args, controlled working directory/environment, timeouts, and redacted logs. Never interpolate provider input into a shell command. Serialize mutating operations on the same installation or save directory.
- Review distribution and license obligations for bundled or downloaded GPL-licensed Legendary/gogdl and their dependencies; process separation does not itself settle licensing obligations. Review Ludusavi's code and manifest-data provenance separately. Do not redistribute a copied manifest dataset in Quark without reviewing its terms.

## 9. Error handling and offline behavior

- One provider failing must not disable other providers, Quark's existing library, or the Store cache.
- Distinguish not signed in, private/unavailable library, empty library, missing tool, unsupported title capability, network error, stale cache, malformed response, and installation failure.
- Keep error text actionable and avoid secret-bearing subprocess output. Redact tokens and authorization codes before logging.
- Retain valid cached catalogue/library data on refresh failure. Never convert an error into an empty library and then save that empty result over the last known-good state.
- Cancellation and application shutdown must not leave partial credentials/config files or mark an installation successful prematurely.

## 10. Delivery phases

1. **Foundation:** versioned provider/library data model, safe credential storage, managed tool registry/download verification, adapter interfaces, backward-compatible migration and tests.
2. **Steam Store:** browse/search/details, stale local cache, official browser purchase links and local recommendations.
3. **Unified My Games and installations:** Steam local discovery and optional owned API, Epic/GOG login and library sync, provider filters, matching/merge/split, install roots, asynchronous install lifecycle and automatic Quark import.
4. **Achievements and saves:** per-title capability UI, safe provider cloud sync, Ludusavi previews, before/after snapshots, 10-per-install retention, restore confirmation and conflict UI.

Each phase leaves already-working launcher/library behavior usable if later phases are incomplete. Do not present the full 1.0 as finished until all required phases are implemented and the release checks below pass.

## 11. Validation and release acceptance

### Automated/local checks

- Existing Rust formatting, build and test suite; add focused tests for schema migration and preserving old data.
- Provider adapter tests with recorded, sanitized fixtures: valid, empty, private/unavailable, stale, malformed, oversized and failed responses.
- Tool manager tests for asset selection, architecture/version mismatch, digest mismatch, interrupted download, rollback, timeout, cancellation and secret redaction. Never run unverified test binaries.
- Install-state transition tests ensure no false success and no duplicate provider installation. Matching tests cover exact, ambiguous, manual merge and split.
- Save tests cover unknown game, preview, path traversal/symlink cases, cancellation, per-install retention of ten, protected backups and restore confirmation.
- UI snapshots for Store/My Games states: loading, populated, empty, offline/stale, signed-out, private Steam library, missing tool and operation failure. Inspect the rendered images after UI changes.
- Build the Linux x86_64 release artifact and verify its checksum and packaged README/resources.

### Manual integration checks

Real account and game checks require the user's Steam/Epic/GOG accounts and installed clients/tools. Record these separately as pending until actually performed:

- Steam client handoff and subsequent local install detection on a Steam installation.
- Steam API key flow with both public and private game-details settings.
- Legendary login, library, one install/launch and supported/unsupported achievement/save cases.
- gogdl login/library discovery, one install/launch and save-sync behavior including conflict handling.
- Ludusavi snapshot before and after a game session, retention, manual folder configuration and a confirmed restore to a test save.
- Verify credentials remain local and redacted in logs; verify no Quark service receives library or token data.

Do not claim full browser/provider integration success from compilation, fixtures, or a Steam login redirect alone. Publish/release only after the user explicitly asks; a design approval does not authorize deployment or release.

## 12. Known upstream limits and references

These constraints inform the adapter boundaries and must remain visible in implementation and release notes:

- Steam `GetOwnedGames` is privacy-gated and requires a Web API key: https://partner.steamgames.com/doc/webapi/iplayerservice
- Steam Web API key handling and client-vs-publisher key security: https://partner.steamgames.com/doc/webapi_overview/auth
- Steam browser OpenID identifies a Steam account but is not a desktop OAuth grant: https://partner.steamgames.com/doc/features/auth
- Steam store search/featured endpoints have no stable public contract identified; treat current endpoints as best-effort and undocumented.
- Legendary project and feature documentation: https://github.com/legendary-gl/legendary
- Legendary achievement support history: https://github.com/legendary-gl/legendary/releases/tag/0.21.0
- gogdl is an integration-oriented, non-user-friendly tool: https://github.com/Heroic-Games-Launcher/heroic-gogdl
- GOG game-side cloud save capabilities are title-dependent: https://docs.gog.com/gc-cloud-saves/
- GOG achievements require game/Galaxy SDK support: https://docs.gog.com/sdk-stats-and-achievements/
- Ludusavi CLI and retention: https://github.com/mtkennerly/ludusavi/blob/master/docs/cli.md and https://github.com/mtkennerly/ludusavi/blob/master/docs/help/backup-retention.md
- Ludusavi manifest and data source: https://github.com/mtkennerly/ludusavi-manifest
