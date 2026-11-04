# Dépannage

> [Accueil](Home) · [Installation](Installation) ·
> [Proton et Wine](Proton-Wine-and-Launch-Options) · **Dépannage**

Avant de modifier un préfixe ou de supprimer un fichier, faites une copie du
dossier concerné. La plupart des problèmes se diagnostiquent avec un journal,
pas en réinitialisant la bibliothèque.

## Vérifier l'environnement

```bash
quark --doctor
```

Cette commande vérifie les éléments locaux dont Quark a besoin : affichage,
outils et chemins. Elle ne modifie rien.

Pour un lancement hors interface :

```bash
quark --search "Hollow Knight"
quark --news 367520
```

## Un jeu ne démarre pas

1. Ouvrez la fiche du jeu et vérifiez **Paramètres du jeu**.
2. Vérifiez que l'exécutable existe encore. Un jeu déplacé ou désinstallé laisse
   une fiche avec un chemin mort ; Quark le signale en rouge.
3. Vérifiez le runtime Proton. **Automatique** choisit un runtime installé ; si
   aucun runtime n'est détecté, installez Proton dans Steam ou déposez un build
   dans `compatibilitytools.d/`.
4. Activez **PROTON_LOG** et relancez.
5. Lisez le journal dans `~/.local/share/quark/logs/<jeu>/`.
6. Essayez un autre runtime Proton avant de toucher au préfixe.

### Erreurs fréquentes

| Message | Cause probable | Action |
| --- | --- | --- |
| `Exécutable introuvable` | Chemin déplacé ou jeu désinstallé | Corriger l'exécutable dans la fiche. |
| `Runtime ... introuvable` | Runtime Proton supprimé | Choisir un autre runtime ou réinstaller Proton. |
| `umu-run introuvable` | umu-launcher absent | Installer umu ou décocher l'option. |
| `gamemoderun n'est pas installé` | GameMode absent | Installer GameMode ou décocher l'option ; le jeu démarre quand même. |
| `gamescope n'est pas installé` | gamescope absent | Installer gamescope ou décocher l'option. |
| Anti-triche introuvable | Runtime BattlEye/EAC absent | Installer le runtime Steam correspondant, puis redétecter. |

Un outil graphique absent est signalé et ignoré : Quark continue sans lui plutôt
que d'échouer. Un runtime Proton absent, lui, empêche le lancement.

## Écran noir, crash ou retour au bureau

- Activez `PROTON_LOG=1` et lisez les dernières lignes du journal.
- Essayez un autre runtime Proton.
- Essayez `PROTON_USE_WINED3D=1` uniquement comme test de compatibilité : si le
  jeu démarre, le problème vient probablement de DXVK/VKD3D ou du pilote.
- Mettez à jour le pilote graphique.
- Vérifiez que le jeu n'a pas besoin d'une DLL ou d'un composant Windows
  supplémentaire. Quark ne lance pas `winetricks` automatiquement.
- Ne supprimez pas le préfixe immédiatement : déplacez-le d'abord pour pouvoir
  revenir en arrière.

## Manette, son, souris

- **Manette** : vérifier udev/Steam Input, essayer sans gamescope, vérifier le
  journal. Un jeu qui utilise Steam Input peut nécessiter un lancement Steam.
- **Son** : essayer `PULSE_LATENCY_MSEC=60`, vérifier le périphérique par défaut
  du système, puis tester sans wrapper.
- **Souris capturée** : essayer le mode fenêtre du jeu, `-windowed` dans les
  arguments, ou désactiver gamescope.
- **Mauvaise GPU** : décocher PRIME sur un desktop mono-GPU, ou vérifier
  `DRI_PRIME` sur un portable AMD.

## L'interface ne s'ouvre pas

- Vérifier que `DISPLAY` ou `WAYLAND_DISPLAY` est défini.
- Essayer sous X11/XWayland si le compositeur Wayland refuse la création de la
  fenêtre.
- Vérifier OpenGL et les pilotes.
- Lancer `quark --doctor`.
- Consulter la sortie du terminal : une erreur de chargement de police ou de
  contexte graphique y apparaît.

## Problème de langue

- L'interface se change dans **Paramètres → Apparence → Langue de
  l'interface**.
- La langue des fiches Steam est un réglage séparé dans **Boutiques & outils**.
- Les noms de jeux, tags et genres personnels ne sont pas traduits.
- Si un texte reste en français après le passage en anglais, c'est probablement
  une donnée utilisateur ou une chaîne nouvellement ajoutée sans entrée de
  catalogue. Notez la page et le texte exact pour le signaler.

## Le fond hero aléatoire ne s'affiche pas

