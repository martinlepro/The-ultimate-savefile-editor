# SaveStudio

Interface d'édition de sauvegardes en Rust (eframe/egui), un seul exécutable. Plateformes : **Linux, Windows, Android** (compilées automatiquement par GitHub ; macOS n'est pas géré).

**Ce dépôt ne contient que l'interface.** Les éditeurs de jeux (définitions `.sve`, emplacements des sauvegardes, thèmes, icônes de jeux, polices) vivent dans le dépôt
[The-ultimate-savefile-editor-games](https://github.com/martinlepro/The-ultimate-savefile-editor-games). Ajouter ou corriger un jeu s'y fait sans toucher à l'application.

## Éditeurs de jeux : téléchargement et mises à jour

- **Au premier lancement**, SaveStudio propose de télécharger les éditeurs de jeux.
- **À chaque démarrage**, il lit `manifest.json` du dépôt des jeux ; si la version est plus récente que celle installée, il **demande** à l'utilisateur s'il veut mettre à jour, puis installe si (et seulement si) il accepte. Désactivable dans *Settings → Game editors*, et un bouton *Check for updates* force la vérification.
- Chaque fichier est vérifié par **SHA-256** ; le nouveau pack est préparé à côté puis substitué d'un coup (en cas d'erreur, l'ancien reste intact).
- Si un pack exige une version de SaveStudio plus récente (`min_app`), l'application le dit et n'installe rien.
- Emplacement du pack : Windows `%APPDATA%\SaveStudio\pack`, Linux `~/.local/share/SaveStudio/pack`, Android `/storage/emulated/0/SaveStudio/pack` (variable `SAVESTUDIO_HOME` pour le déplacer).
- Essai d'un pack local sans rien télécharger : `savestudio --pack /chemin/du/depot-games --open <sauvegarde>`.

Les formats lus par le moteur sont `xml`, `gd`, `hk` et `nbt` ; un nouveau format de fichier demande une mise à jour de l'application. Le format des définitions est décrit dans le README du dépôt des jeux.

## Utilisation

Tapez le nom d'un jeu → ses sauvegardes sont détectées → on les édite → écriture atomique avec copie de sécurité. On peut aussi glisser-déposer un fichier dans la fenêtre.

| Raccourci | Effet |
|---|---|
| **Échap** | retour à l'écran précédent (réglages → éditeur → accueil ; ferme aussi les fenêtres) |
| **Ctrl+Z** | annule le dernier changement (glisser une valeur = un seul pas) |
| **Ctrl+Y** / **Ctrl+Maj+Z** | rétablit |

Chaque écran a un bouton **BACK** visible (dessiné, indépendant de la police du jeu).

## Réglages (écran *Settings*)

Tout est enregistré dans `settings.json` (même dossier que le pack) :

- **Polices** : celle de l'écran titre, celle de l'écran de réglages et celle de l'éditeur de sauvegarde (par défaut : celle de chaque jeu), à choisir parmi **toutes les polices disponibles** — celles du pack et **toutes les .ttf/.otf installées sur l'ordinateur** — avec recherche et aperçu.
- **Apparence** : taille de l'interface, taille du texte, thème de l'écran titre, capitales / casse normale, coins carrés / arrondis, couleurs personnalisées (fond, panneaux, accent, titre) par-dessus le thème de chaque jeu.
- **Disposition de l'éditeur** : onglets à gauche / à droite / en haut, boutons d'action en bas / en haut, espacement compact / normal / aéré, nombre de colonnes (1 à 3 ou selon la carte).
- Copie de sécurité avant écriture, vérification des mises à jour au démarrage, *Reset all settings*.

## Compilation automatique sur GitHub

1. Envoyez le contenu de ce dossier à la **racine** d'un dépôt GitHub (le dossier caché `.github/` doit y être).
2. À chaque envoi, l'onglet **Actions** lance le workflow *Build* :

   | Plateforme     | Fichier produit                    |
   |----------------|------------------------------------|
   | Linux x86_64   | `SaveStudio-linux-x86_64.tar.gz`   |
   | Windows x86_64 | `SaveStudio-windows-x86_64.zip`    |
   | Android arm64  | `SaveStudio-android-arm64.apk`     |
3. Téléchargement : *Actions* → l'exécution → **Artifacts**, ou page **Releases** (pré-version `nightly` à chaque envoi sur la branche par défaut ; un tag `v0.3.0` publie une version).

Remarques : **Windows** — SmartScreen peut avertir (exécutable non signé) → « Informations complémentaires » → « Exécuter quand même ». **Android** — l'APK est signé par une clé jetable ; pour pouvoir mettre à jour par-dessus, ajoutez les secrets `ANDROID_KEYSTORE_BASE64` et `ANDROID_KEYSTORE_PASSWORD`.

## Android

Android interdit de lire les données des autres applications : le dossier de partage est `/storage/emulated/0/SaveStudio/<Jeu>/` (voir `android/locations.svl` du dépôt des jeux). Activez une fois *Accès à tous les fichiers* pour SaveStudio. Pas de clavier virtuel pour l'instant (NativeActivity) : flèches, boutons prédéfinis, interrupteurs et listes fonctionnent, la saisie de texte non. Version expérimentale, non testée sur un appareil.

## Compiler soi-même

```
cargo build --release          # target/release/savestudio
cargo test --release           # tests (réglages, historique, manifeste, SHA-256, XML, GD, HK, NBT…)
SAVESTUDIO_TEST_PACK=/chemin/du/depot-games cargo test --release   # vérifie aussi que tous les .sve du pack se lisent
cargo test --release -- --ignored real_download                     # téléchargement réel depuis GitHub
```
Linux : paquets `libxkbcommon`, `libwayland`/X11, OpenGL. `tools/build_icons.py` régénère `assets/icons.txt` (icônes de l'interface ; celles des jeux viennent du pack).

## Sécurité des données

- Chaque écriture commence par une copie `savestudio-backups/<nom>.<horodatage>.bak` à côté de la sauvegarde (désactivable) ; le bouton **Backups** restaure la dernière.
- Écriture atomique (fichier temporaire + renommage). XML : seuls les textes modifiés changent, le reste reste identique octet pour octet.
- À utiliser sur des sauvegardes **solo / hors-ligne** uniquement.

## Polices

L'application n'embarque que **Pixelify Sans** (licence libre SIL OFL). Les autres polices viennent du pack ; Trajan Pro (Hollow Knight) est une police commerciale d'Adobe et CELESTE.otf a une licence inconnue : voyez la note du dépôt des jeux avant de rendre ce dépôt public.
