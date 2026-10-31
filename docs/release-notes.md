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
