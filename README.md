# Task backup and restore

Interface graphique (Tauri 2) pour **exporter / réimporter les tâches planifiées Windows**
(Task Scheduler) au format XML brut, avec **journalisation complète**.

`task-backup-restore` est un wrapper de la crate [`tsbak`](tsbak) (CLI conservé, inchangé) : toute
la logique métier — export XML, vérification des empreintes SHA-256, classification
créer/mettre à jour/sauter/conflit/mot de passe/utilisateur non mappé, dry-run identique à
l'exécution réelle — est réutilisée telle quelle derrière l'interface.

## Fonctionnalités

- **Tâches & Export** : liste des tâches planifiées, sélection par cases à cocher,
  filtres d'inclusion/exclusion, export vers un dossier choisi (XML brut + `manifest.json`).
- **Archive .zip optionnelle** : l'export peut produire un `.zip` (compression Deflate)
  en un clic, **chiffré AES-256** (variante WinZip AES) si un mot de passe est saisi.
  `manifest.json` et les empreintes SHA-256 sont embarqués tels quels — aucun recalcul.
- **Import** : vérification de l'archive, plan d'import affiché et **résolu tâche par tâche**
  (conflit : conserver/écraser, utilisateur non mappé : compte cible, mot de passe requis :
  saisie masquée), simulation (dry-run) ou import réel, rapport détaillé. L'import accepte
  un dossier d'export classique **ou une archive .zip** : **si le chemin saisi pointe vers
  un `.zip`, l'extraction et la validation (manifeste + empreintes) sont automatiques** —
  sans étape manuelle, l'archive est extraite dans `%TEMP%\tsbak-extract\` (purge après
  7 jours), puis vérifiée/planifiée/importée. Une archive corrompue ou malveillante ne crée
  rien (protection zip-slip incluse).
- **Logs** : journal horodaté par jour dans `%LOCALAPPDATA%\tsbak\logs\` (rétention 14 jours),
  consultable en direct dans l'onglet Logs, dossier ouvrable d'un clic.
- **Élévation sans redémarrage (sidecar)** : l'import réel s'exécute dans un **processus
  enfant élevé temporaire** (`TaskBackupRestore.exe --helper-import`, invite UAC via
  `ShellExecuteExW runas`) — **l'interface reste ouverte** pendant l'import, attend le
  rapport du processus et l'affiche. Plus de relance complète de l'application : l'import
  non élevé s'élève à la volée, la simulation et l'export n'exigent rien.

## Livraison

- `dist\TaskBackupRestore.exe` — **exe portable unique**, aucune installation (exige WebView2,
  préinstallé sur Windows 10/11).
- Le CLI reste disponible : `tsbak\target\release\tsbak.exe` (`list` / `export` / `import` / `validate`).

## Compatibilité Windows

| Composant | Server 2008 R2 / 2012 / 2012 R2 | Server 2016 / 2019 / 2022 / 2025, Win10/11 |
|---|---|---|
| `tsbak.exe` (CLI, même moteur) | ✅ **oui** | ✅ oui |
| `TaskBackupRestore.exe` (interface) | ❌ non | ✅ oui |

**Pourquoi ?** L'interface repose sur WebView2 (Tauri 2). Microsoft a cessé
toutes les mises à jour de WebView2 sur Windows 7 / Server 2008 R2 / 2012 :
il y est figé en **version 109, non patchée depuis octobre 2023** — inacceptable
pour un outil qui s'exécute en administrateur sur des serveurs de production.
La version 109 s'installe sur ces systèmes, mais ne reçoit plus aucun correctif
de sécurité.

**La solution livrée** : le **même moteur** (`tsbak`) est disponible en ligne
de commande (`dist\tsbak.exe`) et fonctionne sur **toutes** les versions de
Windows Server de 2008 R2 à 2025+ (COM Task Scheduler présent depuis Vista,
aucune dépendance runtime). Des assistants `export.cmd` / `import.cmd` /
`validate.cmd` fournissent une expérience guidée sur les anciens serveurs, et
le CLI journalise désormais dans le **même dossier** que l'interface
(`%LOCALAPPDATA%\tsbak\logs\`).

## Comment importer

### Avec l'interface (Windows 10/11, Server 2016+) — 5 étapes

1. **Exporter** sur la machine source : onglet *Tâches & Export*, cocher les
   tâches (ou tout sélectionner), choisir un dossier de destination — l'export
   peut produire directement un **`.zip` (éventuellement chiffré AES-256)**.
2. **Transférer** l'archive (dossier ou `.zip`) vers la machine cible (clé
   USB, réseau…).
3. **Importer** : onglet *Import*, saisir le chemin du dossier ou du `.zip`
   (un `.zip` est détecté, extrait et validé automatiquement — mot de passe
   demandé s'il est chiffré). Cliquer *Vérifier l'archive* puis *Préparer le
   plan*.
4. **Résoudre le plan** : conflits (conserver/écraser), utilisateurs non
   mappés (compte cible) et mots de passe requis (champ masqué) — puis
   *Simuler (dry-run)* pour vérifier sans rien écrire, ou *Importer*
   directement. Si l'interface n'est pas administrateur, une invite **UAC**
   s'affiche : l'import s'exécute dans un processus enfant élevé, l'interface
   reste ouverte.
5. **Vérifier** le rapport (créées / mises à jour / ignorées / bloquées /
   échecs) et consulter l'onglet *Logs* pour le détail horodaté.

### Sans interface (Server 2008 R2 → 2025+)

`dist\` contient des assistants interactifs — double-clic sur **`import.cmd`**
**en administrateur** (clic droit > *Exécuter en tant qu'administrateur*), il
pose les questions (dossier, dossier cible, mots de passe, simulation) et
garde le journal dans `dist\logs\`. En ligne de commande :

```bat
:: 1. Vérifier l'archive
 tsbak.exe validate C:\tsbak\export-2026-01-01
