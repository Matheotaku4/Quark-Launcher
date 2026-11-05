## Quark 1.3.1 — Progression RetroAchievements dans toute l'interface

### Progression dans la bibliothèque

- La progression RetroAchievements apparaît directement dans la liste des jeux,
  sous la forme compacte `RA 3/12`. Les jaquettes de la grille affichent le même
  badge.
- La lecture démarre en arrière-plan dès que la bibliothèque affiche un jeu PS2
  associé à une fiche RA, sans attendre l'ouverture de sa fiche détaillée.
- La fiche d'un jeu PS2 rappelle son compteur à côté de **Jouer**, et son onglet
  **Succès** lit désormais RetroAchievements au lieu d'indiquer qu'aucune
  boutique n'est liée.

### Mes jeux

- Un panneau **PlayStation 2 · RetroAchievements** regroupe les jeux PS2 avec
  une barre de progression et un compteur par jeu.
- Chaque ligne indique si la progression est en cours de lecture, indisponible
  ou chargée, et permet d'ouvrir directement la fiche du jeu dans la
  bibliothèque.

### Tri des succès

- Les succès sont triés du moins rare au plus rare dans les fiches, les fenêtres
  de succès et l'overlay en jeu.
- Les succès sans pourcentage de rareté publié restent en fin de liste.
- Les libellés français et anglais décrivent le nouveau tri.

### Validation PCSX2

- Un test automatisé lance un faux binaire PCSX2 avec une ISO synthétique
  contenant `SYSTEM.CNF`.
- Le test vérifie le numéro de série lu dans l'image, le chemin de l'ISO transmis
  à l'émulateur, la conservation des variables personnalisées et le retrait de
  `WINEDLLOVERRIDES`, qui ne doit pas être transmis à PCSX2.
- Aucun émulateur, BIOS ou compte RetroAchievements réel n'est nécessaire pour
  ce test.

### Mise à jour

- Une configuration **1.3.0** se charge sans réinitialisation. Le format de
  `config.json` ne change pas et aucune migration n'est nécessaire.
- Les jeux PS2, identifiants RA, réglages PCSX2/RetroAchievements, DLL
  overrides, artworks et sauvegardes locales sont conservés.
