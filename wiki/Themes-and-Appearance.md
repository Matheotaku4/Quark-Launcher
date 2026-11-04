# Thèmes et apparence

> [Accueil](Home) · [Premier démarrage](Getting-Started) · **Thèmes et
> apparence** · [Réglages](Settings)

![Apparence en français](https://raw.githubusercontent.com/Matheotaku4/Quark-Launcher/main/docs/images/settings-fr.png)

## Une page pleine écran, pas une petite fenêtre

Depuis la 1.2, **Paramètres** remplit la fenêtre. Une barre latérale à gauche
liste les pages, la page active est encadrée par la couleur d'accent, et chaque
section est séparée par un titre et un trait. Le bouton **×** en haut à droite
ferme les réglages. Le contenu défile indépendamment de la navigation.

Les pages sont : **Apparence**, **En jeu**, **Nouveaux jeux**, **Artworks**,
**Boutiques & outils**, **Statistiques**, **Espace disque** et **À propos**.
La page [Réglages](Settings) décrit chacune d'elles.

## Langue de l'interface

**Apparence → Langue de l'interface** propose **Français** et **English**. Le
changement est immédiat : toute l'interface est reconstruite avec le nouveau
catalogue, sans redémarrer Quark. Le choix est enregistré dans
`settings.ui_language`.

Cette langue est **distincte** de `settings.language`, qui demande à l'API Steam
les descriptions dans une langue donnée. Vous pouvez donc garder l'interface en
français et les fiches Steam en anglais, ou l'inverse.

Les noms de jeux, de tags et de genres personnels ne sont pas traduits : ce sont
vos données. Les libellés de l'application, les infobulles, les boutons, les
statuts et les messages d'erreur le sont.

## Les 13 thèmes

La grille **Apparence → Thème** montre un aperçu de chaque palette. Un clic
applique le thème à toute l'interface immédiatement et l'enregistre.

| Thème | Ambiance |
| --- | --- |
| **AMOLED** | Noir pur, contraste maximal, accent bleu. |
| **Graphite** | Gris neutre, plus doux que le noir pur. |
| **Catppuccin Mocha** | Base sombre chaude, accents pastel. |
| **Catppuccin Macchiato** | Variante Catppuccin un peu plus claire. |
| **Catppuccin Frappé** | Variante Catppuccin plus douce. |
| **Dracula** | Violet sombre et accents magenta. |
| **Nord** | Bleu-gris froid, inspiration arctique. |
| **Gruvbox Dark** | Brun chaud et accents jaune/vert. |
| **Tokyo Night** | Bleu nuit et violet électrique. |
| **Rosé Pine** | Prune sombre et rose poudré. |
| **Everforest** | Vert forêt et tons naturels. |
| **One Dark** | Gris bleuté et accents colorés sobres. |
| **Kanagawa** | Bleu profond inspiré des estampes japonaises. |

Chaque palette définit le fond, les panneaux, les surfaces, le texte, les
bordures et une couleur d'accent avec ses variantes de survol et d'appui. Le
thème ne change pas les couleurs d'accent propres à un jeu : elles restent
calculées à partir de son artwork.

![Apparence en anglais](https://raw.githubusercontent.com/Matheotaku4/Quark-Launcher/main/docs/images/settings-en.png)

## Fond hero aléatoire

**Apparence → Fond → Fond hero aléatoire** ajoute une bannière floutée derrière
l'accueil et la bibliothèque.

- Quark choisit une bannière parmi les artworks de la bibliothèque.
- Le tirage est fait **une fois par session** : redémarrer Quark change
  l'image, naviguer entre les pages ne la change pas.
- Le curseur **Flou** règle le rayon de 2 à 30 px. Le cache de flou tient compte
  du rayon, donc modifier le réglage ne réutilise pas l'ancien rendu.
- Un voile sombre est appliqué entre l'image et le contenu pour garder le texte
  lisible ; le bas de la page est plus assombri que le haut.
- Si la bibliothèque ne contient aucun artwork, aucun fond n'est dessiné : le
  thème reprend son fond normal.

Le fond est décoratif. Il ne remplace pas la bannière de la fiche d'un jeu, ni
les couvertures de la bibliothèque.

## Page d'ouverture

**Apparence → Démarrage → Page d'ouverture** :

- **Accueil** (par défaut) ;
- **Bibliothèque** ;
- **Boutique** ;
- **Mes jeux**.

Le choix est lu une fois à l'ouverture de la fenêtre. Changer le réglage ne vous
téléporte pas immédiatement : il s'applique au prochain démarrage. Une
configuration ancienne qui ne contient pas ce champ démarre sur l'accueil.

## Affichage de la bibliothèque

**Apparence → Bibliothèque** règle :

- la vue par défaut, **liste** ou **grille** ;
- le tri de l'accueil : dernière session, date d'ajout ou temps de jeu ;
- le défilement automatique du carrousel et son intervalle ;
- l'affichage de l'icône de notification.

Les vues nommées, le tri et le groupement de la bibliothèque sont décrits dans
[Bibliothèque et vues](Library-and-Views).

## Notifications et icône de zone de notification

L'icône de notification permet de garder Quark en arrière-plan : la croix range
la fenêtre dans l'icône au lieu de quitter. Les captures Epic/GOG peuvent aussi
afficher une notification de bureau. Ces réglages se trouvent dans
**Apparence** et **En jeu**.

Sous certains environnements sans zone de notification (certains compositeurs
Wayland minimalistes), l'icône peut ne pas apparaître. Quark reste utilisable ;
quittez avec `Ctrl+Q`.

## Couleurs d'accent des jeux

Chaque fiche peut dériver une couleur d'accent de son artwork. Cette couleur
s'applique aux boutons et aux marqueurs de la fiche, sans modifier le reste de
l'interface. Elle ne change pas le thème global et n'est pas enregistrée comme
un thème séparé.

## Réinitialiser l'apparence

Pour revenir aux valeurs par défaut sans perdre la bibliothèque :

1. **Paramètres → Apparence**.
2. Choisissez **AMOLED**, **Accueil**, la vue **Liste**.
3. Désactivez le fond hero aléatoire.
4. Si nécessaire, remettez la langue en français.

Il n'existe pas de bouton « tout réinitialiser » qui efface la bibliothèque.
Une réinitialisation complète demanderait de supprimer `config.json`, ce qui
supprime aussi les jeux, installations, vues et statistiques. Préférez changer
les réglages un par un.

## Voir aussi

- [Réglages](Settings) pour les autres pages.
- [Bibliothèque et vues](Library-and-Views) pour les vues nommées.
- [Dépannage](Troubleshooting) si un texte reste dans la mauvaise langue.

---

[Accueil](Home) · [Réglages](Settings) · [Bibliothèque](Library-and-Views) · [Overlay](Overlay-and-Achievements)
