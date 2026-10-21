//! File strings join the same member/group normalization and collation batch.
use super::channel_host::Host;
use super::{Search, file_source, render};
use crate::state::ShellState;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use super_platinum_core::slack::{
    Error,
    api::custom::texty::{file_matching, file_title_source, ranking},
};

fn unavailable() -> Error {
    Error::Api("profile_file_renderer_unavailable".into())
}
fn text(value: &Value) -> Result<&str, Error> {
    match value {
        Value::Null => Ok(""),
        Value::String(s) => Ok(s),
        _ => Err(unavailable()),
    }
}
pub(super) struct Observed {
    source: Arc<file_source::Snapshot>,
    has_link: bool,
}
impl Observed {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
    }
    pub(super) fn has_link(&self) -> bool {
        self.has_link
    }
    pub(super) fn names(
        &self,
        row: &file_title_source::CandidateObservation,
        normalize: impl Fn(&str) -> Result<String, ranking::Error>,
    ) -> Result<file_matching::Names, ranking::Error> {
        if !self.source.rows().iter().any(|r| {
            r.id() == row.id() && r.source() == row.source() && r.effective() == row.effective()
        }) {
            return Err(ranking::Error::MissingObservation(
                "original file pair".into(),
            ));
        }
        file_matching::Names::observe(row.effective(), normalize)
    }
    pub(super) fn extend_fields(
        &self,
        fields: &mut BTreeSet<String>,
        collation: &mut BTreeSet<String>,
    ) -> Result<(), Error> {
        for row in self.source.rows() {
            extend(row.effective(), fields, collation)?;
        }
        Ok(())
    }
}
fn extend(
    raw: &Value,
    fields: &mut BTreeSet<String>,
    collation: &mut BTreeSet<String>,
) -> Result<(), Error> {
    for key in ["name", "title", "external_url", "permalink"] {
        let value = text(&raw[key])?;
        fields.insert(value.into());
        fields.insert(render::model_query(value).into());
        fields.extend(value.split('\n').map(str::to_owned));
    }
    let title = text(&raw["title"])?;
    collation.insert(
        if title.is_empty() {
            text(&raw["name"])?
        } else {
            title
        }
        .into(),
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    ticket: String,
    query: String,
    locale: String,
    has_link: bool,
}
fn decode(ticket: &str, query: &str, locale: &str, reply: Reply) -> Result<bool, Error> {
    render::same_locale(locale, &reply.locale)?;
    if reply.ticket != ticket || reply.query != query {
        return Err(unavailable());
    }
    Ok(reply.has_link)
}
pub(super) async fn capture(
    search: &Search<impl Host>,
    current: &impl Fn() -> bool,
    ticket: &str,
    locale: &str,
    normalized_query: &str,
    source: Arc<file_source::Snapshot>,
) -> Result<Observed, Error> {
    if !current() || normalized_query.encode_utf16().count() > 32768 {
        return Err(unavailable());
    }
    // v+tG consumes the actual hNDQ query. URL, email and phone existence do
    // not establish a URL authority, insertion label or native caret witness.
    let script = format!(
        "const urls=({})({});const links=({})({},urls);const query={};dioxus.send({{ticket:{},query,locale:Intl.DateTimeFormat().resolvedOptions().locale,has_link:links(query)}});await dioxus.recv();",
        include_str!("../../../../view/profile_edit/sections/rich/trigger.js"),
        include_str!("../../../../view/profile_edit/sections/rich/trigger-data.json"),
        include_str!("file_render/links.js"),
        include_str!("file_render/links-data.json"),
        serde_json::to_string(normalized_query).map_err(|_| unavailable())?,
        serde_json::to_string(ticket).map_err(|_| unavailable())?
    );
    let mut eval = dioxus::document::eval(&script);
    let _release = crate::app_home::inputs::native::retain(eval);
    let result = tokio::select! { biased;
        _ = search.lease.cancelled() => Err(unavailable()),
        result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Reply>()) =>
            result.map_err(|_| unavailable()).and_then(|r| r.map_err(|_| unavailable())),
    };
    if !current() {
        return Err(unavailable());
    }
    Ok(Observed {
        source,
        has_link: decode(ticket, normalized_query, locale, result?)?,
    })
}

#[cfg(test)]
mod tests;
