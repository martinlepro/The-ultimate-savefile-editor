# SaveStudio

Interface d'édition de sauvegardes en Rust (eframe/egui), un seul exécutable. Plateformes : **Linux, Windows, Android** (compilées automatiquement par GitHub ; macOS n'est pas géré).

**Ce dépôt ne contient que l'interface : le style, la personnalisation et les fonctions « en plus »** (partage via Tailscale, annuler, réglages…).
Tout ce qui touche aux jeux vit dans le dépôt [The-ultimate-savefile-editor-games](https://github.com/martinlepro/The-ultimate-savefile-editor-games) :
la **disposition** de chaque éditeur (`.sve`), les **polices**, les thèmes et icônes de jeux, les emplacements des sauvegardes et le **code qui comprend les sauvegardes**
(les codecs xml / gd / hk / nbt, compilés en WebAssembly et exécutés dans un bac à sable sans accès au système). Ajouter ou corriger un jeu, ou un bug de lecture, s'y fait sans toucher à l'application.

## Éditeurs de jeux : téléchargement et mises à jour

- **Au premier lancement**, SaveStudio propose de télécharger les éditeurs de jeux.
- **À chaque démarrage**, il lit `manifest.json` du dépôt des jeux ; si la version est plus récente que celle installée, il **demande** à l'utilisateur s'il veut mettre à jour, puis installe si (et seulement si) il accepte. Désactivable dans *Settings → Game editors*, et un bouton *Check for updates* force la vérification.
- Chaque fichier (y compris le module de codecs) est vérifié par **SHA-256** ; le nouveau pack est préparé à côté puis substitué d'un coup (en cas d'erreur, l'ancien reste intact).
- Si un pack exige une version de SaveStudio plus récente (`min_app`), l'application le dit et n'installe rien.
- Emplacement du pack : Windows `%APPDATA%\SaveStudio\pack`, Linux `~/.local/share/SaveStudio/pack`, Android `/storage/emulated/0/SaveStudio/pack` (variable `SAVESTUDIO_HOME` pour le déplacer).
- Essai d'un pack local sans rien télécharger : `savestudio --pack /chemin/du/depot-games --open <sauvegarde>`.

L'application ne sait lire aucun format toute seule : elle charge `wasm/savecodecs.wasm` (vérifié par SHA-256, exécuté avec l'interpréteur `wasmi`, sans aucune importation donc sans accès aux fichiers ni au réseau). Le format des définitions et l'ABI des codecs sont décrits dans le README du dépôt des jeux.

## Utilisation

Tapez le nom d'un jeu → ses sauvegardes sont détectées → on les édite → écriture atomique avec copie de sécurité. On peut aussi glisser-déposer un fichier dans la fenêtre.

| Raccourci | Effet |
|---|---|
| **Échap** | retour à l'écran précédent (réglages → éditeur → accueil ; ferme aussi les fenêtres) |
| **Ctrl+Z** | annule le dernier changement (glisser une valeur = un seul pas) |
| **Ctrl+Y** / **Ctrl+Maj+Z** | rétablit |

Chaque écran a un bouton **BACK** visible (dessiné, indépendant de la police du jeu).

## Partage de sauvegardes entre ordinateurs (Tailscale)

Écran **SYNC** (accueil). Il faut [Tailscale](https://tailscale.com) sur chaque ordinateur ; SaveStudio n'utilise que le réseau privé chiffré qu'il crée entre vos appareils, jamais Internet.

1. Sur l'ordinateur qui a la sauvegarde : *Sync* → activer **Let my other computers use my saves**. SaveStudio écoute alors **uniquement** sur l'adresse Tailscale de la machine (détectée avec `tailscale status`) et refuse toute connexion qui ne vient pas du réseau Tailscale.
2. Copiez le **mot de passe partagé** (bouton COPY) et collez-le dans l'écran Sync de l'autre ordinateur ; choisissez la machine dans la liste de vos appareils Tailscale (ou tapez son adresse `100.x.y.z`) puis **CONNECT**.
3. Le tableau compare les sauvegardes des deux côtés (identifiées par jeu + chemin relatif, identique sous Windows, Linux et Android) : *In sync*, *newer here/there*, *only here/there*. **PULL** copie ici, **PUSH** remplace là-bas (seulement si l'autre ordinateur a coché *Allow them to replace my saves*). Un fichier compagnon (ex. `SaveGameInfo` de Stardew) voyage avec sa sauvegarde.

Garde-fous : mot de passe obligatoire sur chaque requête ; écriture désactivée par défaut ; un fichier reçu n'est écrit que s'il a la forme d'une sauvegarde du jeu (jamais d'arbitraire) ; empreinte SHA-256 vérifiée ; copie de sécurité avant tout remplacement ; confirmation avant d'écraser.
Android n'a pas la commande `tailscale` : saisissez l'adresse Tailscale de l'appareil (visible dans l'appli Tailscale) dans *Tailscale address*.

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
cargo test --release           # tests (réglages, historique, manifeste, SHA-256, partage entre deux machines…)
SAVESTUDIO_TEST_PACK=/chemin/du/depot-games cargo test --release   # vérifie aussi les .sve et le module wasm du pack
cargo test --release -- --ignored real_download                     # téléchargement réel depuis GitHub
```
Linux : paquets `libxkbcommon`, `libwayland`/X11, OpenGL. `tools/build_icons.py` régénère `assets/icons.txt` (icônes de l'interface ; celles des jeux viennent du pack).

## Sécurité des données

- Chaque écriture commence par une copie `savestudio-backups/<nom>.<horodatage>.bak` à côté de la sauvegarde (désactivable) ; le bouton **Backups** restaure la dernière.
- Écriture atomique (fichier temporaire + renommage). XML : seuls les textes modifiés changent, le reste reste identique octet pour octet.
- À utiliser sur des sauvegardes **solo / hors-ligne** uniquement.

## Polices

L'application n'embarque que **Pixelify Sans** (licence libre SIL OFL). Les autres polices viennent du pack ; Trajan Pro (Hollow Knight) est une police commerciale d'Adobe et CELESTE.otf a une licence inconnue : voyez la note du dépôt des jeux avant de rendre ce dépôt public.
