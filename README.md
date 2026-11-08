# SaveStudio

Éditeur de sauvegardes en Rust (eframe/egui), un seul exécutable (~5 Mo), hors-ligne.
Plateformes : **Linux, Windows, macOS (Apple Silicon et Intel), Android** — compilées automatiquement par GitHub (voir plus bas).
Tapez le nom d'un jeu → ses sauvegardes sont détectées → on les édite → écriture atomique avec sauvegarde de sécurité.

Jeux fournis : **Stardew Valley** (police Pixelify Sans), **Celeste** (CELESTE.otf), **Hollow Knight** (Trajan Pro),
**Geometry Dash** (police « blocs » reconstruite depuis votre feuille de glyphes), **Minecraft Java** (VT323).

## Compilation automatique sur GitHub

1. Créez un dépôt GitHub et envoyez **le contenu** de ce dossier à la **racine** du dépôt
   (le dossier caché `.github/` doit être à la racine ; avec `git` : `git add -A && git commit -m init && git push`).
   Si l'envoi par le navigateur ignore `.github`, créez le fichier via *Add file → Create new file*
   avec le nom `.github/workflows/build.yml` et collez-y le contenu du fichier.
2. À chaque envoi, l'onglet **Actions** lance le workflow *Build* (de quelques minutes à ~15 min la première fois) :

   | Plateforme | Fichier produit |
   |---|---|
   | Linux x86_64 | `SaveStudio-linux-x86_64.tar.gz` |
   | Windows x86_64 | `SaveStudio-windows-x86_64.zip` |
   | macOS Apple Silicon / Intel | `SaveStudio-macos-arm64.tar.gz` / `SaveStudio-macos-x86_64.tar.gz` |
   | Android arm64 | `SaveStudio-android-arm64.apk` |

3. Téléchargement : dans *Actions* → l'exécution → **Artifacts**, ou sur la page **Releases** :
   une pré-version `nightly` est mise à jour à chaque envoi sur la branche par défaut, et créer un tag
   (`git tag v0.1.0 && git push --tags`, ou *Releases → Draft a new release*) publie une version `v0.1.0`.
   Une plateforme qui échoue n'empêche pas les autres d'être publiées.

Remarques :
* **macOS** : binaires non signés ; au premier lancement, clic droit → Ouvrir, ou `xattr -d com.apple.quarantine SaveStudio`.
* **Windows** : SmartScreen peut avertir (exécutable non signé) → « Informations complémentaires » → « Exécuter quand même ».
* **Android** : l'APK est signé par une clé jetable générée à chaque build ; pour pouvoir installer les mises à jour
  par-dessus, ajoutez dans *Settings → Secrets and variables → Actions* `ANDROID_KEYSTORE_BASE64`
  (`base64 -w0 ma.keystore`) et `ANDROID_KEYSTORE_PASSWORD` (mot de passe du keystore, identique pour la clé ; un seul alias dedans).

## Android

L'interface est la mise en page « bureau » réduite pour tenir dans l'écran : confortable en paysage / sur tablette,
plus serrée en portrait. Limites actuelles, à connaître :
* Android interdit aux applications de lire les données des autres applications. Le dossier de partage est donc
  `/storage/emulated/0/SaveStudio/<Jeu>/` : copiez-y la sauvegarde (avec le gestionnaire de fichiers), éditez-la dans
  SaveStudio, puis recopiez-la à sa place. Sous-dossiers : `StardewValley/<Ferme_ID>/<Ferme_ID>`, `Celeste/`,
  `HollowKnight/`, `GeometryDash/`, `Minecraft/<monde>/level.dat`.
* À activer une fois à la main : Réglages → Applications → Accès spécial → *Accès à tous les fichiers* → SaveStudio (le libellé varie selon la marque).
* Pas de clavier virtuel pour l'instant (limite de NativeActivity) : les flèches ▲▼, boutons prédéfinis, interrupteurs
  et listes fonctionnent, la saisie de texte non.
* Cette version Android n'a pas pu être exécutée sur un appareil lors de sa conception : traitez-la comme expérimentale.

## Compiler soi-même / lancer

