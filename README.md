# Task Backup and Restore — tsbak

Sauvegarde et restauration des **tâches planifiées Windows** (Task Scheduler) en **XML brut**,
avec interface graphique **et** ligne de commande partageant le **même moteur**, la même
vérification d'intégrité et les mêmes journaux.

- `TaskBackupRestore.exe` — interface graphique (Tauri 2), Windows 10/11 et Server 2016+.
- `tsbak.exe` — ligne de commande, **Windows Server 2008 R2 → 2025+**, aucune dépendance runtime.
- Guides et assistants inclus : voir [📖 Guide PDF](dist/Guide-tsbak.pdf), [📄 Mémo filtres (PDF)](dist/Memo-motifs.pdf), [📋 MIGRATION.md](MIGRATION.md) (2008 R2 → 2022) et `dist/*.cmd`.

---

## Fonctionnalités

- **Export** : liste des tâches planifiées, sélection par cases à cocher, filtres
  d'inclusion/exclusion (motifs simples `*`), export vers un dossier (XML + `manifest.json`).
  Les tâches système `\Microsoft\` sont **masquées par défaut** dans l'interface (une case à
  cocher permet de les afficher) — elles restent exportables si vous les affichez. En CLI,
  `tsbak list --hide-microsoft` et `tsbak export --hide-microsoft` les excluent.
- **Archive `.zip` optionnelle**, **chiffrable AES-256** (variante WinZip AES, lisible par
  7-Zip/WinZip). Les fichiers d'export sont d'abord écrits dans un dossier temporaire puis
  compressés : le dossier choisi ne reçoit **que** le fichier `.zip`, jamais les XML ni le
  `manifest.json` à côté de l'archive (le dossier temporaire est supprimé, succès ou échec).
  L'archive ne contient **que** les XML de tâches et le `manifest.json` : tout autre fichier
  présent dans le dossier d'export (ancien `.zip`, journal, notes…) est **ignoré**, à la
  compression comme à l'extraction.
- **Import** : dossier d'export **ou** archive `.zip` (détectée, extraite et validée
  automatiquement — protection zip-slip incluse), plan d'import résolu tâche par tâche
  (créer / mettre à jour / ignorer / conflit / utilisateur non mappé / mot de passe requis),
  **simulation (dry-run)** puis import réel.
- **Élévation sans redémarrage** : l'import réel s'exécute dans un **processus enfant élevé**
  temporaire (invite UAC), l'interface reste ouverte et affiche le rapport à la fin.
- **Journalisation** : logs horodatés dans `%LOCALAPPDATA%\tsbak\logs\` (rétention 14 jours),
  consultables en direct dans l'onglet Logs. L'interface et le CLI écrivent dans le **même** dossier.
- **Intégrité vérifiée** : manifeste JSON + empreintes SHA-256 de chaque XML, vérifiées avant
  toute écriture — une archive modifiée ou corrompue est refusée.

## Compatibilité Windows

| Composant | Server 2008 R2 | Server 2012 | Server 2012 R2 | Server 2016 | Server 2019 | Server 2022 | Server 2025 | Windows 10 / 11 |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| `tsbak.exe` (CLI, même moteur) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `TaskBackupRestore.exe` (interface) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |

**Pourquoi l'interface est-elle limitée ?** Elle repose sur WebView2 (Tauri 2). Microsoft a
cessé toutes les mises à jour de WebView2 sur Windows 7 / Server 2008 R2 / 2012 (figé en
version 109, non patchée depuis octobre 2023) — inacceptable pour un outil exécuté en
administrateur sur des serveurs de production. Le **CLI** n'a aucune dépendance runtime
(COM Task Scheduler présent depuis Vista) et couvre donc **toutes** les versions de 2008 R2 à
2025+, avec le même moteur, la même vérification d'intégrité et les mêmes journaux. Des
assistants `.cmd` (`export.cmd`, `import.cmd`, `validate.cmd`) fournissent une expérience
guidée sur les anciens serveurs.

## 📖 Guide d'utilisation (PDF)

Le guide illustré complet est inclus : **[`dist/Guide-tsbak.pdf`](dist/Guide-tsbak.pdf)**

- export / import avec l'interface graphique, captures annotées pas à pas ;
- export / import en ligne de commande ;
- le **mot de passe unique** d'import (section 4.1) : un seul secret appliqué à toutes les tâches
  « Mot de passe requis » ;
- les assistants `.cmd` pour les serveurs 2008 R2 / 2012 ;
- options avancées (`--folder`, `--user-map`, `--password-file`, import non interactif) ;
- sécurité, bonnes pratiques et dépannage.

Ses sources sont dans [`dist/guide/`](dist/guide/) (HTML + captures + scripts de capture). Un **mémo d'une page** sur les
filtres d'inclusion/exclusion est aussi disponible : [`dist/Memo-motifs.pdf`](dist/Memo-motifs.pdf) (source : `dist/guide/memo-motifs.html`).

Les PDF sont **régénérés automatiquement** dès qu'une source de `dist/guide/` change
(workflow GitHub Actions [`guide-pdf.yml`](.github/workflows/guide-pdf.yml)) : sur la branche par défaut
le PDF est reconstruit et commité, et une *pull request* qui modifie les sources sans régénérer
les PDF échoue au contrôle. En local, `tools/guide-pdf.ps1` fait la même chose :

```bash
# Contrôler la fraîcheur des PDF livrés (code 4 si périmés avec -ErrorOnStale)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Check

