// Task backup and restore — logique frontend (français, sans framework).
// S'appuie sur l'API Tauri globale injectée (withGlobalTauri: true).

"use strict";

const TAURI = window.__TAURI__;

function invoke(cmd, args) {
  return TAURI.core.invoke(cmd, args || {});
}

// ---------------------------------------------------------------------------
// Aides DOM
// ---------------------------------------------------------------------------

function escapeHtml(s) {
  return String(s)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function setStatus(message, kind) {
  const bar = document.getElementById("statusbar");
  bar.textContent = message;
  bar.className = "statusbar" + (kind ? " " + kind : "");
}

function showError(err) {
  const msg = (err && err.message) || String(err || "erreur inconnue");
  setStatus("Erreur : " + msg, "error");
  console.error(err);
}

const $ = (id) => document.getElementById(id);

// ---------------------------------------------------------------------------
// Onglets
// ---------------------------------------------------------------------------

document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
    document.querySelectorAll(".tab-page").forEach((p) => p.classList.remove("active"));
    tab.classList.add("active");
    $("tab-" + tab.dataset.tab).classList.add("active");
  });
});

// ---------------------------------------------------------------------------
// Élévation
// ---------------------------------------------------------------------------

async function initElevation() {
  const elevated = await invoke("is_elevated");
  if (elevated) {
    $("elevation-ok").classList.remove("hidden");
  } else {
    $("elevation-banner").classList.remove("hidden");
  }
  return elevated;
}


// ---------------------------------------------------------------------------
// Onglet Tâches & Export
// ---------------------------------------------------------------------------

let tasks = [];

// Les tâches sous \Microsoft\ sont des tâches système : elles sont masquées
// par défaut (case à cocher "Masquer les tâches Microsoft").
const isMicrosoftTask = (path) => path.toLowerCase().startsWith("\\microsoft\\");

function visibleTasks() {
  return $("hide-microsoft").checked
    ? tasks.filter((t) => !isMicrosoftTask(t.path))
    : tasks;
}

function renderTasks() {
  const rows = visibleTasks();
  const tbody = $("task-table").querySelector("tbody");
  tbody.innerHTML = rows
    .map(
      (t) => `
      <tr>
        <td class="col-check"><input type="checkbox" data-path="${escapeHtml(t.path)}" /></td>
        <td>${escapeHtml(t.path)}</td>
        <td>${escapeHtml(t.user_id || "—")}</td>
        <td>${escapeHtml(t.logon_type || "—")}</td>
      </tr>`
    )
    .join("");
  const hidden = tasks.length - rows.length;
  $("task-count").textContent =
    hidden > 0
      ? `${rows.length} tâche(s) affichée(s) — ${hidden} tâche(s) Microsoft masquée(s).`
      : `${rows.length} tâche(s) affichée(s).`;
}

$("hide-microsoft").addEventListener("change", () => {
  if (tasks.length > 0) renderTasks();
});

$("btn-load-tasks").addEventListener("click", async () => {
  setStatus("Chargement des tâches…");
  try {
    tasks = await invoke("list_tasks", { recursive: true });
    renderTasks();
    const hidden = tasks.filter((t) => isMicrosoftTask(t.path)).length;
    setStatus(
      hidden > 0
        ? `${tasks.length} tâche(s) planifiée(s) trouvée(s) — ${hidden} tâche(s) Microsoft masquée(s) (décochez « Masquer les tâches Microsoft » pour les voir).`
        : `${tasks.length} tâche(s) planifiée(s) trouvée(s).`,
      "success"
    );
  } catch (e) {
    showError(e);
  }
});

$("btn-check-all").addEventListener("click", () => {
  $("task-table").querySelectorAll("tbody input[type=checkbox]").forEach((cb) => (cb.checked = true));
});

$("btn-uncheck-all").addEventListener("click", () => {
  $("task-table").querySelectorAll("tbody input[type=checkbox]").forEach((cb) => (cb.checked = false));
});

// ---------------------------------------------------------------------------
// Export ZIP (optionnel, chiffrement AES-256)
// ---------------------------------------------------------------------------

$("zip-enabled").addEventListener("change", () => {
  $("zip-name").disabled = !$("zip-enabled").checked;
  syncZipEncryptState();
});

$("zip-encrypt").addEventListener("change", syncZipEncryptState);

