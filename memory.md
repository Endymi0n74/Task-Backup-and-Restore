# memory.md — Task Backup and Restore (tsbak-gui)

Mémoire du projet : historique, décisions structurantes, leçons apprises. À lire
en complément de [`README.md`](README.md) et [`AGENTS.md`](AGENTS.md).

## En bref

- **Quoi** : export/import des tâches planifiées Windows (Task Scheduler) en XML brut.
- **Chemin** : `D:\Codex\tsbak-gui` (dépôt Git autonome, branche `master`) — publié sur
  GitHub sous **Task Backup and Restore** (`Endymi0n74/Task-Backup-and-Restore`).
- **Deux interfaces, un moteur** : `tsbak` (crate Rust + CLI) et `TaskBackupRestore.exe`
  (Tauri 2). Livraison portable dans `dist/`.
- **Dernière version** : **1.0.0** (2026-09-12) — première version publiée ; les
  numérotations intermédiaires (1.1.0 du 2026-09-10) ont été **renumérotées en 1.0.0** lors de
  la création du dépôt (les entrées de dates ci-dessous gardent leur numérotation d'origine).

## Dates clés

- **2026-09-08** — Premier test réel de l'implémentation COM sur machine Windows :
  trois bugs réels trouvés et corrigés (voir « Leçons apprises »).
- **2026-09-09** — Migration Tauri 1 → Tauri 2 terminée (dépôt consolidé) ; guide
  illustré PDF produit (captures réelles de l'interface, de la console et des
  assistants `.cmd`).
- **2026-09-09/10** — v1.0.0 : archives `.zip` restreintes (XML + manifeste uniquement),
  tâches Microsoft masquées par défaut, nettoyage de code, version unifiée.
- **2026-09-10** — v1.1.0 : option CLI `--hide-microsoft` (list/export) ; `MIGRATION.md`
  (procédure 2008 R2 → 2022) ; section dédiée « filtres Inclure/Exclure » dans le guide
  (section 3) ; mémo d'une page `dist/Memo-motifs.pdf` ; script `make-pdf.ps1`
  (Edge headless) régénère les PDF depuis les sources HTML modifiables.
- **2026-09-11** — Guide : nouvelle sous-section **4.1 « Appliquer un mot de passe unique à
  toutes les tâches »** (champ « Mot de passe unique » + bouton « Remplir avec le même mot de
  passe », étape 5 de la section 4, note CLI `--password-file`, ligne de dépannage),
  vignette annotée `dist/guide/shots/10-import-password.png`, figures renumérotées 1→10.
  La vignette a été capturée sur un **build release local**
  (`src-tauri/target/release/task-backup-restore.exe`) ; les binaires de `dist/` ont ensuite
  été reconstruits.
- **2026-09-11 (suite)** — **`dist/TaskBackupRestore.exe` reconstruit** depuis l'arbre courant
  (nouveau SHA-256 `f03dda61…`) : l'interface embarque bien la sous-section 4.1 (champ
  « Mot de passe unique » + bouton « Remplir avec le même mot de passe »), vérifié dans le
  cache d'assets brotli du build. `dist/tsbak.exe` **inchangé** — identique au binaire
  recompilé, déjà `1.1.0` avec `--hide-microsoft` (la fonctionnalité est purement côté
  interface). Restent à rafraîchir avant publication : `release/tsbak-1.1.0-windows/` et son
  `.zip`, qui datent du 2026-09-10 et contiennent encore l'ancienne interface.
