//! One exact accepted sort-producer edge creates a new read, never revives one.
use super::*;
use people_parameters::SortChange;

enum Stamp {
    Pending(Ticket),
    Failed(Ticket),
    Active(Activation),
}
pub(crate) struct RefreshSource {
    frame: Arc<Frame>,
    owner: Identity,
    cursor: Cursor,
    loading: bool,
    failed: bool,
    stamp: Stamp,
}
impl RefreshSource {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let v = scene(s)?;
        let stamp = if let Some(t) = v.book.pending() {
            Stamp::Pending(t.clone())
        } else if let Some(t) = v.book.failure() {
            Stamp::Failed(t.clone())
        } else {
            Stamp::Active(v.book.active()?.clone())
        };
        Some(Self {
            frame: v.frame.clone(),
            owner: Identity::capture(s)?,
            cursor: Cursor::capture(s),
            loading: s.search_pager.loading,
            failed: s.search_pager.failed,
            stamp,
        })
    }
    fn after(&self, s: &ShellState, change: &SortChange) -> Option<Parameters> {
        let m = s.search_people_module.as_ref()?;
        let v = m.scene.as_ref()?;
        if !self.owner.current(s)
            || !self.cursor.current(s)
            || self.loading != s.search_pager.loading
            || self.failed != s.search_pager.failed
            || !Arc::ptr_eq(&v.frame, &self.frame)
            || !Arc::ptr_eq(&m.controls, &self.frame.controls)
            || s.main_view != MainView::Home
            || s.core.active_channel.as_deref() != Some(self.frame.channel.as_str())
            || s.channel_generation != self.frame.generation
            || s.channel_tab != self.frame.tab
            || s.search_pager.query != self.frame.query
            || s.search_query.trim() != self.frame.query
            || !match &self.stamp {
                Stamp::Pending(t) => v.book.accepts(t),
                Stamp::Failed(t) => v.book.failure_current(t),
                Stamp::Active(a) => v.book.activation_current(a),
            }
        {
            return None;
        }
        self.frame.parameters.after_sort_change(s, change)
    }
}
pub(crate) struct RefreshWork(pub(super) load::Job);
pub(crate) fn after_sort_change(
    s: &mut ShellState,
    source: RefreshSource,
    change: &SortChange,
) -> Option<RefreshWork> {
    let parameters = source.after(s, change)?;
    // Cancel the previous loading latch too. All old frame/ticket/row pointers
    // remain retired, including equal and ABA events.
    s.search_pager.reset();
    let request = s.search_pager.begin(
        s.core.account_epoch,
        source.frame.owner.team(),
        &source.frame.query,
        false,
    )?;
    s.cancel_search_open();
    s.clear_search_results();
    s.search_history_open = false;
    load::begin_in(
        s,
        &request,
        parameters.transport(),
        parameters.client(),
        parameters.session(),
    )
    .map(RefreshWork)
}
pub(crate) fn run_refresh(state: Signal<ShellState>, work: RefreshWork) {
    dioxus::core::spawn_forever(load::run(state, work.0));
}
