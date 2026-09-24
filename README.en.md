# Task Backup and Restore — tsbak

[🇫🇷 Français](README.md) · **🇬🇧 English**

Backup and restore of **Windows scheduled tasks** (Task Scheduler) as **raw XML**,
with a graphical interface **and** a command line sharing the **same engine**, the same
integrity check and the same logs.

- `TaskBackupRestore.exe` — graphical interface (Tauri 2), Windows 10/11 and Server 2016+.
- `tsbak.exe` — command line, **Windows Server 2008 R2 → 2025+**, no runtime dependency.
- Guides and helpers included: see [📖 PDF Guide](dist/Guide-tsbak.pdf), [📄 Filter cheat sheet (PDF)](dist/Memo-motifs.pdf), [📋 MIGRATION.md](MIGRATION.md) (2008 R2 → 2022) and `dist/*.cmd`.

---

## Features

- **Export**: list of scheduled tasks, checkbox selection, inclusion/exclusion
  filters (simple `*` patterns), export to a folder (XML + `manifest.json`).
  The `\Microsoft\` system tasks are **hidden by default** in the interface (a checkbox
  allows showing them) — they remain exportable if you show them. In the CLI,
  `tsbak list --hide-microsoft` and `tsbak export --hide-microsoft` exclude them.
- **Optional `.zip` archive**, **encryptable with AES-256** (WinZip AES variant, readable by
  7-Zip/WinZip). The export files are first written to a temporary folder then
  compressed: the chosen folder receives **only** the `.zip` file, never the XMLs nor the
  `manifest.json` next to the archive (the temporary folder is deleted, success or failure).
  The archive contains **only** the task XMLs and the `manifest.json`: any other file
  present in the export folder (old `.zip`, log, notes…) is **ignored**, both when
  compressing and when extracting.
- **Import**: export folder **or** `.zip` archive (detected, extracted and validated
  automatically — zip-slip protection included), import plan resolved task by task
  (create / update / skip / conflict / unmapped user / password required),
  **simulation (dry-run)** then the real import.
- **Elevation without restart**: the real import runs in a temporary **elevated child
  process** (UAC prompt), the interface stays open and shows the report at the end.
- **Logging**: timestamped logs in `%LOCALAPPDATA%\tsbak\logs\` (14-day retention),
  viewable live in the Logs tab. The interface and the CLI write to the **same** folder.
- **Verified integrity**: JSON manifest + SHA-256 fingerprints of each XML, checked before
  any write — a modified or corrupted archive is rejected.

## Windows compatibility

| Component | Server 2008 R2 | Server 2012 | Server 2012 R2 | Server 2016 | Server 2019 | Server 2022 | Server 2025 | Windows 10 / 11 |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| `tsbak.exe` (CLI, same engine) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `TaskBackupRestore.exe` (interface) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |

**Why is the interface limited?** It relies on WebView2 (Tauri 2). Microsoft has
ended all WebView2 updates on Windows 7 / Server 2008 R2 / 2012 (frozen at version
109, unpatched since October 2023) — unacceptable for a tool run as an
administrator on production servers. The **CLI** has no runtime dependency
(COM Task Scheduler present since Vista) and therefore covers **all** versions from 2008 R2 to
2025+, with the same engine, the same integrity check and the same logs. `.cmd`
helpers (`export.cmd`, `import.cmd`, `validate.cmd`) provide a guided
experience on older servers.

## 📖 User guide (PDF)

The complete illustrated guide is included: **[`dist/Guide-tsbak.pdf`](dist/Guide-tsbak.pdf)**

- export / import with the graphical interface, annotated step-by-step screenshots ;
- export / import from the command line ;
- the import **single password** (section 4.1): one secret applied to all tasks
  "Password required" ;
- the `.cmd` helpers for 2008 R2 / 2012 servers ;
- advanced options (`--folder`, `--user-map`, `--password-file`, non-interactive import) ;
- security, best practices and troubleshooting.

Its sources are in [`dist/guide/`](dist/guide/) (HTML + screenshots + capture scripts). A **one-page cheat sheet** on the
inclusion/exclusion filters is also available: [`dist/Memo-motifs.pdf`](dist/Memo-motifs.pdf) (source: `dist/guide/memo-motifs.html`).

The PDFs are **regenerated automatically** as soon as a source in `dist/guide/` changes
(GitHub Actions workflow [`guide-pdf.yml`](.github/workflows/guide-pdf.yml)): on the default branch
the PDF is rebuilt and committed, and a *pull request* that modifies the sources without
regenerating the PDFs fails the check. Locally, `tools/guide-pdf.ps1` does the same:

```bash
# Contrôler la fraîcheur des PDF livrés (code 4 si périmés avec -ErrorOnStale)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Check

# Régénérer les PDF puis enregistrer l'empreinte de leurs sources
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Build

# Déclarer les sources actuelles couvertes SANS régénérer (PDF retouché à la main)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Update
```

The generator alone remains usable for a standalone PDF (Edge headless, no installation) :

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File dist/guide/scripts/make-pdf.ps1 `
    -Html dist/guide/guide.html -Pdf dist/Guide-tsbak.pdf
