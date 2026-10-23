//! Passive original received-occurrence witness. Never acquires or publishes.
use super::*;
use crate::{
    canvas::quip_titles,
    profile_edit::sections::rich::search::file_source::FileObservation,
    slack_lists::comments::{self, file_navigation},
};
use super_platinum_core::slack::api::custom::texty::file_titles::QuipTitle;

pub(crate) struct Transcript {
    source: file_navigation::Source,
    generation: u64,
    socket: u64,
    team: String,
    ts: String,
    rows: Vec<Row>,
}
struct Row {
    raw: Value,
    token: file_navigation::Token,
    file: FileObservation,
    quip: Option<quip_titles::Observation>,
}
impl Transcript {
    pub(crate) fn capture(shell: &ShellState, message: &Value) -> Result<Self, Error> {
        if !named() || !super::super::enabled(shell) {
            return Err(refused());
        }
        let view = comments::presentation(shell).ok_or_else(refused)?;
        let ts = message["ts"].as_str().ok_or_else(refused)?;
        let mut matches = view.rows.iter().filter(|row| row.ts == ts);
        let row = matches.next().ok_or_else(refused)?;
        if matches.next().is_some() {
            return Err(refused());
        }
        let source =
            file_navigation::Source::capture(shell, view.generation, row).ok_or_else(refused)?;
        let team = shell.core.active_team.as_deref().ok_or_else(refused)?;
        let socket = shell
            .core
            .workspaces
            .get(team)
            .ok_or_else(refused)?
            .rt_generation;
        // Fixture messages use the original rich block/section element grammar.
        // Source::occurrence independently matches each proposed raw/ordinal
        // against the actual accepted MessageVm traversal before granting Token.
        let mut raw = Vec::new();
        collect(&message["blocks"], &mut raw);
        if raw.len() != 2 || raw[0]["file_id"] != files::CANVAS || raw[1]["file_id"] != files::LIST
        {
            return Err(refused());
        }
        let rows = raw
            .into_iter()
            .enumerate()
            .map(|(ordinal, raw)| {
                let id = raw["file_id"].as_str().ok_or_else(refused)?;
                let token = source
                    .occurrence(shell, team, raw, ordinal)
                    .ok_or_else(refused)?;
                let file = FileObservation::capture(shell, team, id)?;
                let quip = if raw["type"] == "canvas" {
                    Some(quip_titles::Observation::capture(shell, id).ok_or_else(refused)?)
                } else {
                    None
                };
                Ok(Row {
                    raw: raw.clone(),
                    token,
                    file,
                    quip,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let witness = Self {
            source,
            generation: view.generation,
            socket,
            team: team.into(),
            ts: ts.into(),
            rows,
        };
        witness
            .current(shell)
            .then_some(witness)
            .ok_or_else(refused)
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        named()
            && self.source.current(shell)
            && self.rows.iter().all(|row| {
                row.token.current(shell)
                    && row.file.current(shell)
                    && row.quip.as_ref().is_none_or(|q| q.current(shell))
            })
    }

    pub(crate) fn snapshot(&self, shell: &ShellState) -> Value {
        json!({"generation":self.generation,"socket":self.socket,"team":self.team,"ts":self.ts,
            "current":self.current(shell),"sourceCurrent":self.source.current(shell),
            "rows":self.rows.iter().enumerate().map(|(ordinal,row)|{
                let quip=row.quip.as_ref().map(|q|{
                    let mut proof=json!({"fileId":row.file.id(),"current":q.current(shell)});
                    match q.value() {
                        QuipTitle::Nullish=>proof["value"]=json!("nullish"),
                        QuipTitle::Text(text)=>{proof["value"]=json!("text");proof["text"]=json!(text);},
                        QuipTitle::Unavailable=>proof["value"]=json!("unavailable"),
                    }
                    proof
                });
                json!({"ordinal":ordinal,"raw":row.raw,"current":row.token.current(shell),"label":row.token.label(),
                    "file":{"id":row.file.id(),"current":row.file.current(shell),"source":row.file.source(),
                        "effective":row.file.effective(),"receipt":row.file.receipt()},"quipTitle":quip})
            }).collect::<Vec<_>>()})
    }
}

fn collect<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    if let Some(values) = value.as_array() {
        for value in values {
            collect(value, out);
        }
    } else if matches!(value["type"].as_str(), Some("canvas" | "file")) {
        out.push(value);
    } else if let Some(values) = value.get("elements") {
        collect(values, out);
    }
}