- **2026-09-12** — **Artefacts de release reconstruits** depuis `dist/` par un script
  désormais versionné, `tools/make-release.ps1` (copie sélective, exclusion des fichiers de
  travail et des données d'export de la machine, puis archive `.zip` à entrées `/`).
  `Memo-motifs.pdf` est **ajouté** à la livraison (il manquait alors que le `guide/` embarqué
  contient déjà `memo-motifs.html`) et `LISEZMOI.txt` le mentionne.
- **2026-09-12 (publication)** — **Rebranding en 1.0.0** : version unifiée `1.0.0`
  (`tsbak/Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`), produit
  renommé **Task Backup and Restore**, **les deux binaires recompilés** (`tsbak.exe`
  `6730a7ad…` — `tsbak --version` répond bien `1.0.0` ; `TaskBackupRestore.exe` `ea9b0750…`,
  FileVersion `1.0.0`, chaîne Rust `import_set_password` présente dans le binaire), CHANGELOG
  consolidé en une seule entrée `1.0.0 — 2026-09-12`, mentions `v1.1.0` neutralisées dans
  `MIGRATION.md` et les sources du guide. Livraison `release/tsbak-1.0.0-windows/`
  (45 fichiers) + `tsbak-1.0.0-windows.zip` SHA-256 `9e8213aa…` (extraction vérifiée :
  empreintes identiques à `dist/`). Publication : dépôt GitHub créé, `v1.0.0` poussé et
  release avec l'archive — voir « État actuel ».
- **2026-09-12 (CI)** — **Guide PDF automatisé** : `tools/guide-pdf.ps1`
  (`-Action Check` | `Build` | `Update`) enregistre l'empreinte des sources de chaque PDF dans
  `dist/guide/pdf-sources.sha256` (guide : `guide.html` + `shots/` + `make-pdf.ps1` ; mémo :
  `memo-motifs.html` + `shots/` + `make-pdf.ps1`) et le workflow
  `.github/workflows/guide-pdf.yml` régénère + commite les PDF quand une source change
  (push sur `master`, `[skip ci]`) mais **échoue** en pull request si le PDF commité est
  périmé. Les sources sont relues par `git ls-files` (les fichiers non suivis sont signalés)
  et les fins de ligne sont normalisées en LF avant hachage (`core.autocrlf=true` ici, aucun
  `.gitattributes`) : mêmes empreintes en local et sur le runner. Vérifié le 2026-09-12 : les
  deux PDF commités sont **identiques en contenu** à un build neuf (guide : 12 octets de
  métadonnées seulement ; mémo : flux de contenu identiques, seuls version Edge 153→154 et
  horodatage changent), donc aucune retouche manuelle n'a été écrasée.

## Décisions structurantes

1. **Le CLI est le moteur** : `tsbak.exe` fonctionne sur **toutes** les versions de
   Server 2008 R2 → 2025+ (aucune dépendance runtime, COM présent depuis Vista).
   L'interface (WebView2) est limitée à Win10/11 et Server 2016+ — Microsoft ne
   patchant plus WebView2 sur 2008 R2/2012 (figé en 109 depuis oct. 2023), c'est un
   choix de sécurité assumé, documenté dans le README et le guide.
2. **Classification unique** : `build_plan` calcule une seule fois la décision
   (créer/mettre à jour/sauter/conflit/mot de passe requis/utilisateur non mappé) ;
   `execute_plan(dry_run)` ne fait que sauter l'écriture. Dry-run et réel donnent donc
   un rapport strictement identique.
3. **Secrets jamais journalisés** : mots de passe en mémoire dans l'état backend,
   transmis au processus élevé via un fichier JSON temporaire (ACL utilisateur,
   supprimé dans tous les chemins). Aucun log, aucune vue ne transporte un secret.
4. **Élévation à la volée** : l'import réel non élevé lance le même exe avec
   `--helper-import` via `ShellExecuteExW runas` (UAC) — l'interface reste ouverte.
5. **ZIP = contenu d'export strict** : seuls les XML de tâches + `manifest.json` entrent
   dans une archive (et seuls ces fichiers en sortent à l'extraction). Les fichiers
   étrangers sont ignorés — cf. `is_export_entry` dans `src-tauri/src/archive.rs`.
6. **Interface : tâches Microsoft masquées par défaut** (`\Microsoft\*`), case à cocher
   « Masquer les tâches Microsoft » dans `ui/app.js`. Décision prise le 2026-09-09/10
   pour réduire le bruit système (des centaines de tâches Windows dans la liste).
7. **Résolution des conflits** : politique `--conflict-policy`/fichier de réponses ;
   pas de mapping utilisateur pour les comptes bien connus (SYSTEM, LOCAL SERVICE…).
   Les tâches `TASK_LOGON_PASSWORD` sont validées par Windows lui-même au
   `RegisterTask` (HRESULT traduit) ; le mapping explicite n'est requis que pour S4U /
   jetons interactifs.
8. **Guide PDF : contrôle par empreinte des sources, jamais par comparaison binaire.** Edge
   headless n'est pas reproductible (version, horodatage, `/ID`) et le PDF peut être retouché à
   la main : comparer les octets produirait un échec permanent. `tools/guide-pdf.ps1` enregistre
   donc l'empreinte des sources de chaque PDF (`dist/guide/pdf-sources.sha256`) ; le workflow
   régénère et commite les PDF sur la branche par défaut mais échoue en pull request si le PDF
   commité est périmé. `-Action Update` déclare les sources couvertes **sans** régénérer, pour
   un PDF livré retouché à la main.

## Leçons apprises

- **COM `windows-rs` 0.58** : `IPrincipal::UserId` et `IPrincipal::LogonType` utilisent
  des paramètres de sortie (`*mut BSTR` / `*mut TASK_LOGON_TYPE`), pas des retours directs.
- **`CoUninitialize` avant libération COM** → STATUS_ACCESS_VIOLATION à la fermeture :
  `WindowsScheduler` libère `ITaskService` dans `Drop` **avant** `CoUninitialize`
  (champ `Option` + `take()`).
- **Collections COM 1-based** : `get_Item`/`Count` démarrent à 1.
- **`CreateFolder`** échoue si le dossier existe : cas ignoré dans `ensure_folder`.
- **WebView2 figé sur anciens serveurs** : ne pas promettre l'interface au-delà de
  Server 2016 — pointer vers le CLI/assistants `.cmd`.
- **`make-pdf.ps1`** : `-Pdf` doit être un chemin **relatif** (le script fait
  `Join-Path (Get-Location) $Pdf` puis `GetFullPath`) — un chemin absolu échoue avec
  « Le format du chemin d'accès donné n'est pas pris en charge ».
- **`tools/make-release.ps1`** : sous Git Bash, `powershell -NoProfile -File <script>`
  peut rester bloqué (aucune instruction exécutée, le script n'écrit rien) alors que le même
  script lancé en `-Command "& '.\tools\make-release.ps1'"` s'exécute en ~1 s. Lancer les
  scripts `.ps1` avec la forme `-Command` dans cet environnement ; le script lui-même est
  correct (vérifié, sortie identique d'une exécution à l'autre).
- **Captures d'écran du guide** : les scripts de capture (`dist/guide/scripts/*.ps1`)
  existent et ont servi à produire les vignettes ; la capture des assistants `.cmd`
  interactifs est fragile (positionnement fenêtre console, timing). L'utilisateur
  retouche ces vignettes à la main.

## État actuel / à faire

- [x] Export/import XML + manifeste SHA-256, filtres, dry-run, ZIP AES-256.
- [x] Élévation à la volée (helper), logs partagés CLI/GUI.
- [x] ZIP restreint (XML + manifeste uniquement), extraction filtrée.
- [x] Tâches Microsoft masquées par défaut.
- [x] v1.0.0 : version unifiée, CHANGELOG, README GitHub.
- [ ] Guide PDF : vignettes 8-11 à refaire à la main par l'utilisateur (ne pas toucher).
      Attention à la renumérotation du 2026-09-11 : les figures ≥ 4 sont décalées de +1
      (ancienne figure 8 = figure 9, etc.).
- [x] Guide PDF : régénération automatisée (2026-09-12) — `tools/guide-pdf.ps1`
      (`Check`/`Build`/`Update`) + `.github/workflows/guide-pdf.yml` (push → régénère et
      commite, pull request → échoue si le PDF commité est périmé). Après une retouche manuelle
      du PDF, relancer `-Action Update` pour réaligner l'empreinte des sources.
- [x] Artefacts de release reconstruits (`tools/make-release.ps1`, 2026-09-12).
- [x] Rebranding **1.0.0** + dépôt GitHub **Task Backup and Restore** (`v1.0.0`, release avec
      `tsbak-1.0.0-windows.zip`).
- [x] Guide PDF : le PDF livré a été régénéré (commit « regenerer le guide PDF ») puis contrôlé
      le 2026-09-12 — contenu identique à un build neuf, empreintes de référence enregistrées
      dans `dist/guide/pdf-sources.sha256`.