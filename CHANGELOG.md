# Changelog

## Non publié

### Export

- **Archive `.zip` autonome** : en mode archive, les XML et le `manifest.json` sont écrits
  dans un dossier temporaire (`%TEMP%\tsbak-export\<pid>-<nanos>`, purgé au-delà de 7 jours)
  **puis** compressés vers le dossier choisi — le dossier de destination ne reçoit plus que
  le fichier `.zip`, jamais les fichiers d'export à côté. Le nom d'archive est validé avant
  toute écriture (nom invalide = aucun fichier sur disque), une archive partielle est
  supprimée si la compression échoue, et le dossier temporaire est supprimé dans tous les
  cas. Tests unitaires `MockScheduler` dédiés (`export_archive_tests`).

### Interface

- **Thème commutable** : `ui/style.css` ne définit plus la palette (structure et composants
  seulement). Le **thème publié** vit dans `ui/themes/legacy.css` (palette historique, active
  par défaut, rendu au pixel près identique à la version précédente) et la **variante locale**
  Hestia (bleu marine / orange) dans `ui/themes/hestia.css`. `tools/select-theme.ps1` bascule
  le thème actif (`-Action Set -Theme hestia|legacy`, `-Action Status`, `-Action Check
  -Require legacy`) : l'interface étant embarquée dans l'exe au moment de la compilation,
  une compilation locale peut porter la variante, tandis que `tools/make-release.ps1`
  **refuse** de produire un livrable si la variante est active.

### Outillage

- **Guide PDF automatisé** : le workflow GitHub Actions
  [`guide-pdf.yml`](.github/workflows/guide-pdf.yml) régénère `dist/Guide-tsbak.pdf` et
  `dist/Memo-motifs.pdf` (Edge headless) et les commite dès qu'une source de `dist/guide/` change
  sur la branche par défaut ; en *pull request* le job **échoue** si le PDF commité est périmé
  (les PDF régénérés sont joints en artefact). Les empreintes des sources de chaque PDF sont
  gérées par [`tools/guide-pdf.ps1`](tools/guide-pdf.ps1) (`-Action Check` | `Build` | `Update`)
  et enregistrées dans `dist/guide/pdf-sources.sha256`.

## 1.0.0 — 2026-09-12

Première version publiée (dépôt GitHub **Task Backup and Restore**). Version unifiée
`1.0.0` pour le CLI, l'interface et le manifeste Tauri.

### Export / import

- **Export** : liste des tâches planifiées, sélection par cases à cocher, filtres
  d'inclusion/exclusion (motifs `*`), export vers un dossier — XML brut + `manifest.json`
  (empreintes SHA-256).
- **Archive `.zip` restreinte au contenu d'export** : une archive ne contient que les XML de
  tâches et le `manifest.json`. Tout autre fichier présent dans le dossier d'export (ancienne
  archive, journal, note…) est **ignoré**, à la compression comme à l'extraction. Une archive
  tierce contenant des fichiers étrangers n'écrit plus ces fichiers sur disque à l'import.
- **Import** : dossier d'export **ou** archive `.zip` (détection automatique, protection
  zip-slip), plan résolu tâche par tâche (créer / mettre à jour / ignorer / conflit /
  utilisateur non mappé / mot de passe requis), **simulation (dry-run)** strictement identique
  à l'exécution réelle, puis import réel.
- **Mot de passe unique** : un champ masqué « Mot de passe unique » + bouton « Remplir avec le
  même mot de passe » applique un seul secret à toutes les tâches « Mot de passe requis » du
  plan (guide, sous-section 4.1).
- **Élévation à la volée** : l'import réel s'exécute dans un processus enfant élevé temporaire
  (invite UAC), l'interface reste ouverte et affiche le rapport à la fin.

### Ligne de commande

- `tsbak.exe` — même moteur que l'interface, **Windows Server 2008 R2 → 2025+**, aucune
  dépendance runtime (COM Task Scheduler présent depuis Vista).
- `--hide-microsoft` sur `tsbak list` et `tsbak export` : exclut les tâches système
  `\Microsoft\`, comme l'interface où elles sont masquées par défaut.

### Interface

- Onglet *Tâches & Export* : les tâches `\Microsoft\` sont **masquées par défaut** (case à
  cocher pour les afficher) ; le compteur et la barre de statut indiquent le nombre de tâches
  masquées.
- Journalisation partagée avec le CLI (`%LOCALAPPDATA%\tsbak\logs\`, rétention 14 jours),
  consultable en direct dans l'onglet *Logs*.

### Sécurité

- Les mots de passe (comptes de tâches, archives) ne sont **jamais journalisés** ni écrits sur
  disque par l'interface : ils restent en mémoire dans le backend et sont transmis au processus
  élevé par un fichier de réponses JSON temporaire (ACL utilisateur, supprimé dans tous les cas).
- Manifeste + empreintes SHA-256 + XML bien formés vérifiés **avant** toute écriture ; chemins
  non sûrs (zip-slip) et entrées étrangères refusés.

### Documentation et livraison

- Guide illustré [`dist/Guide-tsbak.pdf`](dist/Guide-tsbak.pdf) et mémo d'une page
  [`dist/Memo-motifs.pdf`](dist/Memo-motifs.pdf) (filtres d'inclusion/exclusion), sources dans
  `dist/guide/`.
- [`MIGRATION.md`](MIGRATION.md) — procédure pas à pas 2008 R2 → 2022.
- Assistants `.cmd` (`export.cmd`, `import.cmd`, `validate.cmd`) pour les serveurs sans
  interface ; [`tools/make-release.ps1`](tools/make-release.ps1) assemble le dossier de
  livraison et l'archive de release.
- Nettoyage de code : suppression de fonctions mortes (`manifest_summary`, `run_helper`).
