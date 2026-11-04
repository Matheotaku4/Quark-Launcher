# Proton, Wine et options de lancement

> [Accueil](Home) · [Premier démarrage](Getting-Started) ·
> **Proton et Wine** · [Boutiques](Stores-and-Saves) · [Dépannage](Troubleshooting)

Cette page décrit ce que Quark fait réellement quand vous cliquez sur
**Jouer**, l'ordre des enveloppes, chaque option de la fiche du jeu et les
variables d'environnement utiles. Les exemples sont des points de départ : un
jeu peut avoir besoin d'un réglage différent.

## Wine, Proton et umu : les mots justes

| Terme | Rôle |
| --- | --- |
| **Wine** | Couche de compatibilité qui exécute un programme Windows sur Linux. |
| **Proton** | Distribution de Wine, DXVK, VKD3D et correctifs, conçue pour Steam. Quark lance le script `proton` d'un runtime installé. |
| **Préfixe** | Dossier qui contient un faux disque Windows (`drive_c`), la base de registre et les composants installés. Les sauvegardes et réglages d'un jeu y vivent souvent. |
| **Runtime** | Version de Proton installée dans Steam (`Proton 9.0`, `Proton Experimental`, un build `compatibilitytools.d`, Flatpak ou Snap). |
| **umu-launcher** | Lanceur qui applique les correctifs de la base umu et peut utiliser UMU-Proton sans Steam. |
| **DXVK** | Traduit Direct3D 9/10/11 en Vulkan. |
| **VKD3D-Proton** | Traduit Direct3D 12 en Vulkan. |

Quark **ne remplace pas** un Wine système. Le mode normal passe par un runtime
Proton détecté, ou par umu. Un wrapper peut appeler `wine` manuellement, mais
c'est un cas avancé : Quark ne gère alors ni le préfixe, ni les variables
Proton, ni les runtimes anti-triche.

## Comment Quark choisit le mode de lancement

1. **Natif** : le jeu est explicitement marqué natif.
2. **Runtime Proton précis** : le jeu utilise ce runtime.
3. **Automatique** : Quark regarde l'extension de l'exécutable. Un `.exe`,
   `.bat` ou `.msi` part en Proton ; un binaire Linux reste natif.

Si **umu-launcher** est coché sur un jeu Windows, Quark lance `umu-run` au lieu
du script Proton. Sur un jeu natif, l'option umu est ignorée avec un
avertissement.

### Ordre de la commande

De l'extérieur vers l'intérieur :

```text
wrapper utilisateur
  └─ gamescope (si coché)
      └─ gamemoderun (si coché)
          └─ proton run <jeu> <arguments>   (ou umu-run)
```

Le wrapper est une ligne shell : `%command%` est remplacé par la commande
complète déjà enveloppée. Sans `%command%`, la commande est simplement
préfixée.

Les arguments du jeu sont placés après l'exécutable, donc à l'intérieur de
gamescope et de GameMode. Les paramètres ajoutés par une boutique (Epic) sont
ajoutés après les arguments du jeu et ne sont pas modifiés par Quark.

## Options de la fiche du jeu

