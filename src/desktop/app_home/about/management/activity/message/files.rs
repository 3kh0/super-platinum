//! Original file observations for Activity StepText. No workflow draft mutation.
use super::super::{document, document_mut, next};
use super::{
    navigation::{self, Location, Source},
    *,
};
use crate::{
    directory_member_fetches::Context,
    profile_edit::sections::rich::search::file_source::{FileInfoRead, FileObservation},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use super_platinum_core::slack::{
    Error, api, api::custom::texty::file_title_source::OrderedFileInfo,
};
mod acquire;
mod open;
mod policy;
mod title;
pub(super) use policy::Label;
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app_home::about::management::activity::message) enum Kind {
    Canvas,
    List,
}
impl Kind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Canvas => "canvas",
            Self::List => "list",
        }
    }
    fn unavailable(self) -> &'static str {
        match self {
            Self::Canvas => "Canvas unavailable",
            Self::List => "List unavailable",
        }
    }
    fn icon(self) -> Icon {
        match self {
            Self::Canvas => Icon::Description,
            Self::List => Icon::TableRows,
        }
    }
}
pub(super) fn valid_id(id: &str) -> bool {
    (2..=255).contains(&id.len())
        && id.starts_with('F')
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
#[derive(Clone, Default)]
pub(in crate::app_home::about::management::activity) struct State {
    detail: u64,
    entries: BTreeMap<String, Entry>,
    pending: Option<(String, u64)>,
    navigation: Option<Arc<open::Attempt>>,
    last_navigation: Option<Value>,
}
#[derive(Clone)]
struct Entry {
    phase: &'static str,
    source: Source,
    receipt: Option<Value>,
    notice: Option<&'static str>,
}
fn state(s: &ShellState, visit: u64) -> Option<&State> {
    Some(&document(s, visit)?.activity.files)
}
fn state_mut(s: &mut ShellState, visit: u64) -> Option<&mut State> {
    Some(&mut document_mut(s, visit)?.activity.files)
}
pub(in crate::app_home::about::management::activity) use acquire::request;
pub(super) fn project(s: &ShellState, team: &str, id: &str, kind: Kind) -> Option<policy::Label> {
    let observed = FileObservation::capture(s, team, id).ok()?;
    // Raw special icon variants are not part of this ordinary entity slice.
    if policy::truth(&observed.source()["product_icon"]) {
        return None;
    }
    policy::label(observed.effective(), kind)
}
pub(super) fn render(
    state: Signal<ShellState>,
    visit: u64,
    location: Location,
    part: usize,
    team: &str,
    id: String,
    kind: Kind,
    label: Option<policy::Label>,
    epoch: u64,
) -> Element {
    let source = Source::capture_file(&state.peek(), visit, location, part, &id, kind);
    let name = label
        .as_ref()
        .map(|v| v.text.clone())
        .unwrap_or_else(|| kind.unavailable().into());
    let painted = title::Painted::project(&state.peek(), team, &name, epoch);
    let content = rsx! { {crate::icons::icon(kind.icon(),"icon sm")} {painted.render(state)} };
    let action = source.as_ref().and_then(|source| {
        if kind != Kind::List {
            return None;
        }
        let label = label.as_ref()?;
        let url = label.url.clone()?;
        let observation = FileObservation::capture(&state.peek(), source.team(), &id).ok()?;
        Some((source.clone(), observation, url))
    });
    if let Some((source, observation, url)) = action {
        let button = source.button();
        return rsx! {a {id:button,class:"mention-chip workflow-activity-file",href:url.clone(),"data-file":id,"data-file-kind":kind.name(),"data-part":part,
            onclick:move |e|{e.prevent_default();open::start(state,source.clone(),observation.clone(),url.clone(),painted.clone(),part);}, {content}
        }};
    }
    let has_current = source
        .as_ref()
        .is_some_and(|source| FileObservation::capture(&state.peek(), source.team(), &id).is_ok());
    let retry = source.filter(|_| label.is_none()).filter(|_| {
        self::state(&state.peek(), visit).is_some_and(|s| {
            s.pending.is_none()
                && s.navigation.is_none()
                && s.entries
                    .get(&id)
                    .is_some_and(|e| retryable(e.phase, has_current))
        })
    });
    rsx! {span {class:"workflow-activity-file", "data-file":id.clone(),"data-file-kind":kind.name(),"data-part":part,{content}}
        if let Some(source)=retry {button {r#type:"button","data-file-retry":id.clone(),onclick:move |_|acquire::retry(state,source.clone()),"Retry file details"}}
    }
}
pub(in crate::app_home::about::management::activity) fn snapshot(
    s: &ShellState,
    visit: u64,
) -> Value {
    let Some(st) = state(s, visit) else {
        return serde_json::json!({});
    };
    let rows:Vec<_>=st.entries.iter().map(|(id,e)|{
        let obs=FileObservation::capture(s,e.source.team(),id).ok();
        serde_json::json!({"id":id,"phase":e.phase,"current":obs.as_ref().is_some_and(|o|o.current(s)),"source":obs.as_ref().map(FileObservation::source),"effective":obs.as_ref().map(FileObservation::effective),"receipt":obs.as_ref().map(FileObservation::receipt),"response":e.receipt,"notice":e.notice})
    }).collect();
    serde_json::json!({"detail":st.detail,"entries":rows,"pendingFile":st.pending.as_ref().map(|(id,_)|id),"navigating":st.navigation.as_ref().map(|a|a.id()),"lastNavigation":st.last_navigation})
}

fn retryable(phase: &str, current: bool) -> bool {
    matches!(phase, "unavailable" | "retired") || (phase == "ready" && !current)
}

pub(in crate::app_home::about::management::activity) fn notice(
    s: &ShellState,
    visit: u64,
) -> Option<&'static str> {
    let st = state(s, visit)?;
    let detail = document(s, visit)?.activity.detail_revision;
    if st.detail != detail {
        return None;
    }
    if st.pending.is_some() {
        return Some("Loading file details\u{2026}");
    }
    st.entries
        .values()
        .filter(|e| e.source.current(s))
        .find_map(|e| e.notice)
}