# Régénérer les PDF puis enregistrer l'empreinte de leurs sources
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Build

# Déclarer les sources actuelles couvertes SANS régénérer (PDF retouché à la main)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Update
```

Le générateur seul reste utilisable pour un PDF isolé (Edge headless, aucune installation) :

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File dist/guide/scripts/make-pdf.ps1 `
    -Html dist/guide/guide.html -Pdf dist/Guide-tsbak.pdf
```

## Démarrage rapide

### Interface graphique (Windows 10/11, Server 2016+)

`dist\TaskBackupRestore.exe` — **exe portable unique**, aucune installation
(WebView2 préinstallé sur ces systèmes).

1. Onglet *Tâches & Export* → **Charger les tâches** → sélectionner (les tâches
   `\Microsoft\` sont masquées par défaut) → *Exporter…* — éventuellement en `.zip`
   chiffré AES-256.
2. Copier le dossier ou le `.zip` sur la machine cible.
3. Onglet *Import* → chemin du dossier ou du `.zip` (détection automatique) →
   *Vérifier l'archive* → *Préparer le plan* → résoudre les lignes signalées →
   *Simuler (dry-run)* → *Importer* (invite UAC si nécessaire, l'interface reste ouverte).

### Ligne de commande (Server 2008 R2 → 2025+)

```bat
:: 1. Exporter toutes les tâches vers un dossier
tsbak.exe export C:\tsbak\export-2026-01-01

::    ... ou sans les tâches système \Microsoft\
tsbak.exe export C:\tsbak\export-2026-01-01 --hide-microsoft

:: 2. Vérifier l'intégrité (manifeste + empreintes SHA-256)
tsbak.exe validate C:\tsbak\export-2026-01-01

