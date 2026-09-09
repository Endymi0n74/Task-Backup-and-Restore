# Changelog

## 1.1.0 — 2026-09-10

### Nouvelles fonctionnalités

- **CLI : masquage des tâches Microsoft** : nouvelles options `--hide-microsoft`
  sur `tsbak list` et `tsbak export` — les tâches système `\Microsoft\` sont
  exclues, comme dans l'interface graphique (défaut : affichées/exportées).

## 1.0.0 — 2026-09-10

Première version stable publiée.

### Nouvelles fonctionnalités

- **Archives `.zip` restreintes au contenu d'export** : une archive ne contient plus que les
  XML de tâches planifiées et le `manifest.json`. Tout autre fichier présent dans le dossier
  d'export (ancienne archive, journal, note…) est **ignoré**, à la compression comme à
  l'extraction. Une archive tierce contenant des fichiers étrangers n'écrit plus ces fichiers
  sur disque à l'import.
- **Tâches Microsoft masquées par défaut** : dans l'onglet *Tâches & Export*, les tâches
  sous `\Microsoft\` (tâches système) sont masquées par défaut — une case à cocher
  « Masquer les tâches Microsoft » permet de les afficher. Le compteur et la barre de statut
  indiquent le nombre de tâches masquées.

### Améliorations

- Nettoyage de code : suppression de fonctions mortes (`manifest_summary`, `run_helper`).
- Version unifiée à `1.0.0` (CLI, interface et manifeste Tauri).

### Déjà inclus dans les versions précédentes (0.1.x, non publiées séparément)

- Export XML brut + `manifest.json` (empreintes SHA-256), filtres include/exclude.
- Import avec plan tâche par tâche : créer / mettre à jour / ignorer / conflit /
  utilisateur non mappé / mot de passe requis, simulation (dry-run) identique à
  l'exécution réelle.
- Archive `.zip` optionnelle, chiffrée AES-256 (WinZip AES).
- Élévation à la volée (processus enfant administrateur, invite UAC, interface ouverte).
- Journalisation partagée CLI/GUI (`%LOCALAPPDATA%\tsbak\logs\`, rétention 14 jours).
- Assistants `.cmd` (export/import/validate) pour les serveurs sans interface.
- Guide illustré PDF complet (`dist/Guide-tsbak.pdf`).