- Il faut au moins un artwork de type bannière dans la bibliothèque.
- Le fond est dessiné derrière l'accueil et la bibliothèque ; il peut être
  discret sous les cartes et le voile de lisibilité.
- Le tirage change au redémarrage, pas à chaque navigation.
- Augmentez le flou ou vérifiez que l'option est active dans **Apparence**.

## Paramètres

Depuis la 1.2, les réglages remplissent la fenêtre et défilent. Si le bouton
**Fermer** n'est pas visible, `Échap` ferme la page. Si le contenu semble coupé,
redimensionnez la fenêtre : la mise en page est responsive mais certains
blocs demandent une largeur minimale.

## Legendary, gogdl et comptes

- **Legendary introuvable** : vérifier `python3`, puis réinstaller l'outil dans
  **Boutiques & outils**. Quark vérifie la somme SHA-256 avant exécution.
- **Connexion Epic refusée** : le code d'autorisation est à durée limitée ;
  recommencez la connexion.
- **Session GOG expirée** : reconnecter GOG ; le jeton est dans un fichier
  séparé, avec permissions privées.
- **Un jeu de boutique n'apparaît pas** : lancer une synchronisation, vérifier
  que le compte est connecté et que le jeu est bien possédé.
- **Téléchargement bloqué** : annuler puis relancer ; vérifier l'espace disque
  et le dossier d'installation.

## Sauvegardes cloud

- **Epic** : `legendary sync-saves` compare le local et le cloud, le plus récent
  gagne. Un dossier personnalisé peut être nécessaire.
- **GOG** : gogdl exige un dossier de sauvegarde explicite. Sans lui, la
  synchronisation ne peut pas fonctionner.
- **Steam** : Steam Cloud reste géré par Steam ; Quark ne le synchronise pas.
- **Conflit** : sauvegardez le dossier local avant de laisser le cloud écraser
  quoi que ce soit. Quark n'invente jamais une sauvegarde absente.

## Instantanés Ludusavi

- **Aucun jeu détecté** : vérifier la configuration Ludusavi et le dossier de
  sauvegarde.
- **Restauration refusée** : les liens symboliques, `$HOME` comme racine et les
  chemins hors des racines autorisées sont volontairement refusés.
- **Mauvais instantané restauré** : chaque installation de boutique a son propre
  compartiment ; vérifiez l'installation active.
- **Configuration illisible** : Quark la conserve et signale l'erreur ; ne
  l'écrasez pas avec une configuration vide.

## Mise à jour bloquée

- **Binaire de développement** : la mise à jour automatique est ignorée.
- **Dossier en lecture seule** : télécharger l'archive et remplacer le binaire
  manuellement.
- **Jeu en cours** : fermer le jeu avant d'installer.
- **Somme invalide** : l'archive est refusée. Retélécharger l'archive et le
  fichier `.sha256` depuis la release.
- **Ancien binaire conservé** : Quark garde l'exécutable précédent jusqu'au
  prochain démarrage réussi.

Voir [Installation](Installation) pour la procédure manuelle.

## Fichier de configuration

`~/.config/quark/config.json` contient les jeux, installations, réglages et
statistiques. En cas de corruption :

- Quark copie le fichier en `config.json.corrompu-<horodatage>` ;
- la bibliothèque passe en lecture seule pour ne pas écraser les données ;
- un fichier vide n'est jamais écrit à la place.

Ne supprimez pas ce fichier pour résoudre un problème de lancement : le problème
est presque toujours dans le préfixe, le runtime ou les fichiers du jeu, pas dans
la bibliothèque.

## Journaux et confidentialité

Les journaux de session sont dans `~/.local/share/quark/logs/<jeu>/`, en mode
`0600`. Ils ne contiennent pas les valeurs d'environnement complètes, les lignes
de commande intégrales ni le jeton de succès. Avant de partager un journal,
relisez-le : un jeu peut y écrire des chemins personnels.

## Signaler un bug

Indiquez :

- la version de Quark (visible dans **À propos**) ;
- la distribution, la session X11/Wayland et la version du pilote ;
- le runtime Proton et les options activées ;
- les dernières lignes du journal Proton ;
- les étapes reproductibles.

Ne joignez pas `credentials.json`, `user.json`, `gogdl-auth.json` ni un jeton
d'API. Ces fichiers contiennent des secrets de compte.

## Voir aussi

- [Installation](Installation)
- [Proton, Wine et options de lancement](Proton-Wine-and-Launch-Options)
- [Boutiques et sauvegardes](Stores-and-Saves)
- [Overlay et succès](Overlay-and-Achievements)

---

[Accueil](Home) · [Installation](Installation) · [Proton](Proton-Wine-and-Launch-Options) · [Boutiques](Stores-and-Saves)