function syncZipEncryptState() {
  const enabled = $("zip-enabled").checked;
  $("zip-encrypt").disabled = !enabled;
  $("zip-password").disabled = !enabled || !$("zip-encrypt").checked;
  if (!$("zip-encrypt").checked) $("zip-password").value = "";
}

$("btn-export").addEventListener("click", async () => {
  try {
    const selected = Array.from(
      $("task-table").querySelectorAll("tbody input[type=checkbox]:checked")
    ).map((cb) => cb.dataset.path);

    const parse = (id) =>
      $("filter-" + id).value
        .split(/[,\n]/)
        .map((s) => s.trim())
        .filter(Boolean);

    const includePatterns = parse("include");
    const excludePatterns = parse("exclude");

    if (selected.length === 0 && includePatterns.length === 0) {
      const ok = window.confirm(
        "Aucune tâche sélectionnée : toutes les tâches seront exportées. Continuer ?"
      );
      if (!ok) return;
    }

    const dir = await invoke("pick_folder", { title: "Choisir le dossier d'export" });
    if (!dir) return;

    // Options d'archive ZIP (le mot de passe n'est jamais journalisé).
    const zipEnabled = $("zip-enabled").checked;
    const zipEncrypt = zipEnabled && $("zip-encrypt").checked;
    const zipPassword = zipEncrypt ? $("zip-password").value : "";
    if (zipEncrypt && !zipPassword) {
      setStatus("Chiffrement demandé : saisissez un mot de passe d'archive.", "error");
      return;
    }

    setStatus("Export en cours…");
    const include = [...selected, ...includePatterns];
    const summary = await invoke("export_tasks", {
      dir,
      include,
      exclude: excludePatterns,
      zipName: zipEnabled ? $("zip-name").value.trim() || null : null,
      zipPassword: zipPassword || null,
    });
    const zipNote = summary.zipPath
      ? ` — archive ${summary.zipEncrypted ? "chiffrée AES-256" : "ZIP"} : ${summary.zipPath}`
      : "";
    setStatus(
      `${summary.exported.length} tâche(s) exportée(s) vers ${dir} (${summary.skippedByFilter} ignorée(s) par les filtres)${zipNote}.`,
      "success"
    );
  } catch (e) {
    showError(e);
  }
});

// ---------------------------------------------------------------------------
// Onglet Import — état
// ---------------------------------------------------------------------------

let plan = [];
// Décisions collectées dans l'interface, transmises telles quelles au backend.
const decisions = {
  conflicts: {}, // chemin -> "overwrite" | "skip"
  userMap: {}, // source -> cible
  skipTasks: [], // chemins (sérialisé depuis un Set)
};
const skipSet = new Set();
let manifestInfo = null;

// Cache d'extraction automatique des .zip : { raw, dir }.
let resolvedCache = null;

function noteDirChanged() {
  resolvedCache = null;
}

function decisionsPayload() {
  decisions.skipTasks = [...skipSet];
  return decisions;
}

// ---------------------------------------------------------------------------
// Onglet Import — archives .zip
// ---------------------------------------------------------------------------

$("btn-pick-zip").addEventListener("click", async () => {
  try {
    const zip = await invoke("pick_zip_file", { title: "Choisir l'archive .zip d'export" });
    if (zip) {
      $("import-dir").value = zip;
      noteDirChanged();
      setStatus("Archive sélectionnée : " + zip + " — extraction automatique à la vérification/au plan.");
    }
  } catch (e) {
    showError(e);
  }
});

