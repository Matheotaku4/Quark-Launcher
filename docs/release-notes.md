## Quark 1.3.0 — PlayStation 2, DLL overrides et wiki anglais

### PlayStation 2 avec PCSX2

- Une image disque `.iso`, `.chd`, `.cso`, `.bin/.cue` ou `.gz` peut être ajoutée
  comme un jeu. Quark détecte PCSX2 dans le `PATH`, dans les exports Flatpak et
  dans les emplacements courants ; un chemin explicite est disponible dans
  **Paramètres → Boutiques & outils**.
- Pour les ISO non compressés, Quark lit `SYSTEM.CNF` et extrait le numéro de
  série (`SLUS-20062`, `SCES-…`) sans outil externe. Le lancement passe
  directement par PCSX2, sans Proton ni préfixe Wine.
- La jaquette est téléchargée depuis le dépôt communautaire
  [xlenore/ps2-covers](https://github.com/xlenore/ps2-covers), avec l'URL
  `covers/default/<SÉRIE>.jpg`. Les formats compressés restent utilisables avec
  un numéro de série saisi à la main.
- Un onglet **PlayStation 2** dans la fiche du jeu permet de relire l'ISO, de
  corriger le numéro de série, de recharger la jaquette et de gérer la fiche
  RetroAchievements.

### RetroAchievements

- Le pseudo et la clé Web API RetroAchievements se renseignent dans les
  paramètres. PCSX2 continue de se connecter et de débloquer les succès ; Quark
  lit la même progression avec l'API officielle.
- La fiche PS2 recherche la fiche RetroAchievements par titre, propose
  l'identifiant le plus proche et permet de le corriger. L'API ne fournit pas de
  correspondance directe entre numéro de série et identifiant de jeu.
- La progression, les descriptions et la rareté sont affichées dans la fiche et
  dans l'overlay en jeu. Les déblocages restent gérés par PCSX2 : Quark ne les
  réécrit jamais.

### DLL overrides par jeu

- Un onglet **DLL overrides** liste les DLL du dossier source, permet de choisir
  un autre dossier et coche chaque DLL en **n** (native), **b** (builtin) ou les
  deux. Les choix sont écrits dans `WINEDLLOVERRIDES`.
- Les entrées déjà présentes sont reprises telles quelles, y compris une entrée
  sans `=` comme `steam_api64`, et ne sont pas supprimées par un changement de
  dossier.

### Suggestions grisées

- Les propositions des champs Tags et Arguments utilisent maintenant la couleur
  discrète des textes d'aide. Elles ne se confondent plus avec une valeur
  réellement saisie.

### Documentation

- Le wiki est entièrement traduit en anglais, avec une page dédiée à
  l'émulation PS2 et à RetroAchievements.
- Le README et les notes de version décrivent PCSX2, les formats acceptés, la
  source des jaquettes et la configuration RetroAchievements.

### Mise à jour

- Une configuration **1.2.1** se charge sans réinitialisation : jeux,
  installations, vues, temps de jeu, préfixes, artworks, DLL overrides et
  sauvegardes locales sont conservés.
- Le champ `ps2` et les réglages PCSX2/RetroAchievements sont optionnels ; une
  ancienne configuration reste valide telle quelle.