:: 2. Simuler (rien n'est écrit)
 tsbak.exe import C:\tsbak\export-2026-01-01 --dry-run
:: 3. Importer réellement (en administrateur)
 tsbak.exe import C:\tsbak\export-2026-01-01
::    Options : --folder \Restauration-2026  --password-file mdp.txt  --user-map SOURCE:DEST
```

## Compilation

```bash
# App GUI (produit dist\TaskBackupRestore.exe équivalent, sans bundle)
cd src-tauri
cargo build --release

# Crate de base + tests
cd ../tsbak
cargo test
```

## Test réel de bout en bout

Un test `#[ignore]` exécute le flux complet contre le **vrai** scheduler Windows
(COM) : export de quelques tâches réelles vers un `.zip` chiffré AES-256, refus du
mauvais mot de passe, extraction + validation, plan, simulation et import réel
(tâches identiques → rien n'est écrit), puis vérification du rapport et des logs
(secret absent).

```bash
cd src-tauri
cargo test e2e_export_zip_aes_then_reimport -- --ignored --nocapture
```

## Sécurité

- **Les mots de passe ne sont jamais journalisés**, jamais écrits sur disque, jamais
  renvoyés à l'interface : ils sont saisis en champ masqué, transmis à `import_set_password`
  et conservés uniquement en mémoire dans l'état backend (effaçables via
  « Effacer les mots de passe »). Le **mot de passe d'archive .zip** suit la même règle :
  utilisé en mémoire pour chiffrer/déchiffrer, il n'apparaît dans aucun log.
- **Transfert vers le processus élevé** : un import administrateur transfère les décisions
  et mots de passe au processus enfant via un fichier de réponses JSON temporaire dans
  `%TEMP%\task-backup-restore\` (ACL utilisateur uniquement), **supprimé par le helper et par
  l'interface dans tous les chemins, y compris les erreurs**. C'est le seul canal possible
  vers un processus élevé séparé (pas de pipe avec `runas`). Il n'est jamais journalisé ni
  recopié ailleurs.
- Les XML exportés ne contiennent aucun secret : Windows retire automatiquement les mots de
  passe des tâches `TASK_LOGON_PASSWORD`.
- Les fichiers de logs ne contiennent que des chemins de tâches, noms d'utilisateurs et
  compteurs (niveau INFO/WARN/ERROR). L'extraction d'une archive .zip valide d'abord le
  manifeste et les empreintes **avant** d'installer quoi que ce soit, et refuse les chemins
  d'entrée non sûrs (`../`, chemins absolus — protection zip-slip).

## Notes

- L'import en écriture (création/mise à jour) exige des droits administrateur ; la
  simulation et l'export fonctionnent sans. L'interface demande l'élévation **à la volée**
  (processus enfant `--helper-import`), sans jamais se fermer.
- Le mode helper est réutilisable en ligne de commande :
  `TaskBackupRestore.exe --helper-import --dir <archive> --answers <réponses.json> --result <résultat.json>`.
  Le fichier de réponses utilise le format `AnswerFile` de tsbak (mapping utilisateurs,
  décisions de conflit, mots de passe, tâches à sauter).
- Premier test réel de l'implémentation COM (`windows_impl.rs`) effectué le 8 septembre 2026
  sur machine Windows : deux bugs réels trouvés et corrigés (signatures `UserId`/`LogonType`
  à paramètres de sortie, et violation d'accès à la fermeture due à `CoUninitialize` avant
  libération de l'interface COM).