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