Ouvrez la fiche, puis **Paramètres du jeu** (ou l'engrenage de la fiche).

### Runtime et préfixe

- **Runtime Proton** : choisit le build. **Automatique** laisse Quark décider.
- **Préfixe** : dossier utilisé pour ce jeu. Quark l'utilise tel quel et ne crée
  pas de sous-dossier. Laisser vide crée
  `~/.local/share/quark/prefixes/<jeu>`.
- **umu-launcher** : lance via `umu-run`.
- **GAMEID umu** : identifiant de correctifs umu. Vide devient `0`.
- **Anti-triche** : BattlEye ou EasyAntiCheat. Quark cherche le runtime Steam
  correspondant et le transmet à Proton.

Changer de préfixe revient à changer de « machine Windows » : les composants,
la base de registre et parfois les sauvegardes ne suivent pas. Sauvegardez avant
de déplacer un préfixe.

### Options graphiques et système

| Option | Ce que Quark exécute | Effet |
| --- | --- | --- |
| **GameMode** | `gamemoderun <commande>` | Demande au démon GameMode d'appliquer son profil de performance. |
| **MangoHud** | `MANGOHUD=1` | Active la couche Vulkan/OpenGL MangoHud ; le paquet doit être installé. |
| **gamescope** | `gamescope -W <largeur> -H <hauteur> -f [ -F fsr ] [--framerate-limit N] -- <commande>` | Compositeur de jeu, redimensionnement, FSR et limite d'images. |
| **Résolution gamescope** | largeur × hauteur | Taille de la fenêtre/compositeur. |
| **FSR gamescope** | `-F fsr` | Active l'upscale FSR de gamescope. |
| **Limite d'images** | `--framerate-limit N` | 0 désactive la limite ; la valeur est bornée à 1000. |
| **PRIME** | Variables NVIDIA/AMD ci-dessous | Force le rendu sur la GPU dédiée d'un portable hybride. |
| **PROTON_LOG** | `PROTON_LOG=1`, `PROTON_LOG_DIR=...` | Écrit un journal Proton dans le dossier de logs du jeu. |
| **Wrapper** | `sh -c "<wrapper>"` | Commande libre autour du jeu. |

**GameMode** et **MangoHud** sont indépendants : GameMode règle la performance,
MangoHud affiche les mesures. **gamescope** impose le plein écran avec `-f` ;
pour une fenêtre, n'utilisez pas la case et passez par un wrapper gamescope
personnalisé.

## Variables posées automatiquement par Quark

Vous n'avez normalement pas à les ajouter. Si vous les mettez aussi dans vos
variables, les valeurs requises par Quark sont ajoutées après les vôtres et
gagnent.

| Variable | Quand | Rôle |
| --- | --- | --- |
| `WINEPREFIX` | umu uniquement | Préfixe transmis à umu. |
| `STEAM_COMPAT_DATA_PATH` | Proton | Dossier de compatibilité (le préfixe) utilisé par le script Proton. |
| `STEAM_COMPAT_CLIENT_INSTALL_PATH` | Proton | Racine Steam, pour que Proton trouve ses composants. |
| `GAMEID` | umu | Identifiant umu ; `0` si vide. |
| `PROTONPATH` | umu avec runtime épinglé | Dossier du Proton choisi. |
| `PROTON_LOG` / `PROTON_LOG_DIR` | case PROTON_LOG | Journal Proton dans les logs Quark. |
| `MANGOHUD` | case MangoHud | Active MangoHud. |
| `PROTON_BATTLEYE_RUNTIME` | anti-triche BattlEye | Dossier du runtime BattlEye. |
| `PROTON_EAC_RUNTIME` | anti-triche EasyAntiCheat | Dossier du runtime EAC. |
| `__NV_PRIME_RENDER_OFFLOAD`, `__GLX_VENDOR_LIBRARY_NAME`, `__VK_LAYER_NV_optimus`, `DRI_PRIME` | case PRIME | Offload NVIDIA et AMD. |

Les variables de l'utilisateur sont dédupliquées sans tenir compte de la casse :
la dernière valeur valide d'un même nom gagne.

## Variables utiles à ajouter à la main

Dans **Paramètres du jeu → Variables d'environnement**, ajoutez une ligne par
variable. Les valeurs sont sensibles à la casse. Ne mettez pas de guillemets
autour de la valeur dans les champs Quark.

### Graphismes et rendu

| Variable | Valeur typique | Ce que ça change |
| --- | --- | --- |
| `DXVK_HUD` | `fps,frametimes,gpuload` | Affiche un HUD DXVK : images/s, temps d'image, charge GPU. `compiler` ajoute la compilation de shaders, `memory` la mémoire. |
| `DXVK_CONFIG_FILE` | `/chemin/dxvk.conf` | Charge un fichier de configuration DXVK (options par jeu). |
| `VKD3D_CONFIG` | `dxr` | Active le ray tracing D3D12 si le build VKD3D et le pilote le supportent. `dxr11` pour la variante DXR 1.1 selon le build. |
| `VKD3D_FEATURE_LEVEL` | `12_1` | Force le niveau de fonctionnalités D3D12 exposé. Peut empêcher un jeu de démarrer si trop élevé. |
| `PROTON_USE_WINED3D` | `1` | Remplace DXVK par wined3d (OpenGL). Compatibilité de secours, généralement plus lent. |
| `WINE_FULLSCREEN_FSR` | `1` | Active l'upscale FSR de Wine pour les jeux plein écran compatibles. |
| `WINE_FULLSCREEN_FSR_STRENGTH` | `2` | Netteté FSR de 0 (max) à 5 (min). |
| `MANGOHUD_CONFIG` | `fps_only=1` | Options MangoHud quand `MANGOHUD=1` est actif. |
| `mesa_glthread` | `true` | Active le threading OpenGL Mesa pour les jeux OpenGL/wined3d. |
| `__GL_SHADER_DISK_CACHE` | `1` | Cache de shaders NVIDIA sur disque. |
| `__GL_SHADER_DISK_CACHE_SKIP_CLEANUP` | `1` | Évite le nettoyage du cache NVIDIA ; utile pour les jeux à compilation longue. |

### Synchronisation et performance

| Variable | Valeur | Ce que ça change |
| --- | --- | --- |
| `PROTON_NO_ESYNC` | `1` | Désactive esync. À essayer si le jeu se bloque au démarrage. |
| `PROTON_NO_FSYNC` | `1` | Désactive fsync. Même usage, en dernier recours. |
| `WINEESYNC` | `0` ou `1` | Contrôle esync quand le build le permet ; Proton le règle généralement seul. |
| `WINEFSYNC` | `0` ou `1` | Contrôle fsync quand le build le permet. |
| `PULSE_LATENCY_MSEC` | `60` | Augmente la latence audio PulseAudio pour éviter les craquements. |
| `PROTON_FORCE_LARGE_ADDRESS_AWARE` | `1` | Donne plus d'espace d'adressage aux jeux 32 bits ; peut corriger un manque de mémoire, à manier avec prudence. |
| `SteamDeck` | `0` ou `1` | Certains jeux changent d'interface ou de profil selon ce drapeau. `1` peut activer un profil portable non voulu sur desktop. |

### Compatibilité et diagnostics

| Variable | Valeur | Ce que ça change |
| --- | --- | --- |
| `WINEDLLOVERRIDES` | `dll=n,b` | Force l'ordre native/builtin d'une DLL. `n,b` charge la DLL native puis celle de Wine ; `b` force celle de Wine. Une syntaxe incorrecte peut empêcher le jeu de démarrer. |
| `WINEDEBUG` | `-all` | Coupe les messages de debug Wine. Utile quand les logs sont énormes. |
| `WINEARCH` | `win64` | Architecture d'un **nouveau** préfixe. Ne le changez pas sur un préfixe existant : cela peut le rendre inutilisable. |
| `PROTON_LOG` | `0`/`1` | Même effet que la case, mais permet de le poser dans le wrapper. |
| `PROTON_LOG_DIR` | `/chemin` | Destination du journal Proton. |
| `PROTON_ENABLE_NVAPI` | `1` | Active NVAPI sur les builds Proton qui le demandent, pour DLSS/Reflex. Inutile ou nuisible selon la version. |
| `PULSE_SERVER` | socket | Force un serveur PulseAudio/PipeWire précis, pour les setups audio exotiques. |

### umu-launcher

| Variable | Valeur | Ce que ça change |
| --- | --- | --- |
| `GAMEID` | `0` ou `umu-<id>` | Identifiant utilisé par umu pour choisir des correctifs. Laissez `0` si le jeu n'est pas dans la base. |
| `PROTONPATH` | `/chemin/Proton` | Force le runtime Proton utilisé par umu. Quark le pose quand un runtime est épinglé. |
| `UMU_LOG` | `1` | Journal détaillé de umu, utile quand `umu-run` échoue avant Proton. |

### Anti-triche

Les runtimes BattlEye/EasyAntiCheat sont des dossiers Steam, pas des versions de
Proton complètes. Quark les transmet par `PROTON_BATTLEYE_RUNTIME` ou
`PROTON_EAC_RUNTIME`. Un jeu protégé peut malgré tout refuser de fonctionner :
le support dépend du jeu, du serveur et du runtime, pas seulement de Quark.

## Arguments de lancement

Les arguments sont ajoutés après l'exécutable. Ils dépendent du jeu :

- `-windowed`, `-fullscreen`, `-w 1920 -h 1080` ;
- `-novid` pour sauter certaines vidéos ;
- `-dx11`, `-force-d3d11`, `-d3d12` ;
- `--launcher-skip` pour certains jeux récents.

Un argument contenant une espace doit être écrit avec des guillemets dans le
champ Quark, par exemple `"-username" "Nom"`. Les arguments ne sont pas des
variables : `FOO=1` dans ce champ sera probablement ignoré par le jeu. Utilisez
la section Variables pour l'environnement.

## Wrapper : exemples

Le wrapper est une ligne shell exécutée avec `sh -c`. `%command%` représente la
commande complète, déjà enveloppée par gamescope/GameMode si ces cases sont
cochées.

```bash
# Forcer un HUD DXVK
env DXVK_HUD=fps,frametimes %command%
```

```bash
# Netteté FSR plein écran
env WINE_FULLSCREEN_FSR=1 WINE_FULLSCREEN_FSR_STRENGTH=2 %command%
```

```bash
# gamescope manuel en fenêtre 1600x900
gamescope -W 1600 -H 900 -f -- %command%
```

```bash
# Rediriger la sortie du jeu vers un fichier
%command% > "$HOME/quark-game.log" 2>&1
```

**Ne combinez pas** un wrapper `gamescope` avec la case gamescope : vous
obtiendriez deux compositeurs imbriqués. De même, `gamemoderun` dans le wrapper
et la case GameMode se cumulent.

`%command%` contient déjà des arguments potentiellement espacés. Si vous
construisez une commande complexe, citez les chemins et testez d'abord sans
wrapper.

## Gamescope en détail

La case gamescope produit approximativement :

```bash
gamescope -W 2560 -H 1440 -f -F fsr --framerate-limit 60 -- <commande>
```

- `-W`/`-H` : taille logique du compositeur.
- `-f` : plein écran.
- `-F fsr` : upscale FSR.
- `--framerate-limit` : limite les images, utile sur portable ou pour stabiliser
  la consommation.

gamescope doit être installé et accessible dans le `PATH`. S'il manque, Quark
l'indique et continue sans lui plutôt que d'échouer.

## PRIME et portables hybrides

La case PRIME pose :

```text
__NV_PRIME_RENDER_OFFLOAD=1
__GLX_VENDOR_LIBRARY_NAME=nvidia
__VK_LAYER_NV_optimus=NVIDIA_only
DRI_PRIME=1
```

Elle force le rendu sur la GPU dédiée. Sur certains portables AMD, seul
`DRI_PRIME=1` est nécessaire ; sur un desktop mono-GPU, la case peut être sans
effet ou perturber le choix du pilote. Décochez-la si le jeu démarre sur la
mauvaise GPU.

## Réinitialiser un préfixe sans perdre les sauvegardes

Un préfixe corrompu peut empêcher tous les lancements. Avant de le supprimer :

1. Fermez le jeu et Quark.
2. Localisez le préfixe dans la fiche du jeu.
3. **Déplacez-le** au lieu de le supprimer :

   ```bash
   mv "$PREFIX" "$PREFIX.avant-reset-$(date +%Y%m%d)"
   ```

4. Relancez le jeu : Proton recrée un préfixe propre.
5. Si les sauvegardes étaient dans l'ancien préfixe, copiez uniquement le
   dossier de sauvegarde nécessaire, pas tout `drive_c`.

Ne supprimez jamais un préfixe « pour gagner de la place » sans avoir vérifié
où le jeu écrit ses sauvegardes. Pour les jeux Epic/GOG, les sauvegardes cloud
peuvent aussi être dans le dossier de sauvegarde de l'installation.

## Symptômes courants

| Symptôme | À essayer |
| --- | --- |
| Écran noir puis retour au bureau | Lancer avec `PROTON_LOG=1`, lire le journal, essayer un autre runtime Proton. |
| Le jeu ne voit pas la manette | Vérifier Steam Input/udev, essayer sans gamescope, consulter le journal. |
| Pas de son | Tester `PULSE_LATENCY_MSEC=60`, vérifier le périphérique par défaut, essayer sans wrapper. |
| Crash au démarrage après un changement de préfixe | Remettre l'ancien préfixe ou en créer un neuf, puis restaurer la sauvegarde. |
| Performances faibles | Vérifier PRIME, MangoHud, la limite gamescope, puis essayer un runtime plus récent. |
| Le jeu refuse de se lancer avec BattlEye/EAC | Vérifier que le runtime anti-triche Steam est installé et sélectionné ; tous les jeux ne sont pas supportés. |
| Le jeu démarre mais la souris est capturée | Essayez `-windowed`, ou le mode fenêtre sans frontières proposé par le jeu. |
| Un module Windows manque | `winetricks` peut être utilisé dans le préfixe, mais Quark ne l'automatise pas. Sauvegardez le préfixe avant. |

## Voir aussi

- [Boutiques et sauvegardes](Stores-and-Saves) pour les préfixes et comptes.
- [Dépannage](Troubleshooting) pour les erreurs qui ne viennent pas du jeu.
- [Réglages](Settings) pour les valeurs par défaut des nouveaux jeux.

---

[Accueil](Home) · [Boutiques](Stores-and-Saves) · [Overlay](Overlay-and-Achievements) · [Dépannage](Troubleshooting)
