//! Edition de l'action `Exec` (`Programme/script`, `Ajouter des arguments`,
//! `Commencer dans`) d'un XML de tache avant import.
//!
//! Les champs d'action d'une tache pointent souvent vers des ressources
//! propres a la machine source (lettres de lecteur, partages UNC). Avant la
//! restauration, chaque tache peut donc voir les champs de sa **premiere**
//! action modifies individuellement, sans que l'archive d'origine soit
//! jamais reecrite.
//!
//! La reecriture est un parcours d'evenements quick-xml (Reader + Writer) :
//! seuls les champs cibles du premier bloc `<Exec>` sont modifies, les
//! autres actions et le reste du document sont recopies octet a octet.
//! `build_plan` applique la surcharge une seule fois, au meme endroit pour
//! la simulation et pour l'execution reelle (interface ou processus eleve),
//! ce qui preserve l'exigence « dry-run identique au vrai flux ».

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use serde::{Deserialize, Serialize};

use crate::error::{Result, TsbakError};

/// Surcharge des champs d'action d'une tache avant import.
///
/// Sémantique : un champ `None` est laisse inchange ; un champ renseigne
/// est reecrit (`Some("")` vide l'element, un element absent du XML est
/// cree a sa place dans la sequence du schema). Seule la **premiere**
/// action `Exec` de la tache est concernee.
///
/// Le meme type porte la lecture (depuis l'interface) et l'ecriture (fichier
/// de reponses `action_overrides`, decisions d'import, processus eleve) :
/// les trois flux voient donc exactement la meme structure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOverride {
    /// Programme/script (`<Command>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Ajouter des arguments (`<Arguments>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    /// Commencer dans (`<WorkingDirectory>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

impl ActionOverride {
    /// Aucun champ a reecrire (surcharge vide : le XML reste inchange).
    pub fn is_empty(&self) -> bool {
        Field::ALL
            .iter()
            .all(|f| override_for(self, *f).is_none())
    }
}

/// Champs lus dans la premiere action `Exec` d'un XML de tache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionInfo {
    /// Programme/script, `None` si l'element est absent.
    pub command: Option<String>,
    /// Arguments, `None` si l'element est absent.
    pub arguments: Option<String>,
    /// Dossier de demarrage, `None` si l'element est absent.
    pub working_directory: Option<String>,
    /// Nombre d'actions `Exec` du XML (`0` : aucune action executable,
    /// l'edition est impossible).
    pub exec_count: usize,
}

impl ActionInfo {
    /// Champ correspondant a l'element concerne.
    fn get(&self, f: Field) -> &Option<String> {
        match f {
            Field::Command => &self.command,
            Field::Arguments => &self.arguments,
            Field::WorkingDirectory => &self.working_directory,
        }
    }

    /// Champ a remplir lors de la lecture.
    fn get_mut(&mut self, f: Field) -> &mut Option<String> {
        match f {
            Field::Command => &mut self.command,
            Field::Arguments => &mut self.arguments,
            Field::WorkingDirectory => &mut self.working_directory,
        }
    }
}

/// Un des trois elements modifiables d'une action `Exec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    /// `<Command>` : le programme ou le script lance par la tache.
    Command,
    /// `<Arguments>` : les arguments passes au programme.
    Arguments,
    /// `<WorkingDirectory>` : le dossier de demarrage du programme.
    WorkingDirectory,
}

impl Field {
    /// Les trois champs, dans l'ordre impose par le schema XML.
    const ALL: [Field; 3] = [Field::Command, Field::Arguments, Field::WorkingDirectory];

    /// Nom XML local du champ.
    fn tag(self) -> &'static str {
        match self {
            Field::Command => "Command",
            Field::Arguments => "Arguments",
            Field::WorkingDirectory => "WorkingDirectory",
        }
    }

    /// Rang du champ dans la sequence du schema : l'ordre d'insertion des
    /// elements manquants (0 = `Command`, 1 = `Arguments`, 2 =
    /// `WorkingDirectory`).
    fn order(self) -> usize {
        match self {
            Field::Command => 0,
            Field::Arguments => 1,
            Field::WorkingDirectory => 2,
        }
    }

    /// Reconnait le nom local d'une etiquette, eventuel prefixe XML accepte
    /// (`<t:Command>`).
    fn from_name(name: &[u8]) -> Option<Field> {
        let local = local_name(name);
        Field::ALL.iter().copied().find(|f| f.tag().as_bytes() == local)
    }
}

