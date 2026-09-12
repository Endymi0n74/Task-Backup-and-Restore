# AGENTS.md — conventions pour travailler sur tsbak-gui

Guide pour les agents (et humains) qui modifient ce dépôt. Lire aussi
[`README.md`](README.md) et [`memory.md`](memory.md).

## Vue d'ensemble

- **`tsbak/`** — crate Rust du moteur + CLI `tsbak.exe`. Toute la logique métier
  (export, import, validation, classification, mots de passe) vit ici et **doit** y rester.
- **`src-tauri/`** — application Tauri 2 (backend Rust) : commandes, archives ZIP/AES,
  helper d'élévation, journalisation.
- **`ui/`** — frontend statique HTML/CSS/JS (pas de framework, pas de build JS) ; les palettes
  vivent dans `ui/themes/` (`legacy.css` = thème publié, `hestia.css` = variante locale).
- **`dist/`** — livraison : exe portables, assistants `.cmd`, guide PDF + sources.

Règle d'or : **toute logique de décision est dans `tsbak` ; `src-tauri` et `ui/` ne font
que la brancher.** Les tests métier se font avec `MockScheduler` (sans Windows).

## Commandes

```bash
# CLI : tests + build release
cd tsbak && cargo test && cargo build --release

# Interface : tests + build release
cd src-tauri && cargo test && cargo build --release

# Test de bout en bout réel (Windows + COM, ignoré par défaut)
cd src-tauri && cargo test e2e_export_zip_aes_then_reimport -- --ignored --nocapture

# Artefacts de release (dossier + .zip dans release/, non versionné)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\make-release.ps1

# Guide PDF : fraîcheur, régénération, empreintes
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Check
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Build

# Thème de l'interface : statut, bascule, contrôle avant livraison
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Status
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Set -Theme hestia
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Check -Require legacy -ErrorOnStale
```

## Conventions

- **Langue** : code, commentaires, messages utilisateur et docs en **français**
  (encodage UTF-8, accents autorisés). L'interface et le CLI sont francophones.
- **Pas de dépendances superflues** : `tsbak` utilise clap/serde/sha2/quick-xml/walkdir,
  `src-tauri` ajoute tauri/rfd/zip + `windows-rs` (Windows uniquement, `cfg(windows)`).
  Ne pas ajouter de framework JS ni de grosse dépendance sans justification.
- **Sécurité** : les mots de passe ne sont **jamais** journalisés ni sérialisés vers
  l'interface. Toute nouvelle fonction qui manipule un secret doit suivre ce principe
  (aucun `Debug` dérivé, aucun log du secret).
- **Archives ZIP** : une archive d'export ne contient **que** les XML de tâches et le
  `manifest.json` — tout autre fichier est ignoré à la compression comme à l'extraction
  (`is_export_entry` dans `src-tauri/src/archive.rs`). Respecter cette règle dans toute
  modification.
- **Interface** : les tâches `\Microsoft\` sont **masquées par défaut** (case à cocher
  « Masquer les tâches Microsoft » dans `ui/app.js` / `ui/index.html`).
- **Thèmes** : `ui/style.css` ne définit **aucune variable de palette** et décrit l'aspect du
  thème publié (structure + composants) ; les variables (`--bg`, `--primary`, ...) sont
  fournies par le thème actif chargé par `ui/index.html` depuis `ui/themes/`
  (`legacy.css` = thème publié, `hestia.css` = variante locale qui surcharge les règles
  dont la variante change l'aspect). Bascule : `tools\select-theme.ps1 -Action Set -Theme
  hestia|legacy`. Ne jamais livrer un artefact avec une variante active : `make-release.ps1`
  contrôle `-Action Check -Require legacy` et s'arrête sinon. Ajouter une palette = un
  feuillet dans `ui/themes/` + une entrée dans `ui/index.html` et dans `select-theme.ps1`.
  Toute modification de `ui/style.css` doit rester sans effet sur le thème publié.
- **Tests** : toute nouvelle logique métier dans `tsbak` doit avoir des tests unitaires
  basés sur `MockScheduler`. Les tests d'archive (roundtrip, AES, zip-slip, filtrage)
  vivent dans `src-tauri/src/archive.rs`.
- **Version** : version unique `1.0.0` dans `tsbak/Cargo.toml`, `src-tauri/Cargo.toml`
  et `src-tauri/tauri.conf.json` — les maintenir synchronisées (le CLI affiche
  `env!("CARGO_PKG_VERSION")`).
- **Guide PDF** : `dist/Guide-tsbak.pdf` et `dist/Memo-motifs.pdf` sont générés par Edge
  headless depuis `dist/guide/guide.html` / `memo-motifs.html` + `dist/guide/shots/`
  (`dist/guide/scripts/make-pdf.ps1`). L'empreinte des sources de chaque PDF est enregistrée
  dans `dist/guide/pdf-sources.sha256` par `tools/guide-pdf.ps1` (`-Action Check` | `Build` |
  `Update`). Le workflow `.github/workflows/guide-pdf.yml` **régénère et commite** les PDF dès
  qu'une source change sur la branche par défaut, et **échoue en pull request** si le PDF
  commité est périmé. Les octets des PDF ne sont jamais comparés (Edge n'est pas reproductible
  et le PDF est retouché à la main) : c'est l'empreinte des sources qui tranche. Après une
  modification des sources, relancer `-Action Build` (ou `-Action Update` si le PDF livré a été
  retouché à la main) pour ne pas laisser le workflow signaler un PDF périmé.

## Flux d'import (à ne pas casser)

1. `load_and_verify` (manifeste + empreintes SHA-256 + XML bien formé).
2. `build_plan` — classification unique (create/update/skip/conflit/password/unmapped).
3. `execute_plan` — écriture **ou** dry-run, rapport garanti identique.
4. Élévation : import réel non élevé → processus enfant `--helper-import` (UAC).

## Contexte projet

Voir [`memory.md`](memory.md) pour l'historique des décisions, les dates clés et les
leçons apprises (bugs COM, pièges WebView2, etc.).