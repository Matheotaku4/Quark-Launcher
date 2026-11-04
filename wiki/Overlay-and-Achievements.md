# Overlay, captures et succès

> [Accueil](Home) · [Boutiques](Stores-and-Saves) · **Overlay et succès** ·
> [Réglages](Settings) · [Dépannage](Troubleshooting)

## Overlay en jeu

Lancez un jeu depuis Quark, puis appuyez sur **`Maj+Retour arrière`**. Un panneau
plein écran apparaît au-dessus du jeu avec :

- le nom du jeu et la boutique ;
- le chronomètre de session ;
- la progression des succès ;
- la liste des succès, verrouillés ou non selon le réglage ;
- la rareté et la progression quand la boutique les publie ;
- un indicateur des captures récentes.

Le même raccourci, `Échap` ou le bouton de fermeture referme l'overlay.

L'overlay n'est pas une fenêtre qui vole les clics en permanence : fermé, il
reste transparent à la souris. Il n'est proposé que pour un jeu lancé par Quark,
car Quark doit connaître le processus et la session.

### Réglages

**Paramètres → En jeu** :

- activer ou désactiver l'overlay ;
- choisir le raccourci global ;
- afficher ou masquer les succès verrouillés ;
- utiliser des lignes compactes sans description ;
- afficher ou masquer la progression et la rareté.

Un outil global X11 est utilisé pour intercepter le raccourci. Les jeux Proton
passent par XWayland et bénéficient du même chemin. Sur Wayland natif, le
compositeur peut interdire les raccourcis globaux : l'overlay reste accessible
par le bouton de fermeture et les captures intégrées, mais le raccourci peut ne
pas fonctionner.

## Captures d'écran

**`F12`** prend une capture du jeu en cours. Le fichier est écrit dans :

```text
~/.local/share/quark/screenshots/<jeu>/
```

- Les captures Epic et GOG déclenchent un son court et une notification de
  bureau.
- Les captures Steam restent silencieuses.
- La touche est modifiable dans **Paramètres → En jeu**.
- La capture passe par le chemin X11/XWayland ; sur Wayland natif, le
  compositeur peut demander une autorisation ou refuser.

Les captures ne sont pas envoyées sur un serveur. Elles restent des fichiers
locaux.

## Succès Epic

Epic utilise Legendary :

```bash
legendary achievements <app> --json
```

Le parseur accepte du JSON entouré de lignes de journal, une liste nulle et
l'ancien tableau texte (`Total achievements`, `Completed`, `In progress`). Si le
cache de métadonnées est incomplet, Quark lance `legendary info` puis réessaie
une fois. Une session absente ou un jeu sans succès produit un message explicite
au lieu d'une erreur illisible.

## Succès GOG

GOG interroge `gameplay.gog.com` avec la session gogdl. Les deux sources sont
normalisées vers les mêmes lignes : icône, nom, description, date de
déblocage, rareté et progression. Les icônes sont mises en cache localement.

Un succès débloqué pendant la session peut porter un badge **session**.

## API de succès locale

Un jeu ou un script peut signaler un déblocage via le petit serveur local :

```bash
curl -X POST "$QUARK_ACHIEVEMENTS_URL" \
  -H "Authorization: Bearer $QUARK_ACHIEVEMENTS_TOKEN" \
  -d '{"game":"'"$QUARK_GAME_ID"'","achievement":"ACH_FIRST","name":"First step"}'
```

- Le serveur n'écoute que sur `127.0.0.1`.
- Le jeton change à chaque lancement et est masqué dans le journal de session.
- Les variables `QUARK_ACHIEVEMENTS_URL`, `QUARK_ACHIEVEMENTS_TOKEN` et
  `QUARK_GAME_ID` sont fournies au jeu lancé.
- Le jeu ne doit jamais journaliser le jeton.

Cette API est destinée aux jeux qui ne fournissent pas de source de succès
exploitable. Elle ne remplace pas les succès officiels Steam.

## Overlay Epic (EOS)

Dans **Paramètres → Boutiques & outils → Overlay Epic (EOS)**, Quark peut
télécharger une fois l'overlay EOS et l'activer dans le préfixe du jeu. C'est le
mécanisme utilisé par Heroic : l'overlay Epic s'affiche en jeu avec
`Maj+F3`, indépendamment de l'overlay Quark.

Cette fonctionnalité dépend du jeu, du préfixe et de la version d'EOS. Elle
n'est pas nécessaire pour les succès : ceux-ci sont lus via Legendary, même
sans overlay EOS.

## Succès Steam

Steam ne fournit pas de source lisible des succès sans session Steam. Quark
affiche l'onglet Steam uniquement lorsqu'une source exploitable existe. Steam
Cloud et les succès Steam restent gérés par le client Steam.

## Dépannage

| Problème | Piste |
| --- | --- |
| Le raccourci n'ouvre rien | Vérifier que le jeu a été lancé par Quark, puis le raccourci dans **En jeu**. Sur Wayland natif, essayer XWayland. |
| L'overlay apparaît mais pas les succès | Vérifier la connexion Epic/GOG et le cache de métadonnées ; actualiser la fiche. |
| `F12` ne fait rien | Vérifier la touche et les permissions X11/Wayland ; essayer une capture via le compositeur. |
| Les captures ne sont pas au bon endroit | Vérifier `~/.local/share/quark/screenshots/` et les permissions du dossier. |
| Un succès local n'arrive pas | Vérifier l'URL, le jeton et `QUARK_GAME_ID` ; le serveur n'écoute que sur la boucle locale. |
| EOS ne s'affiche pas | Vérifier le préfixe actif, réinstaller l'overlay EOS, tester le jeu hors overlay Quark. |

## Voir aussi

- [Boutiques et sauvegardes](Stores-and-Saves)
- [Proton, Wine et options](Proton-Wine-and-Launch-Options)
- [Réglages](Settings)
- [Dépannage](Troubleshooting)

---

[Accueil](Home) · [Boutiques](Stores-and-Saves) · [Réglages](Settings) · [Dépannage](Troubleshooting)
