# Migration des tâches planifiées — Server 2008 R2 → Server 2022

Guide pas à pas pour migrer les **tâches planifiées** d'un ancien serveur
(Windows Server **2008 R2**) vers un nouveau (**Server 2022**) avec `tsbak`
(CLI + interface).

Ce guide utilise la **ligne de commande sur les deux machines** : c'est la méthode
recommandée, car l'interface graphique (`TaskBackupRestore.exe`) nécessite WebView2,
que Microsoft ne met plus à jour sur 2008 R2 (voir [README](README.md#compatibilit%C3%A9-windows)).

> 📄 L'illustration complète des commandes et des assistants se trouve dans le
> [guide PDF](dist/Guide-tsbak.pdf). Ce document est la procédure ciblée « migration ».

---

## Vue d'ensemble

| # | Étape | Machine | Droits requis |
|---|---|---|---|
| 1 | Préparer et tester `tsbak.exe` | Source (2008 R2) | — |
| 2 | **Exporter** les tâches (XML + manifeste) | Source (2008 R2) | Administrateur * |
| 3 | **Vérifier** l'intégrité de l'archive | Source (2008 R2) | — |
| 4 | **Transférer** l'archive vers la cible | — | — |
| 5 | Préparer la cible (comptes, mots de passe) | Cible (2022) | Administrateur |
| 6 | **Importer en simulation** (dry-run) | Cible (2022) | — |
| 7 | **Résoudre** les blocages (mappings, mots de passe) | Cible (2022) | — |
| 8 | **Importer réellement** | Cible (2022) | Administrateur |
| 9 | Vérifier + post-migration | Cible (2022) | — |

\* L'export de tâches appartenant à **d'autres comptes** exige une session
administrateur sur la source ; l'export des seules tâches du compte courant
fonctionne sans élévation.

---

## Étape 1 — Préparation sur la source (2008 R2)

1. **Copier `tsbak.exe`** (et éventuellement `export.cmd`, `import.cmd`,
   `validate.cmd`) du dossier `dist\` vers la source, par exemple `C:\tsbak\`.
   Le CLI est **portable** : aucune installation, aucune dépendance runtime
   (seul le COM Task Scheduler, présent depuis Vista, est utilisé).

2. **Tester rapidement le binaire** :

   ```bat
   C:\tsbak\tsbak.exe --version
   C:\tsbak\tsbak.exe list --recursive
   ```

   > ⚠️ Le compilateur Rust déclare officiellement Windows 10 comme socle
   > minimum ; en pratique `tsbak.exe` n'utilise que des API présentes depuis
   > Vista/7 et fonctionne sur 2008 R2 — ce test de 10 secondes le confirme
   > sur votre machine avant tout déploiement.

3. **Choisir ce qu'on migre** : en général on **n'importe pas** les tâches
   système `\Microsoft\` (elles sont recréées par Windows) ni celles des
   produits qu'on abandonne. `--hide-microsoft` (v1.1.0+) les exclut d'un
   coup ; sinon `--include` / `--exclude` permettent une sélection fine.

---

## Étape 2 — Exporter sur la source

Dans une invite **en administrateur** (clic droit → *Exécuter en tant
qu'administrateur*), sur la source :

```bat
:: Export de TOUTES les tâches (y compris système \Microsoft\)
C:\tsbak\tsbak.exe export C:\tsbak\export-2026-09-10

:: Recommandé : exclure les tâches système
C:\tsbak\tsbak.exe export C:\tsbak\export-2026-09-10 --hide-microsoft

:: Sélection fine par motifs (inclusion / exclusion répétables)
C:\tsbak\tsbak.exe export C:\tsbak\export-2026-09-10 --include "\Backup\*" --exclude "\Microsoft\*" --exclude "\Logiciel-vieillot\*"
```

Résultat attendu :

```
168 tache(s) exportee(s) vers C:\tsbak\export-2026-09-10
```

Le dossier contient :

- un **XML brut par tâche** (jamais modifié), nommé `Dossier__Sous-dossier__Tache.xml` ;
- un **`manifest.json`** : chemin d'origine, type de connexion, et **empreinte
  SHA-256** de chaque XML — c'est lui qui garantit l'intégrité au transfert.

> 💡 Les XML exportés ne contiennent **aucun mot de passe en clair** : Windows
> les retire automatiquement des tâches `TASK_LOGON_PASSWORD`. Les secrets
> devront être fournis à l'import (étape 5).

---

## Étape 3 — Vérifier l'archive

Toujours sur la source (ou n'importe où, `validate` ne touche pas au planificateur) :

```bat
C:\tsbak\tsbak.exe validate C:\tsbak\export-2026-09-10
:: => Archive valide : 168 tache(s), exportee(s) le ... depuis 'SRV-2008R2'.
```

Cette vérification contrôle le manifeste, les empreintes SHA-256 et le bon
format des XML. **Toute altération (transfert corrompu, édition manuelle) est
détectée ici** et à l'import.

---

## Étape 4 — Transférer l'archive

Copier le **dossier entier** `export-2026-09-10` vers la cible (2022), par
exemple : clé USB, partage réseau, SFTP…

- L'intégrité est déjà garantie par les empreintes : un fichier tronqué sera
  refusé à l'import. Vous pouvez tout de même re-valider après transfert :
  `tsbak.exe validate D:\tsbak\export-2026-09-10`.
- Si l'archive transite par un canal non fiable, préférer un canal chiffré
  (SFTP, VPN…) — les XML contiennent des comptes et chemins sensibles.

---

## Étape 5 — Préparer la cible (2022)

Avant l'import réel, sur le **nouveau** serveur :

1. **Créer les comptes** qui doivent exister pour que les tâches s'exécutent
   (comptes de service, utilisateurs…) — sous leurs noms **cibles**.
2. **Noter les correspondances** d'utilisateurs entre l'ancien et le nouveau
   serveur, par exemple :
   - `SRV-2008R2\svc_backup` → `SRV-2022\svc_backup` (même nom, machine différente) ;
   - `ANCIEN-DOM\svc_batch` → `NOUVEAU-DOM\svc_batch` (domaine renommé).
3. **Préparer le fichier de mots de passe** (une entrée par ligne,
   `#` = commentaire) :

   ```text
   # mots_de_passe.txt — à supprimer après l'import !
   SRV-2022\svc_backup=MonM0tDeP@sse
   NOUVEAU-DOM\svc_batch=AutreSecret
   ```

   > 🔐 Le fichier contient des secrets : le garder hors du partage, le
   > supprimer dès la fin de la migration. Il n'est **jamais journalisé** par
   > tsbak.

4. (Optionnel) Choisir un **dossier cible** du planificateur si vous voulez
   tester sans écraser l'existant, ex. `\Restauration-2026-09-10`.

---

## Étape 6 — Importer en simulation (dry-run)

Sur la cible, **sans rien écrire** :

```bat
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 --dry-run
```

Le rapport liste, pour chaque tâche, la décision prise par le moteur
(identique en simulation et en import réel) :

```
=== Plan d'import (168 tache(s)) ===
CREATE                  \Backup\Nettoyage
UPDATE                  \Surveillance\Watchdog
SKIP (identique)        \Divers\TacheInchangee
CONFLIT                 \Ancien\Planif
MOT DE PASSE REQUIS     \Batch\ImportFichiers
UTILISATEUR NON MAPPE   \S4U\TaskS4U
...
```

Trois cas à résoudre avant l'import réel :

- **CONFLIT** : la tâche existe déjà sur la cible et diffère → décider
  « écraser » (`--conflict-policy overwrite`) ou « ignorer » (`skip`) ;
- **MOT DE PASSE REQUIS** : fournir le secret (étape 7) ou sauter la tâche ;
- **UTILISATEUR NON MAPPE** : le compte source n'existe pas tel quel sur la
  cible → fournir le mapping (étape 7).

---

## Étape 7 — Résoudre les blocages

Relancer la simulation avec les résolutions, jusqu'à obtenir un plan propre :

```bat
:: Mapping d'utilisateurs source:cible (répétable)
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 --dry-run ^
    --user-map "SRV-2008R2\svc_backup:SRV-2022\svc_backup" ^
    --user-map "ANCIEN-DOM\svc_batch:NOUVEAU-DOM\svc_batch"

:: + mots de passe (fichier "utilisateur=mot_de_passe")
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 --dry-run ^
    --user-map "SRV-2008R2\svc_backup:SRV-2022\svc_backup" ^
    --password-file C:\tsbak\mots_de_passe.txt

:: Décider pour les conflits
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 --dry-run --conflict-policy overwrite

:: Ou : ignorer (sans bloquer) les tâches dont on n'a pas le mot de passe
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 --dry-run --skip-password-tasks

:: Tout combiné, avec restauration sous un dossier dédié (test)
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 --dry-run ^
    --user-map "SRV-2008R2\svc_backup:SRV-2022\svc_backup" ^
    --password-file C:\tsbak\mots_de_passe.txt ^
    --conflict-policy overwrite ^
    --folder "\Restauration-2026-09-10"
```

> 💡 Les comptes bien connus (SYSTEM, LOCAL SERVICE, NETWORK SERVICE…) ne
> nécessitent **jamais** de mapping : ils existent identiquement partout.

---

## Étape 8 — Importer réellement

Une fois la simulation au vert, relancer **sans** `--dry-run`, dans une
invite **en administrateur** :

```bat
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 ^
    --user-map "SRV-2008R2\svc_backup:SRV-2022\svc_backup" ^
    --password-file C:\tsbak\mots_de_passe.txt ^
    --conflict-policy overwrite
```

Rapport attendu :

```
=== Rapport d'import ===
Crees (8) : ...
Mis a jour (2) : ...
Ignores (155) : ...
Bloques (0) :
```

**Code de sortie** : `0` = tout a réussi ; `1` = des tâches restent bloquées
ou en échec partiel ; `2` = échec complet. En cas de code ≠ 0, consulter le
rapport puis `%LOCALAPPDATA%\tsbak\logs\` pour le détail horodaté.

---

## Étape 9 — Vérifications et post-migration

1. **Vérifier dans le planificateur** (`taskschd.msc`) : les tâches sont
   présentes, activées, avec les bons comptes.
2. **Exécuter les tâches critiques** en test (clic droit → *Exécuter*).
3. **Vérifier les journaux** :
   ```bat
   notepad %LOCALAPPDATA%\tsbak\logs\tsbak-2026-09-10.log
   ```
4. **Supprimer le fichier de mots de passe** et l'archive du serveur une fois
   la migration validée.
5. **Geler l'ancien serveur** (ou désactiver ses tâches) avant de le
   décommissionner, pour éviter deux exécutions concurrentes.

---

## Automatisation (import non interactif)

Pour planifier la restauration (ex. reprise sur serveur de secours) :

```json
// reponses.json
{
  "user_map": { "SRV-2008R2\\svc_backup": "SRV-2022\\svc_backup" },
  "passwords": { "SRV-2022\\svc_backup": "MonM0tDeP@sse" },
  "conflict_decisions": { "\\Ancien\\Planif": "overwrite" },
  "skip_tasks": ["\\S4U\\TaskS4U"]
}
```

```bat
C:\tsbak\tsbak.exe import D:\tsbak\export-2026-09-10 --answer-file reponses.json --yes
schtasks /create /sc weekly /tn "tsbak-restauration" /tr "C:\tsbak\import.cmd" /ru SYSTEM
```

> 🔐 `reponses.json` contient des secrets : ACL utilisateur uniquement,
> suppression après usage — tsbak n'en garde aucune trace.

---

## Dépannage rapide

| Symptôme | Cause / solution |
|---|---|
| « Accès refusé » / code 2 | Invite **non administrateur** → relancer en administrateur |
| « Utilisateur non mappé » | Compte source absent sur la cible → `--user-map "source:cible"` |
| « Mot de passe requis » | Fournir `--password-file` (ou champ masqué dans l'interface), ou `--skip-password-tasks` |
| `0x80070534` (mappage compte) | L'utilisateur cible n'existe pas ou n'est pas accessible → créer le compte / vérifier le mapping |
| `0x8007052E` / `0x80041318` | Nom d'utilisateur ou mot de passe incorrect → corriger le fichier de mots de passe |
| « Conflit non résolu » | Tâche déjà présente et différente → `--conflict-policy overwrite` (ou `skip`) |
| « Archive invalide » à l'import | Manifeste/empreinte/XML altérés → ré-exporter proprement, ne pas éditer les XML à la main |
| Tâche manquante après import | Vérifier les blocs (`code 1`) dans le rapport et le journal — probablement password/mapping non résolus |

---

## Rappel de sécurité

- Les mots de passe ne sont **jamais** journalisés ni écrits sur disque par
  tsbak — ils transitent uniquement en mémoire (ou dans un fichier que *vous*
  fournissez, à supprimer après usage).
- L'import vérifie manifeste + empreintes + XML bien formés **avant** toute
  écriture : une archive corrompue ne crée rien.
- Préférer un canal de transfert chiffré, et tester d'abord la restauration
  sous un `--folder` dédié avant l'import final.