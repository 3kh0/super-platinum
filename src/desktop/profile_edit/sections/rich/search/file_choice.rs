//! Private whole-file insertion from an already-ranked, eligible source row.
//! 814n requires an observed team; I83v uses the actual ordinary permalink;
//! 8J14 exports URL-or-ID text, never the displayed canonical title.
use super::{Search, channel_host::Host, file_source, member_names};
use crate::{canvas::quip_titles, state::ShellState};
use serde_json::{Value, json};
use std::sync::Arc;
use super_platinum_core::slack::{
    Error,
    api::custom::{
        RichTextContext,
        texty::{file_title_source, file_titles},
    },
};
mod draft;
pub(crate) use draft::draft_file_labels;
mod target;
pub(crate) use target::FileTarget;
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("profile_file_choice_unavailable".into())
}

pub(in crate::profile_edit::sections::rich) struct Choice {
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    context: Arc<crate::scheduled::Context>,
    names: Arc<member_names::Observation>,
    source: Arc<file_source::Snapshot>,
    projection: Projection,
}

pub(super) fn capture(
    shell: &ShellState,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    source: Arc<file_source::Snapshot>,
    id: &str,
) -> Result<Choice, Error> {
    if !search.current(shell) || !names.current(shell, search) || !source.current(shell) {
        return Err(unavailable());
    }
    // A caller supplies an ID from the shared rank, not an arbitrary raw file.
    let candidate = source
        .rows()
        .iter()
        .find(|row| row.id() == id)
        .ok_or_else(unavailable)?;
    let title = source.title(id)?;
    if title.source() != candidate.source() {
        return Err(unavailable());
    }
    let projection = Projection::capture(
        shell,
        search.lease.team(),
        &search.context.session.user_id,
        &names.gantry,
        title,
        id,
    )?;
    let choice = Choice {
        owner: {
            let lease = search.lease.clone();
            Arc::new(move |shell| lease.current(shell))
        },
        context: search.context.clone(),
        names,
        source,
        projection,
    };
    choice
        .current(shell, search)
        .then_some(choice)
        .ok_or_else(unavailable)
}

impl Choice {
    pub(in crate::profile_edit::sections::rich) fn current(
        &self,
        shell: &ShellState,
        search: &Search<impl Host>,
    ) -> bool {
        (self.owner)(shell)
            && search.current(shell)
            && Arc::ptr_eq(&self.context, &search.context)
            && self.names.current(shell, search)
            && self.source.current(shell)
            && self.projection.current_quip(shell)
    }
    pub(in crate::profile_edit::sections::rich) fn value(&self) -> &Value {
        &self.projection.value
    }
    pub(in crate::profile_edit::sections::rich) fn label(&self) -> &str {
        &self.projection.label
    }
}

struct Projection {
    // Retain the actual title row, not just a typed/rendered label.
    _title: file_title_source::Observation,
    quip: Option<quip_titles::Observation>,
    value: Value,
    label: String,
}
impl Projection {
    fn capture(
        shell: &ShellState,
        team: &str,
        user: &str,
        gantry: &Value,
        title: file_title_source::Observation,
        id: &str,
    ) -> Result<Self, Error> {
        observed_team(gantry, team)?;
        let canonical = CanonicalTitle::capture(shell, &title, id)?;
        let CanonicalTitle { kind, quip, label } = canonical;
        let effective = title.effective();
        let href = match effective.get("permalink") {
            None | Some(Value::Null) => None,
            Some(Value::String(url)) => (!url.is_empty()).then_some(url.as_str()),
            _ => return Err(unavailable()),
        };
        let mut value = json!({
            "type":if kind == file_titles::Kind::Canvas {"canvas"} else {"file"},
            "file_id":id,
            "text":href.unwrap_or(id),
        });
        if let Some(href) = href {
            value["url"] = json!(href);
        }
        let mut context = RichTextContext::new_ordered(team, user)?;
        context.file(&value, label.clone())?;
        Ok(Self {
            _title: title,
            quip,
            value,
            label,
        })
    }
    fn current_quip(&self, shell: &ShellState) -> bool {
        self.quip.as_ref().is_none_or(|q| q.current(shell))
    }
}

/// Shared title projection only. Capturing a new private choice still requires
/// the observed gantry team above; displaying an existing token grants no choice.
struct CanonicalTitle {
    kind: file_titles::Kind,
    quip: Option<quip_titles::Observation>,
    label: String,
}
impl CanonicalTitle {
    fn capture(
        shell: &ShellState,
        title: &file_title_source::Observation,
        id: &str,
    ) -> Result<Self, Error> {
        let effective = title.effective();
        if effective["id"].as_str() != Some(id) || id.is_empty() {
            return Err(unavailable());
        }
        let kind = kind(effective)?;
        let quip = if kind == file_titles::Kind::Canvas
            && effective["title_blocks"]
                .as_array()
                .is_some_and(|blocks| !blocks.is_empty())
        {
            Some(quip_titles::Observation::capture(shell, id).ok_or_else(unavailable)?)
        } else {
            None
        };
        let label = file_titles::label_with_quip(
            kind,
            file_titles::Status::Normal,
            effective,
            quip.as_ref()
                .map_or(file_titles::QuipTitle::Unavailable, |q| q.value()),
        )
        .map_err(|_| unavailable())?;
        Ok(Self { kind, quip, label })
    }
    fn current_quip(&self, shell: &ShellState) -> bool {
        self.quip.as_ref().is_none_or(|q| q.current(shell))
    }
}

fn observed_team(gantry: &Value, team: &str) -> Result<(), Error> {
    if team.is_empty() || gantry["ok"] != true || gantry.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(unavailable());
    }
    let rows = gantry["workspaces"].as_array().ok_or_else(unavailable)?;
    let mut matching = rows.iter().filter(|row| row["id"].as_str() == Some(team));
    if matching.next().is_none_or(|row| !row.is_object()) || matching.next().is_some() {
        return Err(unavailable());
    }
    Ok(())
}
fn kind(row: &Value) -> Result<file_titles::Kind, Error> {
    // Current title-source admission already refuses remote/status variants.
    // Keep this boundary explicit rather than deriving href from url_private.
    if [
        "isUnknown",
        "isNonExistent",
        "is_external",
        "is_deleted",
        "is_skill",
    ]
    .iter()
    .any(|key| truth(&row[*key]))
    {
        return Err(unavailable());
    }
    if row["mode"] == "quip" || row["filetype"] == "quip" {
        Ok(file_titles::Kind::Canvas)
    } else if (row["mode"] == "list" || row["filetype"] == "list") && row["subtype"] != "poll" {
        Ok(file_titles::Kind::WholeList)
    } else {
        Err(unavailable())
    }
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}