$("btn-extract-zip").addEventListener("click", async () => {
  try {
    const zip = $("import-dir").value.trim();
    if (!zip.toLowerCase().endsWith(".zip")) {
      setStatus("Sélectionnez d'abord une archive .zip (bouton « Choisir une archive .zip… »).", "error");
      return;
    }
    const password = $("zip-archive-password").value;
    const suggested = zip.replace(/\\/g, "/").split("/").pop().replace(/\.zip$/i, "");
    const dest = await invoke("pick_folder", { title: "Dossier où extraire l'archive" });
    if (!dest) return;
    const target = await window.prompt("Nom du dossier d'extraction", suggested);
    if (!target) return;

    setStatus("Extraction et vérification de l'archive…");
    const destDir = dest.replace(/[\\/]$/, "") + "\\" + target.replace(/[\\/:*?\"<>|]/g, "_");
    const count = await invoke("extract_zip_archive", {
      zipPath: zip,
      destDir,
      password: password || null,
    });
    $("import-dir").value = destDir;
    noteDirChanged();
    setStatus(`Archive extraite et vérifiée (${count} entrée(s)) vers ${destDir}.`, "success");
    await verifyArchive();
  } catch (e) {
    showError(e);
  }
});

// ---------------------------------------------------------------------------
// Onglet Import — vérification & plan
// ---------------------------------------------------------------------------

function currentDir() {
  return $("import-dir").value.trim();
}

/// Dossier réel à utiliser : si le chemin saisi pointe vers un .zip, il est
/// extrait et validé automatiquement (une seule fois, résultat mis en cache).
async function resolveDir() {
  const raw = currentDir();
  if (!raw) return null;
  if (!raw.toLowerCase().endsWith(".zip")) return raw;
  if (resolvedCache && resolvedCache.raw === raw) return resolvedCache.dir;
  setStatus("Archive .zip détectée : extraction et vérification automatiques…");
  const dir = await invoke("extract_zip_auto", {
    zipPath: raw,
    password: $("zip-archive-password").value || null,
  });
  resolvedCache = { raw, dir };
  setStatus("Archive extraite et vérifiée vers " + dir + ".", "success");
  return dir;
}

$("btn-browse").addEventListener("click", async () => {
  try {
    const dir = await invoke("pick_folder", { title: "Choisir le dossier d'archive à importer" });
    if (dir) {
      $("import-dir").value = dir;
      noteDirChanged();
    }
  } catch (e) {
    showError(e);
  }
});

$("btn-verify").addEventListener("click", verifyArchive);

async function verifyArchive() {
  const dir = await resolveDir();
  if (!dir) {
    setStatus("Indiquez d'abord le dossier d'archive.", "error");
    return;
  }
  try {
    const summary = await invoke("validate_archive", { dir });
    manifestInfo = summary;
    const box = $("manifest-summary");
    box.classList.remove("hidden", "error");
    if (summary.valid) {
      box.textContent = `✓ Archive valide : ${summary.taskCount} tâche(s), exportée(s) le ${summary.exportedAt} depuis « ${summary.sourceHost} ».`;
      setStatus("Archive valide.", "success");
    } else {
      box.classList.add("error");
      box.textContent = `✗ Archive invalide : ${summary.error}`;
      setStatus("Archive invalide.", "error");
    }
  } catch (e) {
    showError(e);
  }
}

$("btn-plan").addEventListener("click", loadPlan);
$("btn-replan").addEventListener("click", loadPlan);

function targetFolderValue() {
  return $("import-folder").value.trim() || null;
}

async function loadPlan() {
  const dir = await resolveDir();
  if (!dir) {
    setStatus("Indiquez d'abord le dossier d'archive.", "error");
    return;
  }
  try {
    setStatus("Construction du plan…");
    plan = await invoke("import_build_plan", {
      dir,
      decisions: decisionsPayload(),
      targetFolder: targetFolderValue(),
    });
    renderPlan();
    $("plan-card").classList.remove("hidden");
    $("report-card").classList.add("hidden");
    setStatus(`Plan établi : ${plan.length} tâche(s).`, "success");
  } catch (e) {
    showError(e);
  }
}

function actionBadge(item) {
  const map = {
    create: ["badge-create", "Créer"],
    update: ["badge-update", "Mettre à jour"],
    skip_identical: ["badge-skip", "Identique (rien à faire)"],
    skipped: ["badge-skip", "Sautée"],
    conflict: ["badge-conflict", "Conflit"],
    password_required: ["badge-password", "Mot de passe requis"],
    user_unmapped: ["badge-unmapped", "Utilisateur non mappé"],
  };
  const [cls, label] = map[item.actionKind] || ["badge-skip", item.actionLabel];
  return `<span class="badge ${cls}">${escapeHtml(label)}</span>`;
}

function renderPlan() {
  const tbody = $("plan-table").querySelector("tbody");
  tbody.innerHTML = "";
  let pending = 0;

  const folder = targetFolderValue();
  for (const item of plan) {
    const tr = document.createElement("tr");
    tr.appendChild(td(escapeHtml(item.path)));
    const destCell = td(
      folder && item.targetPath && item.targetPath !== item.path
        ? `<span class="dest-path">${escapeHtml(item.targetPath)}</span>`
        : "—"
    );
    tr.appendChild(destCell);
    tr.appendChild(td(actionBadge(item)));

    const userCell = td(escapeHtml(item.targetUser || item.sourceUser || "—"));
    tr.appendChild(userCell);

    const resolveCell = td("");
    const controls = buildResolutionControls(item);
    resolveCell.appendChild(controls);
    tr.appendChild(resolveCell);

    tbody.appendChild(tr);

    if (
      item.actionKind === "conflict" ||
      item.actionKind === "password_required" ||
      item.actionKind === "user_unmapped"
    ) {
      pending++;
    }
  }

  $("plan-note").textContent = folder
    ? `Restauration sous « ${folder} » (structure d'origine préservée). Résolvez chaque tâche puis « Re-planifier » pour actualiser le plan.`
    : "Résolvez chaque tâche puis « Re-planifier » pour actualiser le plan.";
  $("plan-count").textContent = `${plan.length} tâche(s) — ${pending} à résoudre.`;
}

function td(html) {
  const cell = document.createElement("td");
  cell.innerHTML = html;
  return cell;
}

function buildResolutionControls(item) {
  const wrap = document.createElement("div");
  wrap.className = "resolve";

  const addSkipToggle = () => {
    const label = document.createElement("label");
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = skipSet.has(item.path);
    cb.addEventListener("change", () => {
      if (cb.checked) {
        skipSet.add(item.path);
      } else {
        skipSet.delete(item.path);
      }
      setStatus(`Tâche « ${item.path} » ${cb.checked ? "ajoutée aux" : "retirée des"} sauts.`);
    });
    label.appendChild(cb);
    label.appendChild(document.createTextNode("Sauter"));
    wrap.appendChild(label);
  };

  switch (item.actionKind) {
    case "conflict": {
      const sel = document.createElement("select");
      const opts = [
        ["skip", "Conserver l'existante (recommandé)"],
        ["overwrite", "Écraser par la version importée"],
        ["", "Laisser en attente"],
      ];
      for (const [value, label] of opts) {
        const o = document.createElement("option");
        o.value = value;
        o.textContent = label;
        sel.appendChild(o);
      }
      sel.value = decisions.conflicts[item.path] !== undefined ? decisions.conflicts[item.path] : "skip";
      sel.addEventListener("change", () => {
        if (sel.value === "") {
          delete decisions.conflicts[item.path];
        } else {
          decisions.conflicts[item.path] = sel.value;
        }
        setStatus(`Conflit « ${item.path} » : ${sel.options[sel.selectedIndex].text}.`);
      });
      wrap.appendChild(sel);
      break;
    }
    case "user_unmapped": {
      const input = document.createElement("input");
      input.type = "text";
      input.placeholder = `Compte cible pour ${item.sourceUser}`;
      const mapped = decisions.userMap[item.source_user];
      if (mapped) input.value = mapped;
      input.addEventListener("change", () => {
        const v = input.value.trim();
        if (v) {
          decisions.userMap[item.source_user] = v;
          setStatus(`« ${item.source_user} » mappé vers « ${v} ».`);
        } else {
          delete decisions.userMap[item.source_user];
          setStatus(`Mapping retiré pour « ${item.source_user} ».`);
        }
      });
      wrap.appendChild(input);
      addSkipToggle();
      break;
    }
    case "password_required": {
      const input = document.createElement("input");
      input.type = "password";
      input.placeholder = `Mot de passe pour ${item.targetUser}`;
      input.addEventListener("change", async () => {
        try {
          await invoke("import_set_password", { user: item.targetUser, password: input.value });
          setStatus(input.value ? "Mot de passe mémorisé en mémoire (jamais journalisé)." : "Mot de passe effacé.");
        } catch (e) {
          showError(e);
        }
      });
      wrap.appendChild(input);
      addSkipToggle();
      break;
    }
    case "create":
    case "update":
      addSkipToggle();
      break;
    default:
      wrap.appendChild(document.createTextNode("—"));
  }
  return wrap;
}

// ---------------------------------------------------------------------------
// Onglet Import — exécution
// ---------------------------------------------------------------------------

$("btn-dryrun").addEventListener("click", () => runImport(true));
$("btn-import").addEventListener("click", () => runImport(false));

async function runImport(dryRun) {
  const dir = await resolveDir();
  if (!dir) {
    setStatus("Indiquez d'abord le dossier d'archive.", "error");
    return;
  }
  try {
    // Import réel sans admin : le backend lance un processus administrateur
    // temporaire (invite UAC) — l'interface reste ouverte.
    setStatus(dryRun ? "Simulation en cours…" : "Import en cours… (demande d'élévation UAC le cas échéant)");
    const report = await invoke("import_execute", {
      dir,
      decisions: decisionsPayload(),
      dryRun,
      targetFolder: targetFolderValue(),
    });
    renderReport(report, dryRun);
    setStatus(
      dryRun
        ? `Simulation terminée : ${report.created.length} création(s), ${report.blocked.length} bloquée(s), ${report.failed.length} échec(s).`
        : `Import terminé : ${report.created.length} création(s), ${report.updated.length} mise(s) à jour, ${report.failed.length} échec(s).`,
      report.failed.length === 0 ? "success" : "error"
    );
  } catch (e) {
    showError(e);
  }
}

function renderReport(report, dryRun) {
  const box = $("report-content");
  box.innerHTML = "";
  $("report-card").classList.remove("hidden");

  const title = document.createElement("p");
  title.className = "report-line";
  title.textContent = dryRun
    ? "Résultat de la simulation (rien n'a été écrit)."
    : "Résultat de l'import.";
  box.appendChild(title);

  const sections = [
    ["Créées", report.created, "report-ok"],
    ["Mises à jour", report.updated, "report-ok"],
    ["Ignorées", report.skipped, ""],
    ["Bloquées", report.blocked.map(([p, r]) => `${p} — ${r}`), "report-warn"],
    ["Échecs", report.failed.map(([p, r]) => `${p} — ${r}`), "report-err"],
  ];

  for (const [label, items, cls] of sections) {
    if (items.length === 0) continue;
    const block = document.createElement("div");
    block.className = "report-block";
    const h = document.createElement("h3");
    h.textContent = `${label} (${items.length})`;
    block.appendChild(h);
    const ul = document.createElement("ul");
    for (const item of items) {
      const li = document.createElement("li");
      li.className = cls;
      li.textContent = item;
      ul.appendChild(li);
    }
    block.appendChild(ul);
    box.appendChild(block);
  }

  const exit = document.createElement("p");
  exit.className = "report-line";
  const code = report.exitCode;
  exit.textContent =
    code === 0
      ? "Code de sortie : 0 — tout a réussi."
      : code === 1
        ? "Code de sortie : 1 — des tâches restent bloquées ou en échec partiel."
        : "Code de sortie : 2 — échec complet.";
  exit.className = "report-line " + (code === 0 ? "report-ok" : "report-err");
  box.appendChild(exit);
}

$("btn-clear-pw").addEventListener("click", async () => {
  try {
    await invoke("import_clear_passwords");
    setStatus("Mots de passe mémorisés effacés.", "success");
  } catch (e) {
    showError(e);
  }
});

// ---------------------------------------------------------------------------
// Onglet Logs
// ---------------------------------------------------------------------------

let logSeq = -1; // curseur : dernière séquence connue (voir pollLogs)

function appendLogLine(line) {
  const view = $("log-view");
  const div = document.createElement("div");
  div.className = "log-line-" + (line.level || "info").toLowerCase();
  div.textContent = `${line.timestamp} [${line.level}] ${line.message}`;
  view.appendChild(div);
  view.scrollTop = view.scrollHeight;
}

async function pollLogs() {
  try {
    const page = await invoke("get_logs", { afterSeq: logSeq + 1 });
    for (const line of page.lines) {
      appendLogLine(line);
    }
    logSeq = page.lastSeq;
    if (!$("log-dir-label").textContent) {
      $("log-dir-label").textContent = "Logs : " + (await invoke("log_dir"));
    }
  } catch (e) {
    /* silencieux : le poll ne doit jamais casser l'interface */
  }
}

$("btn-open-logs").addEventListener("click", async () => {
  try {
    await invoke("open_log_folder");
  } catch (e) {
    showError(e);
  }
});

// ---------------------------------------------------------------------------
// Démarrage
// ---------------------------------------------------------------------------

(async function init() {
  if (!TAURI) {
    setStatus("Task backup and restore doit être exécuté depuis l'application (pas dans un navigateur).", "error");
    return;
  }
  try {
    await initElevation();
    setInterval(pollLogs, 1000);
    pollLogs();
  } catch (e) {
    showError(e);
  }
})();