# tsbak

CLI Rust pour exporter/reimporter les taches planifiees Windows (Task
Scheduler), en XML brut, pour migration entre machines ou restauration.

## Compilation

Ce depot compile sur toute plateforme (Linux/macOS/Windows) car l'acces COM
reel au Task Scheduler est isole derriere `cfg(windows)` (voir
`src/scheduler/windows_impl.rs`) et derriere une dependance `windows-rs`
placee dans `[target.'cfg(windows)'.dependencies]` : elle n'est telechargee
et compilee que lorsqu'on cible Windows.

```bash
cargo build --release
cargo test
```

Sur Windows, `cargo build --release` produit un `tsbak.exe` fonctionnel
utilisant reellement `ITaskService`/`ITaskFolder`/`IRegisteredTask`. Sur les
autres plateformes, seules les commandes ne necessitant pas d'acces au
planificateur (`validate`) fonctionnent ; `list`/`export`/`import`
retournent une erreur claire "fonctionnalite non disponible sur cette
plateforme" (code de sortie 2).

**Testé sur machine Windows réelle le 8 septembre 2026** (`list`,
`export`, `validate`, import dry-run sur le planificateur local). Trois
bugs réels ont été trouvés et corrigés à cette occasion :

- `IPrincipal::UserId` et `IPrincipal::LogonType` utilisent des paramètres
  de sortie (`*mut BSTR` / `*mut TASK_LOGON_TYPE`) dans les bindings
  `windows-rs` 0.58, et non un retour direct ;
- les features `Win32_Security` et `Win32_System_Threading` manquaient dans
  `Cargo.toml` pour la détection d'élévation ;
- `CoUninitialize` dans `Drop` était appelé avant la libération de
  l'interface COM (`ITaskService`), provoquant une violation d'accès
  (STATUS_ACCESS_VIOLATION) à la fermeture du processus — corrigé en
  libérant l'interface avant la déinitialisation COM.

## Architecture

- `src/scheduler/mod.rs` : trait `TaskSchedulerApi`, seule porte d'entree
  vers le planificateur pour toute la logique metier.
- `src/scheduler/windows_impl.rs` : implementation reelle via COM (Windows
  uniquement).