/// Nom local d'une etiquette : le eventuel prefixe XML est retire
/// (`t:Exec` -> `Exec`).
fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&b| b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

/// Valeur a ecrire pour le champ, si la surcharge le renseigne.
fn override_for(o: &ActionOverride, f: Field) -> Option<&str> {
    match f {
        Field::Command => o.command.as_deref(),
        Field::Arguments => o.arguments.as_deref(),
        Field::WorkingDirectory => o.working_directory.as_deref(),
    }
}

/// L'element existe-t-il dans la premiere action lue ?
fn present(info: &ActionInfo, f: Field) -> bool {
    info.get(f).is_some()
}

/// Erreur XML rattachee a une tache (jamais de chemin ni de valeur
/// sensible dedans : uniquement le chemin de la tache et la raison).
fn invalid_xml(task: &str, reason: impl Into<String>) -> TsbakError {
    TsbakError::InvalidXml {
        task: task.to_string(),
        reason: reason.into(),
    }
}

/// Une etiquette de contenu ne contient que des espacements ?
fn is_whitespace(text: &BytesText) -> bool {
    let bytes: &[u8] = text;
    !bytes.is_empty() && bytes.iter().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
}

/// Balise fermante correspondant a une etiquette (nom local et eventuel
/// prefixe conserves) : permet d'ouvrir un element auto-ferme avant
/// d'ecrire son contenu puis sa fermeture.
fn paired_end(name: &quick_xml::name::QName<'_>) -> BytesEnd<'static> {
    BytesEnd::new(String::from_utf8_lossy(name.as_ref()).into_owned())
}

/// Balise d'ouverture construite a partir d'un element auto-ferme, les
/// espacements de fin (devant `/`) retires pour eviter `<Command >`.
fn start_from_empty(e: &BytesStart<'_>) -> BytesStart<'static> {
    let trimmed = String::from_utf8_lossy(e).trim_end().to_string();
    let name_len = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());
    BytesStart::from_content(trimmed, name_len)
}

/// Lit la premiere action `Exec` d'un XML de tache : valeurs des trois
/// champs modifiables et nombre total d'actions `Exec`.
///
/// Une tache sans action executable (tache `ComHandler`, par exemple)
/// renvoie des champs `None` et `exec_count == 0` : l'edition est alors
/// impossible. Les commandes des actions qui ne sont pas `Exec` ne sont
/// jamais lues ici.
pub fn read_action(xml: &str, task: &str) -> Result<ActionInfo> {
    let mut reader = Reader::from_str(xml);
    let mut info = ActionInfo::default();
    let mut exec_index = 0usize; // nombre d'actions Exec deja ouvertes
    let mut in_exec = false;
    let mut field: Option<Field> = None;
    let mut buf = String::new();

    loop {
        let event = reader
            .read_event()
            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
        match event {
            Event::Eof => break,

            Event::Start(e) => {
                let name = e.name();
                let name = name.as_ref();
                if local_name(name) == b"Exec" {
                    if !in_exec {
                        exec_index += 1;
                        in_exec = true;
                    }
                } else if field.is_none() && in_exec && exec_index == 1 {
                    if let Some(f) = Field::from_name(name) {
                        if info.get(f).is_none() {
                            *info.get_mut(f) = Some(String::new());
                            buf.clear();
                            field = Some(f);
                        }
                    }
                }
            }

            Event::Empty(e) => {
                let name = e.name();
                let name = name.as_ref();
                if local_name(name) == b"Exec" {
                    if !in_exec {
                        exec_index += 1;
                    }
                } else if in_exec && exec_index == 1 && field.is_none() {
                    if let Some(f) = Field::from_name(name) {
                        if info.get(f).is_none() {
                            *info.get_mut(f) = Some(String::new());
                        }
                    }
                }
            }

            Event::End(e) => {
                let name = e.name();
                let name = name.as_ref();
                if let Some(f) = field {
                    if Field::from_name(name) == Some(f) {
                        *info.get_mut(f) = Some(std::mem::take(&mut buf));
                        field = None;
                        continue;
                    }
                }
                if local_name(name) == b"Exec" && in_exec {
                    in_exec = false;
                }
            }

            Event::Text(t) => {
                if field.is_some() {
                    let text = t
                        .unescape()
                        .map_err(|e| invalid_xml(task, format!("texte illisible: {e}")))?;
                    buf.push_str(&text);
                }
            }

            Event::CData(c) => {
                if field.is_some() {
                    let raw: &[u8] = &c;
                    buf.push_str(&String::from_utf8_lossy(raw));
                }
            }

            _ => {}
        }
    }

    info.exec_count = exec_index;
    Ok(info)
}

/// Reecrit la premiere action `Exec` du XML selon `overrides` et renvoie le
/// XML modifie (l'archive d'origine n'est jamais toichee).
///
/// Les elements absents dont un champ est renseigne sont crees a leur place
/// dans la sequence du schema (`<Command>`, `<Arguments>`,
/// `<WorkingDirectory>`). Les autres actions et le reste du document sont
/// recopies octet a octet. Avec une surcharge vide, le XML rendu est
/// strictement identique a l'entree.
///
/// # Erreurs
///
/// [`TsbakError::InvalidXml`] si le XML n'est pas analysable, ou si la
/// tache ne contient aucune action `Exec` a editer.
pub fn apply_action(xml: &str, task: &str, overrides: &ActionOverride) -> Result<String> {
    if overrides.is_empty() {
        return Ok(xml.to_string());
    }

    // Verification prealable : le XML doit etre analysable et comporter une
    // action Exec a editer (la lecture ne porte que sur la premiere).
    let info = read_action(xml, task)?;
    if info.exec_count == 0 {
        return Err(invalid_xml(
            task,
            "aucune action Exec a editer dans cette tache",
        ));
    }

    // Elements absents a creer, deja tries dans l'ordre du schema.
    let mut inserts: Vec<(Field, String)> = Vec::new();
    for f in Field::ALL {
        if let Some(v) = override_for(overrides, f) {
            if !present(&info, f) {
                inserts.push((f, v.to_string()));
            }
        }
    }

    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::new());

    let mut in_exec = false; // premiere action Exec ouverte
    let mut exec_done = false; // premiere action Exec fermee
    let mut field: Option<Field> = None; // champ texte en cours de remplacement
    let mut replaced = [false; 3]; // champ deja reecrit (premiere occurrence)
    let mut child_ws: Option<String> = None; // espacements des enfants de l'Exec
    let mut pending_ws: Option<String> = None; // espacements retenus devant le prochain evenement

    loop {
        let event = reader
            .read_event()
            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;

        if let Event::Eof = event {
            break;
        }

        // Un champ en cours de remplacement avale tous les evenements
        // jusqu'a sa balise fermante, remplacee par la valeur demandee.
        if let Some(f) = field {
            let closes = matches!(
                &event,
                Event::End(e) if Field::from_name(e.name().as_ref()) == Some(f)
            );
            if !closes {
                continue;
            }
            let value = override_for(overrides, f).unwrap_or_default();
            writer
                .write_event(Event::Text(BytesText::new(value)))
                .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
            if let Event::End(e) = event {
                writer
                    .write_event(Event::End(e))
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
            }
            field = None;
            continue;
        }

        match event {
            Event::Start(e) => {
                let name = e.name();
                let opens_first_exec =
                    !in_exec && !exec_done && local_name(name.as_ref()) == b"Exec";

                if opens_first_exec {
                    in_exec = true;
                    writer
                        .write_event(Event::Start(e))
                        .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                    continue;
                }

                if in_exec {
                    if let Some(f) = Field::from_name(name.as_ref()) {
                        // Elements manquants de rang inferieur, puis les
                        // espacements retenus, puis l'element courant.
                        flush_inserts(&mut writer, &mut inserts, Some(f.order()), &child_ws)
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                        flush_ws(&mut writer, &mut pending_ws)
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                        let value = if replaced[f.order()] {
                            None
                        } else {
                            override_for(overrides, f)
                        };
                        writer
                            .write_event(Event::Start(e))
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                        if value.is_some() {
                            replaced[f.order()] = true;
                            field = Some(f);
                        }
                        continue;
                    }
                }

                flush_ws(&mut writer, &mut pending_ws)
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                writer
                    .write_event(Event::Start(e))
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
            }

            Event::Empty(e) => {
                let name = e.name();

                // Une action vide `<Exec />` est ouverte pour recevoir les
                // elements crees par la surcharge.
                if !in_exec && !exec_done && local_name(name.as_ref()) == b"Exec" {
                    exec_done = true;
                    if inserts.is_empty() {
                        writer
                            .write_event(Event::Empty(e))
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                    } else {
                        let end = paired_end(&name);
                        writer
                            .write_event(Event::Start(start_from_empty(&e)))
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                        flush_inserts(&mut writer, &mut inserts, None, &child_ws)
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                        writer
                            .write_event(Event::End(end))
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                    }
                    continue;
                }

                if in_exec {
                    if let Some(f) = Field::from_name(name.as_ref()) {
                        flush_inserts(&mut writer, &mut inserts, Some(f.order()), &child_ws)
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                        flush_ws(&mut writer, &mut pending_ws)
                            .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                        let value = if replaced[f.order()] {
                            None
                        } else {
                            override_for(overrides, f)
                        };
                        match value {
                            Some(v) => {
                                replaced[f.order()] = true;
                                let end = paired_end(&name);
                                writer
                                    .write_event(Event::Start(start_from_empty(&e)))
                                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                                writer
                                    .write_event(Event::Text(BytesText::new(v)))
                                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                                writer
                                    .write_event(Event::End(end))
                                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                            }
                            None => {
                                writer
                                    .write_event(Event::Empty(e))
                                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                            }
                        }
                        continue;
                    }
                }

                flush_ws(&mut writer, &mut pending_ws)
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                writer
                    .write_event(Event::Empty(e))
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
            }

            Event::End(e) => {
                let closes_first_exec = in_exec && local_name(e.name().as_ref()) == b"Exec";
                if closes_first_exec {
                    // Fin de la premiere action : derniers elements crees,
                    // puis les espacements precedant la balise fermante.
                    flush_inserts(&mut writer, &mut inserts, None, &child_ws)
                        .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                    flush_ws(&mut writer, &mut pending_ws)
                        .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                    writer
                        .write_event(Event::End(e))
                        .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                    in_exec = false;
                    exec_done = true;
                    continue;
                }

                flush_ws(&mut writer, &mut pending_ws)
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                writer
                    .write_event(Event::End(e))
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
            }

            Event::Text(t) => {
                if in_exec && is_whitespace(&t) {
                    // Espacements entre enfants : retenus pour laisser la
                    // place aux elements crees juste avant la fermeture.
                    let ws = t
                        .unescape()
                        .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                    if child_ws.is_none() {
                        child_ws = Some(ws.clone().into_owned());
                    }
                    pending_ws = Some(ws.into_owned());
                    continue;
                }
                flush_ws(&mut writer, &mut pending_ws)
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                writer
                    .write_event(Event::Text(t))
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
            }

            other => {
                flush_ws(&mut writer, &mut pending_ws)
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
                writer
                    .write_event(other)
                    .map_err(|e| invalid_xml(task, format!("XML illisible: {e}")))?;
            }
        }
    }

    // Un XML bien forme ferme la premiere action avant la fin du flux.
    if field.is_some() || in_exec {
        return Err(invalid_xml(task, "XML incomplet (balise non fermee)"));
    }

    let bytes = writer.into_inner();
    String::from_utf8(bytes).map_err(|e| invalid_xml(task, format!("XML reecrit illisible: {e}")))
}

