## Quark 1.0.9 — icônes Lucide, fiches Steam liées et robustesse

- **Icônes Lucide** : les symboles dessinés à la main sont remplacés par la police d'icônes
  Lucide embarquée (`assets/fonts/lucide.ttf`, licence ISC). Plus, grille, liste, dossier,
  téléchargement, réglages, recherche, lien et flèches gardent la même taille et restent nets
  avec Archivo.
- **Défilement plus rapide** : la molette et le pavé tactile défilent environ un tiers plus
  vite qu'avec les réglages par défaut d'egui, sans modifier les autres gestes.
- **Installation GOG retrouvée** : gogdl peut créer un dossier dont le nom diffère du
  `folder_name` annoncé. Quark cherche désormais le manifeste `goggame-<id>.info` dans le
  dossier choisi et ses sous-dossiers, au lieu de déclarer l'installation non confirmée.
  L'erreur « installation terminée sans dossier de jeu » ne doit plus apparaître quand le jeu
  est bien présent.
- **Doublons GOG « Amazon Prime »** : les éditions dont le titre se termine par
  « Amazon Prime », « + Amazon Prime » ou « Prime Gaming » sont nettoyées et fusionnées avec
  la fiche normale, en gardant l'identifiant GOG de base et l'état d'installation suivi.
- **Succès Epic lisibles** : `legendary achievements --json` peut ajouter des lignes de
  journal autour du JSON et renvoyer des listes `null`. Le parseur lit le premier objet
  complet, tolère ces formes, lit les icônes (`icon_link`) et la progression à paliers
  (`progress`) des succès Epic, et affiche des noms et descriptions un peu plus grands et
  plus contrastés dans la fiche et dans « Mes jeux ».
- **Aperçus SteamGridDB** : les propositions utilisent maintenant la vignette `thumb` de
  l'API v2 au lieu de décoder la bannière pleine résolution pour une tuile de 132×76. Les
  aperçus s'affichent plus vite et échouent beaucoup moins souvent ; l'image finale reste la
  version pleine taille.
- **Jeux populaires** : la section « Populaires sur Steam » passe à 20 jeux. SteamDB ne
  fournit pas d'API publique et bloque les requêtes automatisées ; Quark utilise donc le
  classement officiel `ISteamChartsService/GetMostPlayedGames` de Steam, avec les pics de
  joueurs.
- **Lier une fiche depuis les paramètres** : pour un jeu sans source Steam/SteamGridDB, la
  section **Fiche Steam** propose une recherche. Un résultat lié puis enregistré récupère la
  jaquette, la bannière, le logo et l'icône ; un bouton permet aussi de délier la fiche.
