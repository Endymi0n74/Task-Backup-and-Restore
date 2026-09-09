# memory.md — Task backup and restore (tsbak-gui)

Mémoire du projet : historique, décisions structurantes, leçons apprises. À lire
en complément de [`README.md`](README.md) et [`AGENTS.md`](AGENTS.md).

## En bref

- **Quoi** : export/import des tâches planifiées Windows (Task Scheduler) en XML brut.
- **Chemin** : `D:\Codex\tsbak-gui` (dépôt Git autonome, branche `master`).
- **Deux interfaces, un moteur** : `tsbak` (crate Rust + CLI) et `TaskBackupRestore.exe`
  (Tauri 2). Livraison portable dans `dist/`.
- **Dernière version** : 1.0.0 (2026-09-10).

## Dates clés

- **2026-09-08** — Premier test réel de l'implémentation COM sur machine Windows :
  trois bugs réels trouvés et corrigés (voir « Leçons apprises »).
- **2026-09-09** — Migration Tauri 1 → Tauri 2 terminée (dépôt consolidé) ; guide
  illustré PDF produit (captures réelles de l'interface, de la console et des
  assistants `.cmd`).
- **2026-09-09/10** — v1.0.0 : archives `.zip` restreintes (XML + manifeste uniquement),
  tâches Microsoft masquées par défaut, nettoyage de code, version unifiée.

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
8. **Le guide PDF est retouché manuellement** par l'utilisateur : ne pas régénérer
   `dist/Guide-tsbak.pdf` sans demande explicite (les vignettes 8-11 doivent être
   refaites à la main).

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
- [ ] Publier le dépôt sur GitHub + créer la release 1.0.0 avec les artefacts `dist/`.