```

## Quick start

### Graphical interface (Windows 10/11, Server 2016+)

`dist\TaskBackupRestore.exe` — **single portable exe**, no installation
(WebView2 preinstalled on these systems).

1. *Tasks & Export* tab → **Load tasks** → select (the `\Microsoft\` tasks are
   hidden by default) → *Export…* — optionally as an AES-256 encrypted `.zip`.
2. Copy the folder or the `.zip` to the target machine.
3. *Import* tab → path to the folder or the `.zip` (automatic detection) →
   *Check the archive* → *Prepare the plan* → resolve the flagged rows →
   *Simulate (dry-run)* → *Import* (UAC prompt if necessary, the interface stays open).

### Command line (Server 2008 R2 → 2025+)

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

## Build

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

The `tsbak` crate builds on any platform (real COM access to the Task Scheduler is
isolated behind `cfg(windows)`); only `list`/`export`/`import` require a
Windows machine, `validate` works everywhere.

### Interface theme

The interface is static (`ui/`): the active theme is therefore **embedded in the exe at
build time**. `ui/style.css` contains only the structure and the components; the
palettes live in `ui/themes/` and `ui/index.html` activates only one of them:

| Stylesheet | Role |
|---|---|
| [`ui/themes/legacy.css`](ui/themes/legacy.css) | **published theme** — historical palette, active by default |
| [`ui/themes/hestia.css`](ui/themes/hestia.css) | local variant (navy blue / orange), not published |

```powershell
# Variante locale dans l'interface (puis recompiler pour l'embarquer)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Set -Theme hestia

# Thème actif
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Status

# Retour au thème publié avant toute compilation ou publication
powershell -NoProfile -ExecutionPolicy Bypass -File tools\select-theme.ps1 -Action Set -Theme legacy
```

The published artifact always keeps the historical palette: `tools\make-release.ps1` checks
the active theme (`-Action Check -Require legacy`) and **aborts** if the variant is active. A new palette = a stylesheet in `ui/themes/`, one more line in
`ui/index.html` and its name in `tools\select-theme.ps1`. Choosing a theme never touches
`ui/style.css`, shared by both.

## Publishing (GitHub release)

The distributed artifacts are assembled from `dist/` by a single script:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\make-release.ps1
```

It reads the version from `src-tauri/tauri.conf.json`, rebuilds
`release\tsbak-<version>-windows\` (exe + `.cmd` helpers + `Guide-tsbak.pdf` +
`Memo-motifs.pdf` + guide sources) while excluding the screenshot working files and the
**real export data** (recreated by `guide\demo\*.cmd`), then produces
`release\tsbak-<version>-windows.zip` — archive with `/` entries, readable outside
Windows — and displays the SHA-256 fingerprints. The `release/` folder is not versioned:
it is attached to the GitHub release.

## Real end-to-end test

A `#[ignore]` test runs the complete flow against the **real** Windows scheduler (COM):
export of real tasks to an AES-256 encrypted `.zip`, refusal of the wrong password,
extraction + validation, plan, simulation and real import (identical tasks → nothing is
written), then verification of the report and the logs (secret absent).

```bash
cd src-tauri
cargo test e2e_export_zip_aes_then_reimport -- --ignored --nocapture
```

## Security

- **Passwords** (task accounts, `.zip` archive) are **never logged**,
  never written to disk, never sent back to the interface: they stay in memory in the
  backend, clearable via "Clear passwords".
- Transfer to the elevated process via a temporary JSON answers file
  (`%TEMP%\task-backup-restore`, user-only ACL), **deleted in all cases**.
- The exported XMLs contain no secrets: Windows automatically removes the
  passwords of `TASK_LOGON_PASSWORD` tasks.
- The import checks manifest + fingerprints + well-formed XMLs **before** any write, refuses
  unsafe paths (zip-slip) and ignores non-export entries (non-XML / non-manifest).

## Repository structure

| Path | Role |
|---|---|
| `tsbak/` | Rust crate of the engine + `tsbak.exe` CLI (`scheduler/`, `export.rs`, `import.rs`, `password.rs`, `answers.rs`, `wizard.rs`) |
| `src-tauri/` | Tauri 2 application: commands (`commands.rs`), ZIP/AES archives (`archive.rs`), elevation helper (`helper.rs`), logs (`app_log.rs`) |
| `ui/` | static HTML/CSS/JS frontend (no framework) ; palettes in `ui/themes/` (`legacy` = published, `hestia` = local variant) |
| `dist/` | Delivery: `TaskBackupRestore.exe`, `tsbak.exe`, `.cmd` helpers, `Guide-tsbak.pdf` + guide sources |
| `tools/` | Maintenance scripts: icon generator (`make_icon.py`), release artifact assembly (`make-release.ps1`), interface theme (`select-theme.ps1`), PDF guide freshness and regeneration (`guide-pdf.ps1`) |
| `.github/workflows/` | Continuous integration: `guide-pdf.yml` (PDF guide regenerated and checked) |

## License

MIT — © Endymi0n74. See [`CHANGELOG.md`](CHANGELOG.md) for the version history.