- `src/scheduler/mock.rs` : implementation en memoire utilisee par tous les
  tests (unitaires et d'integration), permettant de valider toute la
  logique de classification sans machine Windows.
- `src/export.rs` : parcours + ecriture XML brut + manifest.json, filtres
  `--include`/`--exclude`.
- `src/import.rs` : `load_and_verify` (verification des empreintes SHA-256 +
  bonne formation XML), `build_plan` (classification identique pour
  dry-run et execution reelle), `execute_plan` (ecriture ou simulation).
- `src/password.rs` : resolution des mots de passe (fichier, fichier de
  reponses, invite masquee), jamais journalises.
- `src/answers.rs` : format du fichier de reponses JSON pour un import
  100% non interactif, mapping utilisateurs, comptes bien connus.
- `src/wizard.rs` : `Interactor` (TTY ou non-interactif) pour les
  decisions de conflit et de mapping utilisateur.
- `src/error.rs` : traduction des HRESULT frequents en messages clairs.
- `src/model.rs` : structures partagees (manifeste, classification,
  rapport d'execution et son code de sortie).

## Choix de conception notables

- **Classification avant ecriture, unique pour dry-run et execution
  reelle** : `build_plan` calcule une seule fois la decision
  (create/update/skip/conflit/mot de passe requis/utilisateur non mappe).
  `execute_plan(..., dry_run)` ne fait que sauter l'appel `RegisterTask`
  en dry-run ; le rapport affiche est donc garanti identique.
- **Aucune question posee pour une tache qui ne necessite aucune
  ecriture** : une tache deja identique sur la cible est classee
  `SkipIdentical` avant toute resolution de mot de passe ou de mapping
  utilisateur.
- **Mapping utilisateur uniquement quand aucune autre validation
  n'existe** : les taches `TASK_LOGON_PASSWORD` sont validees par Windows
  lui-meme au moment de `RegisterTask` (echec HRESULT traduit
  explicitement) ; le mapping explicite (`--user-map`/fichier de reponses)
  n'est donc requis que pour S4U et les jetons interactifs, qui n'ont pas
  cette seconde chance.
- **Code de sortie** : 0 si tout a reussi, 1 si des taches sont bloquees
  (conflit/mot de passe/mapping) sans qu'aucune ecriture tentee n'ait
  echoue, 2 si toutes les ecritures tentees ont echoue ou en cas d'erreur
  fatale (droits insuffisants, plateforme non supportee, manifeste
  invalide).

## Format du fichier de reponses (`--answer-file`)

```json
{
  "user_map": { "OLDPC\\bob": "NEWPC\\bob" },
  "passwords": { "NEWPC\\bob": "motdepasse" },
  "conflict_decisions": { "\\MonDossier\\MaTache": "overwrite" },
  "skip_tasks": ["\\Autre\\TacheASauter"]
}
```

## Format du fichier de mots de passe (`--password-file`)

```
# commentaires acceptes
DOMAIN\alice=motdepasse1
NEWPC\bob=motdepasse2
```

## Journalisation

Chaque commande ecrit un journal horodate dans
`%LOCALAPPDATA%\tsbak\logs\tsbak-AAAA-MM-JJ.log` (le **meme dossier** que
l'interface graphique Task backup and restore), avec une retention de 14 jours. La
journalisation est best-effort : elle ne fait jamais echouer la commande.
Les mots de passe n'y figurent jamais. `TSBAK_LOG_DIR` permet de derouter le
dossier de journaux (utilise par les tests).

## Compatibilite Windows

- **CLI (`tsbak.exe`)** : Windows Server **2008 R2 a 2025+**, Windows 10/11.
  Aucune dependance runtime : seul le COM Task Scheduler (present depuis
  Vista) et des API Win32 anciennes sont utilises. Note honnete : le
  compilateur Rust (>= 1.76) declare officiellement Windows 10 comme socle
  minimum ; ce binaire n'utilise toutefois que des API existantes depuis
  Vista/7 et fonctionne en pratique sur 2008 R2 — a valider sur une vraie
  machine avant deploiement massif.
- **GUI (`TaskBackupRestore.exe`)** : Windows 10/11 et Windows Server 2016 et plus,
  car elle repose sur WebView2, dont Microsoft a cesse les mises a jour sur
  Windows 7/Server 2008 R2 et 2012 (fige en version 109, non patchee depuis
  octobre 2023).

## Ligne de commande

```bash
# Liste des taches (recursif)
tsbak.exe list --recursive

# Liste sans les taches systeme \Microsoft\
tsbak.exe list --hide-microsoft

# Export de toutes les taches
tsbak.exe export C:\tsbak\export-2026-01-01

# Export sans les taches systeme \Microsoft\ (comme l'interface)
tsbak.exe export C:\tsbak\export-2026-01-01 --hide-microsoft

# Export avec motifs d'inclusion/exclusion
# (--hide-microsoft equivaut a ajouter --exclude "\Microsoft\*")
tsbak.exe export C:\tsbak\export-2026-01-01 --include "\Backup\*" --exclude "\Microsoft\*"
```

## Import pas a pas (CLI)

Sur la machine cible, en **administrateur** (clic droit > Executer en tant
qu'administrateur) :

```bash
# 1. Verifier l'archive copiee (integrite + empreintes SHA-256)
tsbak.exe validate C:\tsbak\export-2026-01-01

# 2. Simuler l'import (rien n'est ecrit — recommandé avant tout import reel)
tsbak.exe import C:\tsbak\export-2026-01-01 --dry-run

# 3. Import reel
tsbak.exe import C:\tsbak\export-2026-01-01
```

Options frequentes :

- `--folder \Restauration-2026` : restaure chaque tache **sous ce dossier**
  du planificateur en preservant la structure d'origine (evite les
  collisions si le dossier existe deja, regroupe toutes les taches
  restaurees) ;
- `--password-file mdp.txt` : fournit les mots de passe des comptes
  (`DOMAINE\user=motdepasse`, un par ligne) ;
- `--user-map OLDPC\user:NEWPC\user` : remplace un compte source par un
  compte cible ;
- `--skip-password-tasks` : ignore (sans bloquer) les taches dont le mot de
  passe n'est pas fourni ;
- `--yes` : repond oui par defaut aux questions non bloquantes.

Des assistants interactifs (`export.cmd`, `import.cmd`, `validate.cmd`) sont
fournis dans `dist/` : ils posent les questions, executent et conservent le
journal dans `dist\logs\`.
