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
mod display;
pub(crate) use display::FileTitles;
mod draft;
pub(super) mod users;
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
    users: Option<Arc<users::Users>>,
}

pub(super) fn capture(
    shell: &ShellState,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    source: Arc<file_source::Snapshot>,
    id: &str,
) -> Result<Choice, Error> {
    capture_with_users(shell, search, names, source, id, None)
}
pub(super) fn capture_with_users(
    shell: &ShellState,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    source: Arc<file_source::Snapshot>,
    id: &str,
    users: Option<Arc<users::Users>>,
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
    let users = users::for_title(shell, title.effective(), users.as_deref())?;
    let projection = Projection::capture_with_context(
        shell,
        search.lease.team(),
        &search.context.session.user_id,
        &names.gantry,
        title,
        id,
        users.as_deref().map(|u| (u, &names.policy)),
        search.lease.canvas_title_locale(),
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
        users,
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
            && self.users.as_ref().is_none_or(|u| u.current(shell))
    }
    pub(in crate::profile_edit::sections::rich) fn title_users_proof(
        &self,
        shell: &ShellState,
    ) -> Option<Value> {
        self.users.as_ref().map(|users| {
            let mut proof = users.snapshot(shell);
            proof["policy"] = self.names.title_policy_proof(shell);
            proof
        })
    }
    pub(in crate::profile_edit::sections::rich) fn canvas_fallback_proof(
        &self,
        shell: &ShellState,
        search: &Search<impl Host>,
    ) -> Option<Value> {
        self.projection.canvas_locale.map(|locale| {
            let mut proof = locale.proof(self.current(shell, search));
            proof["quipTitle"] = self.projection.quip_proof(shell);
            proof
        })
    }
    pub(in crate::profile_edit::sections::rich) fn value(&self) -> &Value {
        &self.projection.value
    }
    pub(in crate::profile_edit::sections::rich) fn label(&self) -> &str {
        &self.projection.label
    }
}

struct Projection {
    canvas_locale: Option<super::super::title_locale::CanvasTitle>,
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
        Self::capture_with_users(shell, team, user, gantry, title, id, None)
    }
    fn capture_with_users(
        shell: &ShellState,
        team: &str,
        user: &str,
        gantry: &Value,
        title: file_title_source::Observation,
        id: &str,
        users: Option<(
            &users::Users,
            &super_platinum_core::block_actions::name_policy::Snapshot,
        )>,
    ) -> Result<Self, Error> {
        Self::capture_with_context(shell, team, user, gantry, title, id, users, None)
    }
    fn capture_with_context(
        shell: &ShellState,
        team: &str,
        user: &str,
        gantry: &Value,
        title: file_title_source::Observation,
        id: &str,
        users: Option<(
            &users::Users,
            &super_platinum_core::block_actions::name_policy::Snapshot,
        )>,
        canvas_locale: Option<super::super::title_locale::CanvasTitle>,
    ) -> Result<Self, Error> {
        observed_team(gantry, team)?;
        let canonical =
            CanonicalTitle::capture_with_context(shell, &title, id, users, canvas_locale)?;
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
            canvas_locale: canvas_locale.filter(|_| kind == file_titles::Kind::Canvas),
            _title: title,
            quip,
            value,
            label,
        })
    }
    fn quip_proof(&self, shell: &ShellState) -> Value {
        let Some(quip) = &self.quip else {
            return Value::Null;
        };
        let mut proof = json!({"fileId":self._title.effective()["id"],
            "current":quip.current(shell)});
        match quip.value() {
            file_titles::QuipTitle::Nullish => proof["value"] = json!("nullish"),
            file_titles::QuipTitle::Text(text) => {
                proof["value"] = json!("text");
                proof["text"] = json!(text);
            }
            file_titles::QuipTitle::Unavailable => proof["value"] = json!("unavailable"),
        }
        proof
    }
    fn current_quip(&self, shell: &ShellState) -> bool {
        self.quip.as_ref().is_none_or(|q| q.current(shell))
    }
}

/// Shared title projection only. Capturing a new private choice still requires
/// the observed gantry team above; displaying an existing token grants no choice.
#[derive(Clone)]
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
        Self::capture_with_users(shell, title, id, None)
    }
    fn capture_with_users(
        shell: &ShellState,
        title: &file_title_source::Observation,
        id: &str,
        users: Option<(
            &users::Users,
            &super_platinum_core::block_actions::name_policy::Snapshot,
        )>,
    ) -> Result<Self, Error> {
        Self::capture_with_context(shell, title, id, users, None)
    }
    fn capture_with_context(
        shell: &ShellState,
        title: &file_title_source::Observation,
        id: &str,
        users: Option<(
            &users::Users,
            &super_platinum_core::block_actions::name_policy::Snapshot,
        )>,
        canvas_locale: Option<super::super::title_locale::CanvasTitle>,
    ) -> Result<Self, Error> {
        let effective = title.effective();
        if effective["id"].as_str() != Some(id) || id.is_empty() {
            return Err(unavailable());
        }
        let kind = kind(effective)?;
        let ids = if kind == file_titles::Kind::Canvas {
            file_titles::user_ids(effective).map_err(|_| unavailable())?
        } else {
            Default::default()
        };
        if !ids.is_empty() && users.is_none_or(|(u, _)| !u.current(shell)) {
            return Err(unavailable());
        }
        let quip = if ids.is_empty()
            && kind == file_titles::Kind::Canvas
            && effective["title_blocks"]
                .as_array()
                .is_some_and(|blocks| !blocks.is_empty())
        {
            Some(quip_titles::Observation::capture(shell, id).ok_or_else(unavailable)?)
        } else {
            None
        };
        let label = file_titles::label_with_context(
            kind,
            file_titles::Status::Normal,
            effective,
            quip.as_ref()
                .map_or(file_titles::QuipTitle::Unavailable, |q| q.value()),
            canvas_locale.map(|locale| locale.value()),
            |id| {
                users
                    .ok_or(file_titles::Unavailable::InvalidMetadata)
                    .and_then(|(users, policy)| users.label(id, policy))
            },
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
