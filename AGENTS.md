# AGENTS.md — conventions pour travailler sur tsbak-gui

Guide pour les agents (et humains) qui modifient ce dépôt. Lire aussi
[`README.md`](README.md) et [`memory.md`](memory.md).

## Vue d'ensemble

- **`tsbak/`** — crate Rust du moteur + CLI `tsbak.exe`. Toute la logique métier
  (export, import, validation, classification, mots de passe) vit ici et **doit** y rester.
- **`src-tauri/`** — application Tauri 2 (backend Rust) : commandes, archives ZIP/AES,
  helper d'élévation, journalisation.
- **`ui/`** — frontend statique HTML/CSS/JS (pas de framework, pas de build JS).
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
- **Tests** : toute nouvelle logique métier dans `tsbak` doit avoir des tests unitaires
  basés sur `MockScheduler`. Les tests d'archive (roundtrip, AES, zip-slip, filtrage)
  vivent dans `src-tauri/src/archive.rs`.
- **Version** : version unique `1.0.0` dans `tsbak/Cargo.toml`, `src-tauri/Cargo.toml`
  et `src-tauri/tauri.conf.json` — les maintenir synchronisées (le CLI affiche
  `env!("CARGO_PKG_VERSION")`).
- **Le guide PDF (`dist/Guide-tsbak.pdf`) est généré à partir de
  `dist/guide/guide.html`** (sources + captures dans `dist/guide/`). Ne pas régénérer le
  PDF sans raison : l'utilisateur le retouche manuellement.

## Flux d'import (à ne pas casser)

1. `load_and_verify` (manifeste + empreintes SHA-256 + XML bien formé).
2. `build_plan` — classification unique (create/update/skip/conflit/password/unmapped).
3. `execute_plan` — écriture **ou** dry-run, rapport garanti identique.
4. Élévation : import réel non élevé → processus enfant `--helper-import` (UAC).

## Contexte projet

Voir [`memory.md`](memory.md) pour l'historique des décisions, les dates clés et les
leçons apprises (bugs COM, pièges WebView2, etc.).