- **Bibliothèque protégée** (issues #1, #2, #18) : une configuration illisible ou inaccessible
  n'est plus remplacée par une bibliothèque vide. Quark crée une copie horodatée unique, passe
  en lecture seule et affiche un bandeau persistant ; une sauvegarde échouée garde les
  modifications en attente et réessaie avec un délai croissant. Les écritures sont atomiques
  (`fsync` + renommage), y compris pour `session-achievements.json`, qui n'est plus écrasé
  quand il est corrompu.
- **Processus et sessions** (issues #6, #7, #8, #9, #12) : l'arrêt d'un jeu est idempotent,
  continue l'escalade vers `SIGKILL` même si le processus chef est déjà mort, et vérifie le
  groupe avant d'envoyer un signal. Un PID n'est reconnu comme le jeu que si l'exécutable, la
  ligne de commande et l'heure de démarrage correspondent, ce qui évite de tuer un processus
  qui a réutilisé son numéro. Les commandes Epic/GOG ont leur propre groupe de processus, et
  tous les processus auxiliaires (notifications, son, ouverture de dossier, navigateur) sont
  récoltés pour éviter les zombies.
- **Pont des succès** (issues #3, #21) : chaque connexion est traitée dans son thread avec une
  limite de 32 connexions et des timeouts courts ; une connexion locale lente ne bloque plus
  les rapports. Un envoi GOG ne marque désormais que le succès réellement confirmé, pas tous
  ceux de la session.
- **Chemins et suppression** (issues #13, #14, #20) : l'exécutable Steam est choisi par
  correspondance avec le titre et `installdir`, pas par sa taille ; `installdir` ne peut plus
  sortir de `steamapps/common`. La désinstallation exige une racine d'installation connue et
  refuse les liens symboliques ou les chemins hors racine. Les chemins non UTF-8 sont conservés
  jusqu'à `Command` au lieu de passer par `display().to_string()`.
- **Sauvegardes Ludusavi** (issues #15, #16) : les destinations de restauration sont limitées
  aux dossiers du jeu, les liens symboliques (même cassés) sont refusés et `$HOME` n'est plus
  une racine autorisée. Retirer le dernier dossier personnalisé le supprime aussi de la
  configuration Ludusavi ; deux installations du même titre ne partagent plus la même entrée.
- **Réseau et mise à jour** (issues #10, #11) : les corps HTTP (images, succès, actualités,
  recherche, GitHub) sont lus avec une limite pendant le streaming. Les archives de mise à
  jour sont plafonnées, comparées à leur taille annoncée et vérifiées par la somme SHA-256
  publiée ; une archive invalide est supprimée.
- **X11 et démo** (issues #5, #17, #19) : le raccourci de capture est relâché et son thread X11
  arrêté quand le jeu s'arrête ou quand la touche change. Une perte de connexion X11 depuis ce
  thread ne termine plus tout le launcher. `--ui --demo` utilise désormais une configuration et
  des données temporaires, sans lire ni écrire la bibliothèque réelle.
- **Journaux de session** (issue #4) : les journaux sont créés en `0600`, ne contiennent plus
  les valeurs des variables d'environnement ni la ligne de commande complète, et sont plafonnés
  à 4 Mio par session.
- **Vérifications** : `cargo test --offline` (235 tests) et
  `cargo build --release --locked --offline` passent. Les écrans avec icônes Lucide, la
  recherche de liaison et le rendu `--demo` isolé ont été contrôlés hors écran. Les comptes
  Epic/GOG réels et l'API SteamGridDB avec une clé personnelle ne sont pas disponibles dans
  cet environnement.

## Quark 1.0.8 — Mes jeux fluide, téléchargements simultanés et Archivo

- **Cartes de « Mes jeux » réparées** : le pied de carte utilisait un `scope_builder`
  qui faisait avancer le curseur de la rangée. La carte suivante commençait donc avant la
  fin de la précédente et les couvertures se chevauchaient. Le pied est maintenant un enfant
  isolé et clipé, avec 60 px réservés sous l'image : les cartes restent séparées, y compris
  avec une police agrandie.
- **Défilement virtualisé** : seules les lignes visibles sont construites et décodées. Le
  bouton « Afficher 72 jeux de plus » disparaît : toute la bibliothèque défile d'un seul
  tenant, sans blocage ni pic de mémoire au moment de l'affichage. Une bibliothèque de 450
  jeux de boutique a été rendue en 1360×860 et 1024×640.
- **Trois téléchargements simultanés** : chaque installation Epic/GOG est une tâche
  indépendante, avec son propre pourcentage, sa vitesse, son ETA et son bouton Annuler. Le
  panneau de la barre du haut et la page « Mes jeux » listent toutes les tâches ; une même
  fiche ne peut pas être lancée deux fois. La limite est de trois pour ne pas saturer la
  connexion et le disque.
- **Images de boutique dans « Personnaliser »** : une fiche Steam du même titre est
  prioritaire lors du rattachement d'une installation GOG/Epic. Sa jaquette et sa bannière
  sont complétées si elles manquent, et les images déjà en cache apparaissent sous chaque
  emplacement avec un bouton **Depuis la boutique** (GOG, Epic, Steam). Une image existante
  n'est jamais remplacée sans action de l'utilisateur.
- **Police Archivo** : l'interface embarque Archivo (licence SIL OFL, fichier dans
  `assets/fonts/`) au lieu de dépendre de la police du système. Les polices egui restent en
  secours pour les glyphes absents.
- **Recherche multi-sites** : les résultats continuent d'ouvrir la page du site dans le
  navigateur. Aucun lien direct d'hébergeur n'est extrait et aucune archive n'est téléchargée
  ou décompressée automatiquement.
- **Vérifications** : `cargo test --offline` (218 tests) et
  `cargo build --release --locked --offline` passent ; le rendu de 450 jeux de boutique a été
  contrôlé. Les téléchargements simultanés et le rattachement des images n'ont pas pu être
  testés avec de vrais comptes Epic/GOG dans cet environnement.

## Quark 1.0.7 — 660 jeux espacés, recherche multi-sites et bannières importées

- **Grille virtualisée** : la vue grille ne construit et ne décode plus que les lignes
  visibles. Avec 660 jeux, les 660 jaquettes n'étaient téléchargées qu'une fois, mais elles
  restaient toutes retenues dans un `Vec` de textures : la mémoire montait de plusieurs
  centaines de mégaoctets et les cartes finissaient par se chevaucher. Les lignes ont aussi
  une hauteur fixe de 44 px sous la jaquette, ce qui garde un espace régulier entre les jeux
  et empêche le titre de déborder sur la rangée suivante. `--demo-count 660` permet de
  reproduire cette bibliothèque au rendu hors écran.
- **Recherche multi-sites depuis la boutique** : un clic sur un jeu de la boutique ouvre une
  fenêtre qui cherche son titre sur **ankergames.net**, **astralgames.net**,
  **gamebounty.world**, **kryo.to** et **scene.cat**. Chaque source répond séparément :
  AnkerGames, GameBounty et Kryoto par leur API JSON, scene.cat par son HTML, AstralGames via
  son lecteur de page. Les résultats affichent le titre, la taille, la version ou la catégorie,
  et s'ouvrent dans le navigateur. **Voir sur Steam** reste disponible, et un bouton de repli
  ouvre la recherche du site quand l'adaptateur échoue ou ne trouve rien. Quark ne télécharge
  aucun fichier depuis ces sites.
- **Bannière importée avec la jaquette** : l'adoption d'un jeu Epic/GOG récupère désormais
  **deux** images — la jaquette verticale et l'illustration large (`backgroundImage` GOG,
  `DieselStoreFrontWide`/`OfferImageWide` Epic, `library_hero` Steam). Les jeux déjà liés qui
  n'avaient que leur poster sont complétés en arrière-plan, une fiche à la fois, sans jamais
  écraser une image personnalisée. Une image verticale est refusée comme bannière.
- **Vérifications** : `cargo test --offline` (218 tests) et
  `cargo build --release --locked --offline` passent. Les cinq adaptateurs ont reçu de vraies
  réponses réseau pour « Cyberpunk 2077 », et le parcours complet « Portal 2 » depuis une carte
  de la boutique a été rendu avec les résultats filtrés des cinq sources. Le rendu de 660 jeux
  a été contrôlé en 1360×860 et 1024×640.

Limites : les résultats réels des cinq sites dépendent de leur disponibilité et de leur mise
en page ; AstralGames passe par un lecteur tiers puisque sa recherche est rendue en
JavaScript. Les bannières Epic/GOG n'ont pas pu être vérifiées avec un vrai compte dans cet
environnement ; les parseurs et le cache sont testés, et le repli Steam reste disponible.

## Quark 1.0.6 — installations confirmées, grilles légères et images des succès

- **Confirmer avant d'installer** : un clic gauche ou droit sur une carte Epic/GOG non
  installée ouvre une fenêtre avec le titre, la **taille à télécharger**, le **dossier
  d'installation** et un bouton **Installer**. Le dossier proposé vient de **Paramètres →
  Nouveaux jeux → Dossier d'installation des boutiques** (`~/Games` par défaut) ; **Changer…**
  permet d'en choisir un autre pour cette installation seulement.
- **Dossier par défaut respecté** : Legendary et gogdl reçoivent désormais ce dossier parent,
  au lieu de rouvrir systématiquement le sélecteur. Le dossier est créé si nécessaire.
- **Débit en MB/s** : les panneaux d'installation et de téléchargement affichent la vitesse en
  **MB/s**, avec la taille, l'ETA et le bouton **Annuler**.
- **Cartes plus légères** : les grilles utilisent des miniatures de **360 px** au lieu des
  jaquettes pleines (600×900 ou 1920×620), ce qui réduit fortement la mémoire GPU et le coût
  d'envoi des textures quand on clique sur **Afficher … de jeux de plus**. Les jaquettes
  restent nettes dans la fiche jeu.
- **Couvertures récupérées plus vite** : jusqu'à **quatre téléchargements en parallèle** et
  32 jeux préparés par balayage. Steam, GOG puis le repli Steam restent les sources, avec le
  monogramme seulement en dernier recours.
- **Temps de jeu GOG réparé** : Quark lit le total publié sur la page de profil GOG
  (`matchedGameStats`, en minutes), au lieu de dépendre uniquement des sessions Galaxy. Les
  sessions restent un repli quand le profil est privé ou indisponible. Le temps local n'est
  toujours jamais substitué à celui de la boutique.
- **Images des succès** : les URL `image_url_unlocked` / `image_url_locked` de GOG et les
  icônes publiées par Legendary pour Epic sont téléchargées dans
  `~/.local/share/quark/artwork/achievement-icons/` et affichées à côté de chaque succès.
  L'image est mise en cache, réduite à 96 px et n'est jamais redemandée au réseau.
- **Succès triés par rareté** : GOG publie le pourcentage de joueurs ayant obtenu chaque
  succès (`rarity`). La liste l'affiche et se trie **du plus rare au plus courant**, comme
  les trieurs de rareté d'Epic ou de Heroic ; un succès sans pourcentage connu (Epic le
  publie rarement) reste en fin de liste. La progression locale d'un succès à paliers est
  désormais libellée « progression » pour ne pas être confondue avec ce pourcentage.

## Quark 1.0.5 — overlay en jeu, téléchargements sous la main et temps de jeu des boutiques

- **Overlay en jeu, actif par défaut** : dès qu'un jeu lancé par Quark démarre, une petite
  fenêtre transparente, toujours au-dessus et qui laisse passer la souris affiche le jeu, sa
  boutique et le chrono de la session. Elle se ferme avec le jeu ; le réglage **Paramètres →
  En jeu** permet de la désactiver et de changer la touche de capture.
- **Captures F12** : un raccourci X11 global (donc aussi pour les jeux Proton sous XWayland)
  écrit une capture dans `~/.local/share/quark/screenshots/<jeu>/<horodatage>.png`. Les jeux
  Epic et GOG déclenchent en plus un carillon et une notification système, comme le client
  Steam ; Steam reste silencieux. Sans serveur X, Quark essaie `grim` puis les outils du
  bureau, et l'overlay reste disponible pour les succès.
- **Téléchargements visibles partout** : une icône dans la barre du haut (badge = transferts
  actifs) ouvre un panneau avec, pour chaque installation Epic/GOG, l'étape, la barre de
  progression, la taille téléchargée et totale, le débit, l'ETA et **Annuler**.
- **Désinstallation proposée** : les cartes Epic/GOG ont une action **Désinstaller…** avec
  confirmation ; le dossier d'installation est supprimé, les sauvegardes et le préfixe Proton
  sont conservés. Retirer de la bibliothèque un jeu lié propose aussi de supprimer ses
  fichiers, et plusieurs désinstallations sont mises en file.
- **Installations vérifiées en direct** : toutes les 5 secondes, Quark recontrôle les dossiers
  des jeux installés. Un jeu supprimé hors de Quark repasse en **Non installé**, son chemin
  mort est effacé et un message l'explique.
- **Succès dans la fiche** : sous la bannière, les onglets **Actualités / Succès** affichent
  débloqués/total, la progression, les succès masqués et une pastille **session** pour ceux
  rapportés pendant la partie, avec **Actualiser**.
- **Temps de jeu de la boutique** : la fiche d'un jeu lié à Epic ou GOG affiche le temps publié
  par la boutique (service de bibliothèque Epic, sessions GOG), jamais le compteur local. Steam
  garde le temps local, faute de publication.
- **Environnement des jeux de boutique** : un jeu adopté depuis Epic/GOG ne reçoit plus les
  variables « par défaut » ; celles déjà copiées sont filtrées au lancement. Ses propres
  variables restent appliquées.
- **Jeux installés reliés aux fiches existantes** : si un jeu ajouté à la main porte le même
  titre (exécutable vide ou dans le dossier installé), l'installation est reliée à sa fiche au
  lieu de créer un doublon. Un exécutable absent mais présent dans le dossier GOG/Epic est
  détecté au lancement.
- **Couvertures** : Epic et GOG d'abord, puis repli sur la jaquette verticale Steam du même
  titre, et seulement ensuite le monogramme.
- **Grosses bibliothèques** : la grille de **Mes jeux** rend 72 cartes puis un bouton
  **Afficher … de plus**, au lieu de construire 450 cartes à chaque image.

Limites assumées : l'overlay, le raccourci X11, la capture, le son et les notifications n'ont
pas pu être essayés dans une vraie session graphique (aucun serveur X/audio dans
l'environnement de développement) ; sous Wayland natif, la capture globale dépend des outils
du bureau. Les succès et temps de jeu Epic/GOG n'ont pas été validés avec de vrais comptes —
les parseurs et les états d'erreur sont couverts par des tests. Le repli Steam ne s'applique
qu'après l'échec de la source de la boutique.

## Quark 1.0.4 — téléchargements suivis, jaquettes justes, succès Epic et GOG

- **Progression des installations** : Legendary et gogdl écrivent leur avancement ligne par
  ligne ; Quark l'affiche en haut de **Mes jeux** (pourcentage, étape, taille, débit, ETA)
  avec un bouton **Annuler** qui ferme proprement le processus. Le pourcentage ne recule
  jamais quand un fichier est repris, et une bande animée remplace le chiffre tant que
  l'outil n'en publie pas.
- **Jaquettes corrigées** : les couvertures GOG passaient par `images.background` + suffixe
  vertical, un identifiant qui sert une *autre* image — d'où des jaquettes qui ne
  correspondaient pas au jeu. Elles viennent maintenant de `api.gog.com/v2/games/<id>`
  (`boxArtImage` → `_glx_vertical_cover`), et le cache est versionné pour ignorer les
  anciennes. Côté Epic, `legendary info --json` ne publie pas `keyImages` : la couverture
  est lue dans le cache de métadonnées que Legendary vient d'écrire, puis importée dès que
  le jeu est installé et adopté (en arrière-plan, sans bloquer la page).
- **Succès Epic et GOG** : Epic reste lu par `legendary achievements --json` (avec la
  progression des succès à paliers) ; GOG est lu par `gameplay.gog.com` avec la session
  gogdl (`client_id` = identifiant du jeu, `user_id` de la session), succès masqués compris
  une fois débloqués. L'entrée **Succès…** de GOG n'est plus grisée.
- **Succès envoyés par la session de jeu** : chaque lancement transmet
  `QUARK_ACHIEVEMENTS_URL`, `QUARK_ACHIEVEMENTS_TOKEN` et `QUARK_GAME_ID`. Un POST JSON
  débloque un succès ; Quark l'enregistre, l'affiche avec une pastille **session** et
  l'écrit chez GOG (Epic n'expose aucune écriture hors de son client : ces succès restent
  locaux). L'écoute ne sort pas de la boucle locale, le jeton change à chaque lancement et
  le journal de session le masque.
- **Réglages plus épurés** : les cases à cocher des paramètres du jeu et des paramètres
  globaux laissent place à des lignes pleine largeur — texte et explication à gauche,
  interrupteur animé à droite. GameMode, MangoHud, PRIME, gamescope (avec son bloc
  résolution/FSR/fps), journal Proton, umu, anti-triche, icône de notification et carrousel
  suivent le même modèle.
- **Transition héros / fond** : le changement de jeu fond la bannière et le fond flouté en
  0,55 s avec une courbe adoucie, et le bas de la bannière se dissout dans la version
  floutée de la même image : plus d'arête nette entre le hero et le background.
- **Boutique dynamisée** : grande bannière avec dégradé, remise et prix, rangée
  **Top réductions**, en-têtes de section avec filet coloré et compteur, cartes qui se
  soulèvent au survol avec remise et pastille Linux sur l'image.

Limites assumées : les installations Epic/GOG, les succès et le pont de session n'ont pas
été validés avec de vrais comptes (parseurs, états d'erreur et pont local couverts par des
tests) ; Steam n'expose toujours pas ses succès sans sa propre session.

## Quark 1.0.3 — « Mes jeux » : tri, recherche, couvertures et installations Epic/GOG

- **Tri et recherche** : les onglets de **Mes jeux** (y compris **Toutes**) sont triés par nom,
  accents et casse ignorés, et un champ *Rechercher un jeu* filtre les jeux possédés au fil de
  la frappe. Le filtre ne s'applique qu'aux titres des boutiques synchronisées et s'efface d'un
  clic.
- **Couvertures des boutiques** : chaque carte affiche la jaquette verticale du jeu — Steam
  depuis son CDN, GOG depuis les images produit (`images.background` + suffixe vertical), Epic
  via `legendary info`. Téléchargées une par une à l'ouverture de la page, mises en cache sur
  disque et mémorisées pour ne pas être redemandées ; à défaut, le monogramme reste affiché.
- **Installation Epic réparée** : « Installation Epic non confirmée » venait de Legendary
  attendant des réponses clavier alors que le processus n'a pas de terminal. L'appel passe
  désormais par `-y` (global) avec `--skip-sdl --skip-dlcs --platform Windows`. En cas d'échec,
  le message cite la dernière ligne utile de l'outil (les lignes pouvant contenir une adresse de
  compte sont écartées) au lieu d'un simple « a échoué ».
- **Installation GOG** : `gogdl info` donne le nom du dossier, puis `gogdl download` installe
  dans le dossier choisi, sans invite interactive et avec un délai adapté au téléchargement.
- **Double-clic sur une carte** : installe le jeu s'il ne l'est pas encore (Steam, Epic ou GOG
  selon la boutique et l'outil installé), sinon ouvre sa fiche dans la bibliothèque.
- **Installations adoptées** : après une installation réussie, Quark crée la fiche du jeu —
  réglages par défaut, couverture et lien boutique — pour qu'il apparaisse dans la
  bibliothèque, prêt à lancer.
- **Succès Epic** : *Actions → Succès…* lit `legendary achievements <app> --json` et affiche
  débloqués/total, une barre de progression et la liste complète. **GOG** ne publie ses succès
  qu'auprès de Galaxy, sans identifiant client public : l'entrée reste désactivée et l'explique.
- **Fiche de jeu** : la bannière et le fond flouté se fondent enchaînés au changement de jeu
  (0,38 s) au lieu de changer d'un coup ; la fiche ne redemande des images que pendant la
  transition.

Limites assumées : les installations Epic et GOG ainsi que les succès Epic n'ont pas pu être
exécutés avec de vrais comptes — les arguments, les parseurs et les états d'échec sont couverts
par des tests, mais le chemin réseau complet demande un essai manuel. Les succès GOG restent
hors de portée de Quark.

## Quark 1.0.2 — synchronisation GOG, « Mes jeux » par boutique et réglages clarifiés

- **Synchronisation GOG** : gogdl n'expose aucune commande de liste. Quark lit donc sa session,
  rafraîchit le `refresh_token` auprès de `auth.gog.com` (client public, comme gogdl), puis
  interroge `embed.gog.com/user/data/games` et `api.gog.com/products` par lots de 40, avec repli
  sur `getFilteredProducts`. La session n'est réécrite que si elle a changé, atomiquement et en
  `0600`. Les jeux GOG possédés apparaissent dans **Mes jeux** ; l'installation GOG reste à faire.
- **Epic : « sortie stdout fournisseur trop volumineuse ou illisible » corrigé** :
  `legendary list --json` sérialise métadonnées, assets et succès — plusieurs mégaoctets, au-delà
  du plafond de 4 Mio du runner. La liste passe par `legendary list --csv` (quatre colonnes) et un
  parseur RFC 4180 maison (guillemets doublés, virgules, CRLF, DLC ignorés) ; l'ancien JSON reste
  accepté pour les versions qui ne connaissent pas `--csv`.
- **Synchronisation au démarrage, réglages à part** : les boutiques connectées se synchronisent
  seules une fois l'interface affichée, et **Paramètres → Boutiques & outils** ouvre sur une
  section *Synchronisation* (un bouton par boutique, « Tout synchroniser », nombre de jeux,
  dernière synchronisation, erreurs et états en cours). **Mes jeux** n'a plus de bouton de
  synchronisation : elle affiche le résultat.
- **Mes jeux en grille, un onglet par boutique** : Toutes / Steam / Epic Games / GOG avec
  compteurs, cartes en grille et actions regroupées par jeu.
- **Bannière de jeu sans fondu vers le noir** : le dégradé du hero est retiré, l'image garde son
  bas net ; le fond flouté reçoit un dégradé lissé (maillage à quatre sommets, sans bandes) pour
  que les actualités restent lisibles.
- **Animations** : fondu à chaque changement d'onglet, apparition en cascade des tuiles (fondu et
  léger soulèvement), couvertures soulevées au survol, pastille « EN JEU » qui respire pendant
  une partie, transitions des pages de réglages, spinners d'état.
- **Réglages réorganisés** : Apparence · Nouveaux jeux · Artworks · Boutiques & outils ·
  Statistiques · Espace disque · À propos, une phrase d'introduction par page, et fiche de jeu en
  LANCEMENT / MA FICHE / FICHE STEAM.

Limites assumées : la synchronisation GOG et la liste Epic n'ont pas pu être validées avec de
vrais comptes (les parseurs, le rafraîchissement de session et les cas d'erreur sont couverts par
des tests) ; GOG ne s'installe pas encore ; achievements et sauvegardes cloud restent absents
faute de source vérifiable.

## Quark 1.0.1 — installation de Legendary, connexions Epic/GOG et boutique refaite

- **Installation de Legendary réparée** : l'asset officiel `legendary_linux_x64` est un
  *zipapp* Python (`#!/usr/bin/env python3` suivi d'une archive zip), pas un binaire ELF — il
  était donc rejeté à juste titre par un contrôle trop strict. Le format est maintenant vérifié
  correctement (comme pour gogdl), `python3` est exigé explicitement, et l'ELF reste requis pour
  Ludusavi. Le téléchargement, le digest SHA-256, la comparaison de version et le remplacement
  atomique sont validés sur la vraie release 0.21.1.
- **Connexion Epic et GOG depuis le launcher** (Paramètres → Boutiques & outils) : la page
  officielle s'ouvre dans le navigateur, vous collez le code qu'elle affiche, Quark l'échange
  via Legendary ou gogdl. Aucun mot de passe ne transite par Quark, le code n'est ni journalisé
  ni réaffiché. Comme les deux outils sortent avec le code 0 même quand la boutique refuse le
  code, la connexion n'est déclarée réussie que si le fichier de session a réellement changé.
- **Boutique Steam verticale** : à la une, recommandations, populaires du moment (avec le pic de
  joueurs simultanés), meilleures ventes, offres spéciales, nouveautés et prochainement — en
  sections qui défilent vers le bas, plus jamais sur le côté. Les recommandations viennent de la
  Steam Web API (`IPlayerService/GetOwnedGames`) dès qu'une clé et un SteamID64 sont
  configurés, sinon de la bibliothèque Quark ; la source employée est affichée.
- **Plus de débordement à droite** : ligne de recherche de la boutique, rangées de « Mes jeux »
  et marges de page recalées, vérifiées en 1360 et 1024 px.
- **Documentation** : README mis à jour (format des outils, `python3` requis, procédure de
  connexion, sections de la boutique) et un aperçu CI de la connexion Epic.

Limites assumées : la connexion réelle à Epic et GOG n'a pas pu être menée de bout en bout sans
comptes (seuls le refus et le succès simulé sont couverts par des tests), et un quota GitHub
épuisé côté API peut encore bloquer le téléchargement d'un outil — sans digest publié, Quark
refuse d'installer à l'aveugle plutôt que de faire confiance à un binaire non vérifié. GOG reste
sans bibliothèque ni installation, cible Linux x86_64 uniquement.