:: 3. Simuler (rien n'est écrit)
tsbak.exe import C:\tsbak\export-2026-01-01 --dry-run

:: 4. Importer réellement (en administrateur)
tsbak.exe import C:\tsbak\export-2026-01-01
::    Options utiles :
::    --folder \Restauration-2026        restaure sous ce dossier, structure préservée
::    --password-file mdp.txt            mots de passe (DOMAINE\user=motdepasse, 1/ligne)
::    --user-map OLDPC\user:NEWPC\user   remplace un compte source par un compte cible
::    --skip-password-tasks              ignore (sans bloquer) les tâches sans mot de passe
::    --answer-file reponses.json        import 100 % non interactif
```

## Compilation

```bash
# CLI (produit dist/tsbak.exe équivalent)
cd tsbak
cargo build --release
cargo test

# Interface graphique (produit l'exe, sans bundle)
cd ../src-tauri
cargo build --release
cargo test
```

Le crate `tsbak` compile sur toute plateforme (l'accès COM réel au Task Scheduler est
isolé derrière `cfg(windows)`) ; seul `list`/`export`/`import` nécessitent une machine
Windows, `validate` fonctionne partout.

### Thème de l'interface

L'interface est statique (`ui/`) : le thème actif est donc **embarqué dans l'exe au moment
de la compilation**. `ui/style.css` ne contient que la structure et les composants ; les
palettes vivent dans `ui/themes/` et `ui/index.html` n'en active qu'une :

| Feuillet | Rôle |
|---|---|
| [`ui/themes/legacy.css`](ui/themes/legacy.css) | **thème publié** — palette historique, active par défaut |
| [`ui/themes/hestia.css`](ui/themes/hestia.css) | variante locale (bleu marine / orange), non publiée |

```powershell
# Variante locale dans l'interface (puis recompiler pour l'embarquer)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Set -Theme hestia

# Thème actif
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Status

# Retour au thème publié avant toute compilation ou publication
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Set -Theme legacy
```

Le livrable publié garde toujours la palette historique : `tools\make-release.ps1` vérifie
le thème actif (`-Action Check -Require legacy`) et **s'arrête** si la variante est active. Une nouvelle palette = un feuillet dans `ui/themes/`, une ligne de plus dans
`ui/index.html` et son nom dans `tools\select-theme.ps1`. Le choix du thème ne touche jamais
`ui/style.css`, commun aux deux.

## Publication (release GitHub)

Les artefacts distribués sont assemblés depuis `dist/` par un script unique :

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\make-release.ps1
```

Il lit la version dans `src-tauri/tauri.conf.json`, reconstruit
`release\tsbak-<version>-windows\` (exe + assistants `.cmd` + `Guide-tsbak.pdf` +
`Memo-motifs.pdf` + sources du guide) en écartant les fichiers de travail des captures et les
**données d'export réelles** (recréées par `guide\demo\*.cmd`), puis produit
`release\tsbak-<version>-windows.zip` — archive à entrées `/`, lisible hors Windows — et
affiche les empreintes SHA-256. Le dossier `release/` n'est pas versionné : il est joint à la
release GitHub.

## Test réel de bout en bout

Un test `#[ignore]` exécute le flux complet contre le **vrai** scheduler Windows (COM) :
export de tâches réelles vers un `.zip` chiffré AES-256, refus du mauvais mot de passe,
extraction + validation, plan, simulation et import réel (tâches identiques → rien n'est
écrit), puis vérification du rapport et des logs (secret absent).

```bash
cd src-tauri
cargo test e2e_export_zip_aes_then_reimport -- --ignored --nocapture
```

## Sécurité

- Les **mots de passe** (comptes de tâches, archive `.zip`) ne sont **jamais journalisés**,
  jamais écrits sur disque, jamais renvoyés à l'interface : ils restent en mémoire dans le
  backend, effaçables via « Effacer les mots de passe ».
- Transfert vers le processus élevé via un fichier de réponses JSON temporaire
  (`%TEMP%\task-backup-restore`, ACL utilisateur uniquement), **supprimé dans tous les cas**.
- Les XML exportés ne contiennent aucun secret : Windows retire automatiquement les mots de
  passe des tâches `TASK_LOGON_PASSWORD`.
- L'import vérifie manifeste + empreintes + XML bien formés **avant** toute écriture, refuse
  les chemins non sûrs (zip-slip) et ignore les entrées non export (non-XML / non-manifeste).

## Structure du dépôt

| Chemin | Rôle |
|---|---|
| `tsbak/` | Crate Rust du moteur + CLI `tsbak.exe` (`scheduler/`, `export.rs`, `import.rs`, `password.rs`, `answers.rs`, `wizard.rs`) |
| `src-tauri/` | Application Tauri 2 : commandes (`commands.rs`), archives ZIP/AES (`archive.rs`), helper d'élévation (`helper.rs`), logs (`app_log.rs`) |
| `ui/` | Frontend statique HTML/CSS/JS (sans framework) ; palettes dans `ui/themes/` (`legacy` = publié, `hestia` = variante locale) |
| `dist/` | Livraison : `TaskBackupRestore.exe`, `tsbak.exe`, assistants `.cmd`, `Guide-tsbak.pdf` + sources du guide |
| `tools/` | Scripts de maintenance : générateur d'icône (`make_icon.py`), assemblage des artefacts de release (`make-release.ps1`), thème de l'interface (`select-theme.ps1`), fraîcheur et régénération du guide PDF (`guide-pdf.ps1`) |
| `.github/workflows/` | Intégration continue : `guide-pdf.yml` (guide PDF régénéré et contrôlé) |

## Licence

MIT — © Endymi0n74. Voir [`CHANGELOG.md`](CHANGELOG.md) pour l'historique des versions.