//! One settings transaction, with source-owned target facts independent of text buffers.
use super::*;
use protocol::run_settings::{
    self as domain, Context, Draft, Entity, Kind, Mode, Selection, TargetPolicy,
};
use std::{collections::BTreeMap, sync::Arc};
mod acquire;
mod actions;
mod hydrate;
mod query;
pub(super) mod source;
mod view;
pub(super) use actions::{bind_settings, change, search};
pub(super) use hydrate::start as hydrate;
pub(in crate::app_home::about::management) use view::panel;

#[derive(Clone)]
pub(super) struct Row {
    pub(super) id: String,
    pub(super) kind: Kind,
    pub(super) label: String,
    pub(super) raw: Value,
    pub(super) external: bool,
}
pub(super) struct Settings {
    pub(super) context: Context,
    pub(super) copy: String,
    policy: TargetPolicy,
    pub(super) draft: Option<Draft>,
    source: source::Source,
    pub(super) rows: BTreeMap<String, Row>,
    pub(super) epoch: u64,
    query: String,
    external_query: String,
    pub(super) generation: u64,
    pending: bool,
    pending_mode: Option<Mode>,
    hydration_started: bool,
    results: Vec<String>,
    external_results: Vec<String>,
    receipts: Vec<Value>,
    notice: Option<String>,
}
impl Settings {
    pub(super) fn capture(s: &ShellState, original: &Original) -> Option<Self> {
        let raw = original.editor.run_permissions()?;
        let source = source::Source::capture(s, original)?;
        let (context, rows) = source.context(s, original)?;
        let policy = TargetPolicy::from_acquired(
            original.editor.external_preferences(),
            original.editor.restriction(),
        )
        .ok()?;
        let draft = Draft::from_acquired(raw, &context).ok();
        Some(Self {
            context,
            copy: original.editor.copy_permissions()["permission_type"]
                .as_str()?
                .into(),
            policy,
            draft,
            source,
            rows,
            epoch: next(),
            query: String::new(),
            external_query: String::new(),
            generation: next(),
            pending: false,
            pending_mode: None,
            hydration_started: false,
            results: vec![],
            external_results: vec![],
            receipts: vec![],
            notice: None,
        })
    }
    pub(super) fn source_current(&self, s: &ShellState, original: &Original) -> bool {
        self.source.current(s, original)
    }
    pub(super) fn current(&self, s: &ShellState, original: &Original) -> bool {
        original.current(s, None) && self.source.current(s, original)
    }
    pub(super) fn expected(&self) -> Value {
        json!({"epoch":self.epoch,"copy":self.copy,"mode":self.display_mode(),
            "ids":self.draft.as_ref().map(|d|d.ids()).unwrap_or_default(),
            "visibleIds":self.draft.as_ref().map(|d|d.ids().iter().filter(|id| self.pending_mode.unwrap_or(d.mode())==Mode::NamedEntities || self.pending_mode.unwrap_or(d.mode())==Mode::Everyone && self.context.is_external(id)==Ok(true)).cloned().collect::<Vec<_>>()).unwrap_or_default(),
            "external":self.draft.as_ref().is_some_and(Draft::external_enabled),
            "query":self.query,"externalQuery":self.external_query,
            "results":if self.display_mode()==Some("named_entities"){self.results.as_slice()}else{&[]},"externalResults":if self.draft.as_ref().is_some_and(Draft::external_enabled){self.external_results.as_slice()}else{&[]}})
    }
    fn display_mode(&self) -> Option<&'static str> {
        self.pending_mode
            .map(Mode::as_str)
            .or_else(|| self.draft.as_ref().map(|d| d.mode().as_str()))
    }
    pub(super) fn desired(&self) -> Result<Option<protocol::Command>, domain::Error> {
        if self.pending {
            return Err(domain::Error::Unresolved);
        }
        self.draft
            .as_ref()
            .ok_or(domain::Error::Unresolved)?
            .desired(&self.context, &self.policy)
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"epoch":self.epoch,"generation":self.generation,"pending":self.pending,
            "mode":self.display_mode(),"ids":self.draft.as_ref().map(Draft::ids),
            "externalEnabled":self.draft.as_ref().is_some_and(Draft::external_enabled),"query":self.query,"externalQuery":self.external_query,
            "results":self.results,"externalResults":self.external_results,"receipts":self.receipts,"notice":self.notice,
            "scope":{"currentTeam":self.context.current_team_id,"topLevel":self.context.scope_id,"unified":self.context.unified_user,
                "collaborators":self.context.collaborators,"availableExternal":self.context.available_external_ids},
            "rows":self.rows.values().map(|r|json!({"id":r.id,"kind":format!("{:?}",r.kind),"label":r.label,"external":r.external,"raw":r.raw})).collect::<Vec<_>>()})
    }
}
pub(super) fn owned(s: &ShellState, visit: u64, id: u64) -> Option<&Confirmation> {
    s.app_home
        .about
        .management
        .permissions
        .confirmation
        .as_ref()
        .filter(|c| c.original.id == id && c.original.visit == visit && c.original.current(s, None))
}
