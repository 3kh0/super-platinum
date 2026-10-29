//! Owned common3 People filter. Native adaptation clears all retained Search
//! bags, then fetches visible People only; Source e3 eagerly runs all modules.
use super::*;
use api::AccountType;

pub(crate) fn current(s: &ShellState) -> Option<AccountType> {
    Some(scene(s)?.frame.controls.account_types)
}
#[derive(Clone)]
pub(crate) struct FilterSource {
    frame: Arc<Frame>,
    owner: Identity,
    cursor: Cursor,
    failed: bool,
    activation: Activation,
}
impl FilterSource {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let v = scene(s)?;
        if s.search_pager.loading || v.book.pending().is_some() {
            return None;
        }
        let activation = v.book.active()?.clone();
        if !v.book.activation_current(&activation) {
            return None;
        }
        Some(Self {
            frame: v.frame.clone(),
            owner: Identity::capture(s)?,
            cursor: Cursor::capture(s),
            failed: s.search_pager.failed,
            activation,
        })
    }
}
pub(super) fn plan(
    s: &mut ShellState,
    source: FilterSource,
    kind: AccountType,
) -> Option<load::Job> {
    if !source.owner.current(s)
        || !source.cursor.current(s)
        || s.search_pager.loading
        || s.search_pager.failed != source.failed
        || source.frame.controls.account_types == kind
        || !owned(s, &source.frame).is_some_and(|v| v.book.activation_current(&source.activation))
    {
        return None;
    }
    // Native same-value refusal is stricter than Source's unconditional handler.
    // Captured promoted query is used; an edited box never supplies a new query.
    let mut controls = source.frame.controls.as_ref().clone();
    controls.account_types = kind;
    let request = s.search_pager.begin(
        s.core.account_epoch,
        source.frame.owner.team(),
        &source.frame.query,
        false,
    )?;
    s.cancel_search_open();
    s.clear_search_results();
    s.search_history_open = false;
    let module = s.search_people_module.as_mut()?;
    module.controls = Arc::new(controls);
    module.sessions.start_query(now_ms, uuid::Uuid::new_v4);
    let mut job = load::begin_in(
        s,
        &request,
        source.frame.parameters.transport(),
        source.frame.parameters.client(),
        source.frame.parameters.session(),
    )?;
    job.entry = load::EntryMode::FirstAll;
    Some(job)
}
pub(crate) fn change(mut state: Signal<ShellState>, source: FilterSource, kind: AccountType) {
    let job = state
        .try_write()
        .ok()
        .and_then(|mut s| plan(&mut s, source, kind));
    if let Some(job) = job {
        dioxus::core::spawn_forever(load::run(state, job));
    }
}