/// Ecrit les elements encore a creer dont le rang est strictement inferieur
/// a `before` (tous si `before` est `None`), en reutilisant l'indentation
/// des enfants de l'action.
fn flush_inserts<W: std::io::Write>(
    writer: &mut Writer<W>,
    inserts: &mut Vec<(Field, String)>,
    before: Option<usize>,
    child_ws: &Option<String>,
) -> quick_xml::Result<()> {
    while let Some(rank) = inserts.first().map(|entry| entry.0.order()) {
        if before.is_some_and(|b| rank >= b) {
            break;
        }
        let (f, value) = inserts.remove(0);
        if let Some(ws) = child_ws {
            writer.write_event(Event::Text(BytesText::from_escaped(ws.clone())))?;
        }
        writer.write_event(Event::Start(BytesStart::new(f.tag())))?;
        writer.write_event(Event::Text(BytesText::new(&value)))?;
        writer.write_event(Event::End(BytesEnd::new(f.tag())))?;
    }
    Ok(())
}

/// Ecrit les espacements retenus devant le prochain evenement (s'il y en a).
fn flush_ws<W: std::io::Write>(
    writer: &mut Writer<W>,
    pending: &mut Option<String>,
) -> quick_xml::Result<()> {
    if let Some(ws) = pending.take() {
        writer.write_event(Event::Text(BytesText::from_escaped(ws)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASK: &str = "\\Backup\\Nightly";

    /// XML d'une tache tel qu'exporte (indentation, UTF-16 annonce).
    fn sample() -> String {
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <LogonTrigger />
  </Triggers>
  <Actions Context="Author">
    <Exec>
      <Command>C:\Program Files (x86)\Nightly\job.exe</Command>
      <Arguments>--serveur SRV01 --verbose</Arguments>
      <WorkingDirectory>\\srv-fic\ComputerMonitoring</WorkingDirectory>
    </Exec>
  </Actions>
</Task>"#
            .to_string()
    }

    fn parse_check(xml: &str) {
        let mut reader = Reader::from_str(xml);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("XML reecrit non analysable: {e}\n{xml}"),
            }
        }
    }

    #[test]
    fn lit_les_champs_de_la_premiere_action() {
        let info = read_action(&sample(), TASK).unwrap();
        assert_eq!(info.command.as_deref(), Some(r"C:\Program Files (x86)\Nightly\job.exe"));
        assert_eq!(info.arguments.as_deref(), Some("--serveur SRV01 --verbose"));
        assert_eq!(info.working_directory.as_deref(), Some(r"\\srv-fic\ComputerMonitoring"));
        assert_eq!(info.exec_count, 1);
    }

    #[test]
    fn compte_les_actions_exec_sans_compter_les_comhandler() {
        let xml = r#"<Task><Actions><ComHandler><Command>{clsid}</Command></ComHandler><Exec><Command>a.exe</Command></Exec><Exec><Command>b.exe</Command></Exec></Actions></Task>"#;
        let info = read_action(xml, TASK).unwrap();
        assert_eq!(info.exec_count, 2, "le ComHandler n'est pas une action Exec");
        assert_eq!(info.command.as_deref(), Some("a.exe"), "la premiere action Exec est lue");
    }

    #[test]
    fn sans_action_exec_les_champs_sont_absents() {
        let xml = r#"<Task><Actions><ComHandler><Command>{clsid}</Command></ComHandler></Actions></Task>"#;
        let info = read_action(xml, TASK).unwrap();
        assert_eq!(info.exec_count, 0);
        assert_eq!(info.command, None);
        assert_eq!(info.arguments, None);
        assert_eq!(info.working_directory, None);
    }

    #[test]
    fn elements_vides_lus_comme_valeurs_vides() {
        let xml = r#"<Task><Actions><Exec><Command></Command><Arguments /></Exec></Actions></Task>"#;
        let info = read_action(xml, TASK).unwrap();
        assert_eq!(info.command.as_deref(), Some(""));
        assert_eq!(info.arguments.as_deref(), Some(""));
        assert_eq!(info.working_directory, None);
    }

    #[test]
    fn surcharge_vide_ne_change_pas_le_xml() {
        let xml = sample();
        let out = apply_action(&xml, TASK, &ActionOverride::default()).unwrap();
        assert_eq!(out, xml, "une surcharge vide rend le XML inchange");
        // Meme sans aucune action Exec, une surcharge vide reste inoffensive.
        let com = r#"<Task><Actions><ComHandler><Command>{c}</Command></ComHandler></Actions></Task>"#;
        assert_eq!(apply_action(com, TASK, &ActionOverride::default()).unwrap(), com);
    }

    #[test]
    fn remplace_le_programme_sans_toucher_aux_autres_champs() {
        let xml = sample();
        let overrides = ActionOverride {
            command: Some(r"D:\Scripts\job.exe".to_string()),
            ..Default::default()
        };
        let out = apply_action(&xml, TASK, &overrides).unwrap();
        let expected = sample().replace(
            r"C:\Program Files (x86)\Nightly\job.exe",
            r"D:\Scripts\job.exe",
        );
        assert_eq!(out, expected, "seul le <Command> change, octet par octet");
        let info = read_action(&out, TASK).unwrap();
        assert_eq!(info.command.as_deref(), Some(r"D:\Scripts\job.exe"));
        assert_eq!(info.arguments.as_deref(), Some("--serveur SRV01 --verbose"));
        assert_eq!(
            info.working_directory.as_deref(),
            Some(r"\\srv-fic\ComputerMonitoring")
        );
    }

    #[test]
    fn vide_un_champ_existant() {
        let xml = sample();
        let overrides = ActionOverride {
            working_directory: Some(String::new()),
            ..Default::default()
        };
        let out = apply_action(&xml, TASK, &overrides).unwrap();
        let info = read_action(&out, TASK).unwrap();
        assert_eq!(info.working_directory.as_deref(), Some(""));
        assert!(out.contains("<WorkingDirectory></WorkingDirectory>"));
        parse_check(&out);
    }

    #[test]
    fn cree_working_directory_absent_avec_indentation() {
        let xml = r#"<Task><Actions>
    <Exec>
      <Command>job.exe</Command>
      <Arguments>-a</Arguments>
    </Exec>
  </Actions></Task>"#;
        let overrides = ActionOverride {
            working_directory: Some(r"D:\Scripts".to_string()),
            ..Default::default()
        };
        let out = apply_action(xml, TASK, &overrides).unwrap();
        let expected = r#"<Task><Actions>
    <Exec>
      <Command>job.exe</Command>
      <Arguments>-a</Arguments>
      <WorkingDirectory>D:\Scripts</WorkingDirectory>
    </Exec>
  </Actions></Task>"#;
        assert_eq!(out, expected, "l'element cree reprend l'indentation des enfants");
        parse_check(&out);
    }

    #[test]
    fn insere_arguments_avant_working_directory_existant() {
        // Le schema impose l'ordre Command, Arguments, WorkingDirectory :
        // l'element cree ne doit pas finir apres WorkingDirectory.
        let xml = r#"<Task><Actions><Exec><Command>job.exe</Command><WorkingDirectory>D:\old</WorkingDirectory></Exec></Actions></Task>"#;
        let overrides = ActionOverride {
            arguments: Some("-c".to_string()),
            ..Default::default()
        };
        let out = apply_action(xml, TASK, &overrides).unwrap();
        let expected = r#"<Task><Actions><Exec><Command>job.exe</Command><Arguments>-c</Arguments><WorkingDirectory>D:\old</WorkingDirectory></Exec></Actions></Task>"#;
        assert_eq!(out, expected);
        parse_check(&out);
    }

    #[test]
    fn cree_les_trois_elements_dans_un_exec_vide() {
        let xml = r#"<Task><Actions><Exec /></Actions></Task>"#;
        let overrides = ActionOverride {
            command: Some("job.exe".to_string()),
            arguments: Some("-x".to_string()),
            working_directory: Some(r"D:\w".to_string()),
            ..Default::default()
        };
        let out = apply_action(xml, TASK, &overrides).unwrap();
        let expected = r#"<Task><Actions><Exec><Command>job.exe</Command><Arguments>-x</Arguments><WorkingDirectory>D:\w</WorkingDirectory></Exec></Actions></Task>"#;
        assert_eq!(out, expected);
        parse_check(&out);
    }

    #[test]
    fn echappe_les_caracteres_speciaux_des_valeurs() {
        let xml = sample();
        let value = r#"a && b <c> "d" 'e'"#;
        let overrides = ActionOverride {
            arguments: Some(value.to_string()),
            ..Default::default()
        };
        let out = apply_action(&xml, TASK, &overrides).unwrap();
        assert!(!out.contains("a && b"), "le texte brut ne doit pas etre insere");
        assert!(out.contains("&amp;&amp;"), "esperluettes echappees");
        parse_check(&out);
        let info = read_action(&out, TASK).unwrap();
        assert_eq!(info.arguments.as_deref(), Some(value), "aller-retour sans perte");
    }

    #[test]
    fn ne_modifie_que_la_premiere_action() {
        let xml = r#"<Task><Actions><Exec><Command>old-1</Command></Exec><Exec><Command>old-2</Command></Exec></Actions></Task>"#;
        let overrides = ActionOverride {
            command: Some("new".to_string()),
            ..Default::default()
        };
        let out = apply_action(xml, TASK, &overrides).unwrap();
        assert_eq!(
            out,
            r#"<Task><Actions><Exec><Command>new</Command></Exec><Exec><Command>old-2</Command></Exec></Actions></Task>"#,
            "la deuxieme action reste intacte"
        );
        let info = read_action(&out, TASK).unwrap();
        assert_eq!(info.exec_count, 2);
    }

    #[test]
    fn remplace_une_action_exec_auto_fermante() {
        let xml = r#"<Task><Actions><Exec><Command /></Exec></Actions></Task>"#;
        let overrides = ActionOverride {
            command: Some("job.exe".to_string()),
            ..Default::default()
        };
        let out = apply_action(xml, TASK, &overrides).unwrap();
        assert_eq!(
            out,
            r#"<Task><Actions><Exec><Command>job.exe</Command></Exec></Actions></Task>"#
        );
        parse_check(&out);
    }

    #[test]
    fn sans_action_exec_une_surcharge_active_erre() {
        let xml = r#"<Task><Actions><ComHandler><Command>{c}</Command></ComHandler></Actions></Task>"#;
        let overrides = ActionOverride {
            command: Some("job.exe".to_string()),
            ..Default::default()
        };
        let err = apply_action(xml, TASK, &overrides).unwrap_err();
        assert!(
            err.to_string().contains("aucune action Exec"),
            "message clair: {err}"
        );
        assert!(err.to_string().contains(TASK), "la tache est nommee: {err}");
    }

    #[test]
    fn tolerate_un_prefixe_xml_sur_les_etiquettes() {
        let xml = r#"<Task><t:Actions><t:Exec><t:Command>old</t:Command></t:Exec></t:Actions></Task>"#;
        let info = read_action(xml, TASK).unwrap();
        assert_eq!(info.exec_count, 1);
        assert_eq!(info.command.as_deref(), Some("old"));
        let overrides = ActionOverride {
            command: Some("new".to_string()),
            ..Default::default()
        };
        let out = apply_action(xml, TASK, &overrides).unwrap();
        assert_eq!(
            out,
            r#"<Task><t:Actions><t:Exec><t:Command>new</t:Command></t:Exec></t:Actions></Task>"#
        );
    }

    #[test]
    fn la_serialisation_ne_conserve_que_les_champs_renseignes() {
        let overrides = ActionOverride {
            command: Some("job.exe".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&overrides).unwrap();
        assert_eq!(json, r#"{"command":"job.exe"}"#);
        let parsed: ActionOverride =
            serde_json::from_str(r#"{"command":"job.exe","workingDirectory":"D:\\w"}"#).unwrap();
        assert_eq!(parsed.command.as_deref(), Some("job.exe"));
        assert_eq!(parsed.working_directory.as_deref(), Some(r"D:\w"));
        assert_eq!(parsed.arguments, None);
        assert!(ActionOverride::default().is_empty());
        assert!(!overrides.is_empty());
    }
}