```
cargo build --release          # binaire : target/release/savestudio
cargo test --release           # 10 tests (XML octet-exact, GD, Hollow Knight, NBT, détection, définitions)
./target/release/savestudio                      # écran d'accueil
./target/release/savestudio --open CHEMIN        # ouvre directement une sauvegarde
./target/release/savestudio --open CHEMIN --page 2   # … sur la 3e catégorie
# Android (SDK + NDK + `rustup target add aarch64-linux-android` + `cargo install cargo-apk`) :
cd android && CARGO_APK_RELEASE_KEYSTORE=... CARGO_APK_RELEASE_KEYSTORE_PASSWORD=... cargo apk build --release --lib
```

On peut aussi glisser-déposer un fichier dans la fenêtre. Linux : paquets `libxkbcommon`, `libwayland`/X11, OpenGL.

## Les fichiers de jeu `.sve`

Un jeu = **un seul fichier d'une seule ligne** (aucun saut de ligne, ~2–4 Ko). Ils sont intégrés au binaire ;
un `games/*.sve` posé à côté de l'exécutable (ou dans `./games`) remplace/ajoute un jeu sans recompiler.

Enregistrements séparés par `;`, champs par `|`, `\` échappe `; | \`.

```
@id|Nom|Court|police|codec|thème|l1=..|l2=..|win:motif|lin:motif|mac:motif     jeu
#Catégorie|icône                                                                onglet
+Sous-catégorie|icône|c=N                                                       carte (N colonnes)
type|Libellé|chemin|options                                                     champ
```

* **police** : `pixelify`, `vt323`, `trajan`, `celeste`, `blocks` — **codec** : `xml`, `gd`, `hk`, `nbt`
  — **thème** : `wood`, `celeste`, `hollow`, `blocks`, `stone`, `home`
* `l1`/`l2` : titre et sous-titre de la page, avec `{chemin}` pour insérer une valeur du fichier.
* Motifs de détection `win:` / `lin:` / `mac:` / `and:` (Android) : `~` (dossier perso), `%VAR%`, `*`, `{d}` (nom du dossier parent).
* **types** : `n` nombre · `t` texte · `b` booléen · `e` liste de choix · `s` curseur · `l` liste · `g` grille de cases · `i` information (lecture seule)
* **options** : `min` `max` `st`(pas) `dp`(décimales) `u`(unité) `sc`(échelle) `off`(décalage) `btn=Libellé:valeur`
  `bh`(en-tête du bouton) `o=Libellé:valeur,…` `r=a-b` `names=` `xp=`/`xt=` `ro` `also=chemin:valeur`
  (`=` recopie la valeur). `min`/`max`/`btn` sont en unités **affichées**, `also` en valeur brute.
* **chemins** : `a/b/c`, `a/b[2]`, `a/b[*]` (tous), `a?b` (alternatives), `~clé` (plist : valeur qui suit la clé),
  `@scale` / `@backup` (réglages de l'application).

## Sécurité des données

* Chaque écriture commence par une copie `savestudio-backups/<nom>.<horodatage>.bak` à côté de la sauvegarde
  (désactivable dans Settings) ; le bouton **Backups** restaure la dernière.
* Écriture atomique (fichier temporaire + renommage). XML (Stardew, Celeste, GD) : seuls les textes modifiés changent,
  le reste du fichier reste identique octet pour octet.
* À utiliser sur des sauvegardes **solo / hors-ligne** uniquement.

## Limites connues

* Minecraft : Java Edition (`level.dat`) ; Bedrock non géré. Stardew : `SaveGameInfo` n'est pas modifié (la liste de
  chargement du jeu peut afficher les anciennes valeurs jusqu'à la prochaine sauvegarde en jeu).
* Geometry Dash : chemins Windows/macOS/Proton. Les chiffres de CELESTE.otf étant décoratifs, ils sont rendus en Pixelify Sans.
* Interface en anglais.

## Licences des polices — à vérifier avant de rendre le dépôt public

`assets/fonts/` contient des sous-ensembles des polices fournies. Pixelify Sans et VT323 sont sous licence libre (SIL OFL).
**Trajan Pro** (Hollow Knight) est une police commerciale d'Adobe et **CELESTE.otf** a une licence inconnue : les publier dans un
dépôt public ou dans des binaires distribués peut ne pas être permis. Gardez le dépôt privé, ou remplacez ces fichiers
(par exemple par *Cinzel*, libre et proche de Trajan) puis relancez `tools/build_fonts.py`.

## Régénérer les ressources

* `tools/build_fonts.py <dossier_polices> <feuille_de_glyphes.png>` : sous-ensembles Latin-1 de vos polices (+ police « blocs »).
* `tools/build_icons.py` : icônes pixel-art 12×12 → `assets/icons.txt`.
