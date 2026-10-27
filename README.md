# Quark

Launcher de jeux **léger pour Linux**, écrit en Rust (egui), pensé comme une alternative
sobre à Steam : noir profond (AMOLED), affichage liste ou grille, bannière de jeu avec
bouton **Jouer** et temps de jeu, artworks officiels automatiques, et lancement des jeux
Windows via **Proton**.

## Fonctionnalités

- **Deux onglets** : `Boutique` (volontairement vide pour l'instant) et `Bibliothèque`.
- **Ajout de jeu en 2 étapes** :
  1. tu tapes le nom du jeu, Quark le cherche sur **Steam** (API publique `storesearch`)
     puis sur **SteamGridDB**, et récupère les **artworks originaux** (bannière `library_hero`,
     logo, couverture `library_600x900`, icône) ;
  2. tu indiques l'**exécutable** (champ texte, sélecteur de fichiers du système, ou glisser-déposer).
- **Ajout par dossier** : tu choisis un dossier, Quark détecte les exécutables, propose un
  titre reconnu par la recherche, et tu peux corriger le titre, dire « c'est un jeu » ou
  « ce n'est pas un jeu », puis tout ajouter d'un coup.
- **Artworks contrôlés** : chaque image téléchargée est vérifiée (dimensions réelles) pour
  écarter une vignette de mauvaise forme, avec repli sur la fiche `appdetails` de Steam ;
  couverture `library_600x900_2x`, bannière `library_hero`, logo transparent et petite capsule.
- **Affichage façon Steam** : liste à gauche (logo + titre, la ligne sélectionnée a un liseré
  bleu), panneau du jeu à droite ; bannière en haut, bouton **Jouer** avec le **temps de jeu**
  juste à droite, l'**engrenage des paramètres en haut à droite** et le bouton **ⓘ** juste à sa
  gauche, qui affiche ou masque les informations détaillées du jeu.
- **Bascule Liste / Grille** depuis la colonne de gauche (la grille élargit la colonne et
  affiche les couvertures 2:3).
- **Thème AMOLED** (`#000000`) par défaut, variante « Graphite » (noir doux) dans les paramètres.
- **Proton** : détection automatique des runtimes (`steamapps/common/Proton*` et
  `compatibilitytools.d/*`, y compris Flatpak/Snap), choix du runtime **par jeu** ou global,
  **préfixe Proton par défaut** dans les paramètres et **surcharge par jeu**.
- **Suivi du temps de jeu** : chaque session est chronométrée (relance, temps total, dernière session).
- **Diagnostic en ligne de commande** : `--doctor`, `--search`, `--fetch` (aucun écran requis).

## Compilation

Prérequis : Rust récent (`rustup`), les en-têtes X11/Wayland si ton système ne les a pas déjà :

```bash
# Debian/Ubuntu
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev \
                 libwayland-dev libgl1-mesa-dev
# Arch
sudo pacman -S base-devel libx11 libxkbcommon wayland mesa
```

Puis :

```bash
cargo run --release        # lance Quark
cargo build --release      # binaire : target/release/quark
```

### Dépendances principales

- `eframe` / `egui` **0.32** (interface), `egui_extras` 0.32 (chargement des images, y compris
  les miniatures de la recherche en HTTP).
- `reqwest` (bloquant, rustls) pour Steam et SteamGridDB, `serde` pour `config.json`.
- `rfd` pour les sélecteurs de fichiers **natifs** (XDG Desktop Portal : dialogue GTK ou KDE
  selon le bureau, `zenity` en secours). Aucune dépendance GTK à la compilation.
- `image` (codecs PNG/JPEG/WebP) : Quark décode lui-même les artworks et les envoie à egui
  sous forme de textures, sans dépendre du chargeur `file://` d'`egui_extras`, qui n'est pas
  compilé par défaut et affichait une icône d'erreur à la place des bannières.

À noter : `egui` 0.37 réorganise son API (panneaux/`App::ui`) ; Quark vise volontairement la
série 0.32 dont l'API est stable et suffisante ici.

## Utilisation

1. Onglet **Bibliothèque** → bouton **+** (en haut de la colonne de gauche).
2. Tape le nom du jeu : la recherche démarre toute seule (Steam puis SteamGridDB si une clé
   API est renseignée).
3. Clique sur le bon résultat, puis renseigne l'exécutable → **Ajouter le jeu**.
   Les artworks sont téléchargés en arrière-plan (barre de progression en bas de la colonne).
4. Le jeu apparaît sélectionné : bannière, logo, bouton **Jouer** et temps de jeu.
   L'engrenage en haut à droite du panneau ouvre les **paramètres du jeu**
   (exécutable, arguments, runtime Proton, préfixe Proton propre au jeu, suppression).

### Clé API SteamGridDB (optionnelle mais recommandée)

Sans clé, Quark utilise uniquement les artworks officiels servis par le CDN Steam.
Avec une clé gratuite (<https://www.steamgriddb.com/profile/preferences/api>), il va chercher
les bannières/logos/couvertures communautaires les mieux notées, y compris pour les jeux
absents de Steam (GOG, itch.io, émulation…). La clé se saisit dans **Paramètres → Artworks**
et reste dans ton fichier de configuration local.

## Fichiers

| Chemin | Contenu |
| --- | --- |
| `~/.config/quark/config.json` | réglages + jeux (JSON lisible et éditable à la main) |
| `~/.local/share/quark/artwork/<id>/` | artworks téléchargés (aucune requête réseau à l'affichage) |
| `~/.local/share/quark/prefixes/<id>/` | préfixes Proton créés automatiquement |

## Releases

Chaque push sur `main` déclenche [`.github/workflows/release.yml`](.github/workflows/release.yml) :
GitHub Actions compile Quark, crée le paquet
`quark-<version>-linux-x86_64.tar.gz` (+ empreinte SHA-256) et publie une release taguée
`v<version>` (numéro lu dans `Cargo.toml`). On peut aussi la lancer à la main depuis
l'onglet **Actions** du dépôt.

```bash
tar -xzf quark-0.2.1-linux-x86_64.tar.gz
cd quark && ./quark
```

## Proton, concrètement

- Détection : `steam_root/{steamapps/common,compatibilitytools.d}`, plus
  `~/.local/share/Steam`, `~/.steam/root`, Flatpak (`~/.var/app/com.valvesoftware.Steam`) et Snap.
- Lancement : `proton run <exe>` avec
  `STEAM_COMPAT_DATA_PATH` = préfixe effectif et `STEAM_COMPAT_CLIENT_INSTALL_PATH` = racine Steam.
- **Préfixe effectif** : préfixe propre au jeu → sinon *préfixe par défaut* des paramètres →
  sinon `~/.local/share/quark/prefixes/<id>`.
- **Mode de lancement** : `Automatique` (Proton seulement si l'exécutable est un `.exe`, sinon
  natif), `Linux (natif)` pour forcer le lancement direct, ou un runtime Proton précis.
- Si tu utilises un runtime de `compatibilitytools.d` (GE-Proton…), il est utilisé par défaut
  (récence privilégiée) quand aucun choix n'a été fait.

## Mode diagnostic (sans écran)

```bash
target/release/quark --doctor          # dossiers, runtimes Proton détectés, état des jeux
target/release/quark --search portal   # résultats Steam + SteamGridDB et plan d'artworks
target/release/quark --fetch portal    # télécharge réellement les artworks dans /tmp
```

## Structure du code

```
src/main.rs         point d'entrée + CLI (--doctor/--search/--fetch)
src/app.rs          état de l'application, boucle egui, lancement, temps de jeu, toasts
src/ui_library.rs   onglet Bibliothèque : colonne liste/grille + panneau du jeu (bannière, Jouer)
src/ui_store.rs     onglet Boutique (emplacement réservé)
src/ui_dialogs.rs   ajout de jeu, paramètres globaux, paramètres du jeu (sélecteurs natifs)
src/config.rs       modèle de données, thème, persistance JSON, formats (temps de jeu, dates)
src/artwork.rs      recherche Steam/SteamGridDB + téléchargement et cache des artworks
src/scan.rs         découverte des jeux d'un dossier (ajout par lot)
src/images.rs       cache de textures : décodage des artworks par Quark lui-même
src/proton.rs       détection des runtimes Proton + construction de la ligne de commande
src/theme.rs        thème AMOLED/Graphite
src/widgets.rs      icônes vectorielles et boutons dessinés à la main (aucune police d'icônes)
```

## Prochaines étapes possibles

- Remplir l'onglet Boutique (recherche, fiches, prix).
- Vues supplémentaires : grille plein écran, jeux récents, favoris.
- Import automatique de la bibliothèque Steam (`appmanifest_*.acf`) et des raccourcis non-Steam.
