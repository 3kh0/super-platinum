use super::*;

#[derive(Clone, Serialize)]
pub(super) struct Expected {
    visit: u64,
    document: u64,
    generation: u64,
    search: String,
    filters: Filters,
}
impl Expected {
    pub(super) fn capture(s: &ShellState, visit: u64) -> Option<Self> {
        let d = document(s, visit).filter(|d| d.activity.active && !d.activity.member_composing)?;
        Some(Self {
            visit,
            document: d.id,
            generation: d.activity.member_generation,
            search: d.activity.user_search.clone(),
            filters: d.activity.filters.clone(),
        })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        Self::capture(s, self.visit).is_some_and(|v| {
            v.document == self.document
                && v.generation == self.generation
                && v.search == self.search
                && v.filters == self.filters
        })
    }
}
pub(super) async fn snapshot(expected: &Expected, selection: Option<&str>) -> Result<(), Error> {
    let mut expected = expected.clone();
    if let Some(value) = selection {
        expected.filters.user = value.into();
    }
    let script = format!(
        "{}\n{}\ndioxus.send(await workflowMemberCommittedSnapshot({}));await dioxus.recv();",
        include_str!("../date_witness.js"),
        include_str!("native.js"),
        json!(expected)
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<bool>()).await;
    let _ = eval.send(());
    if crate::fixture::is_fixture() && std::env::var(fixture::FLAG).as_deref() == Ok("1") {
        eprintln!(
            "workflow-activity-member-guard: {}",
            json!({"expected":expected,"result":format!("{result:?}")})
        );
    }
    if matches!(result, Ok(Ok(true))) {
        Ok(())
    } else {
        Err(unavailable())
    }
}
pub(super) async fn query(state: Signal<ShellState>, q: &Query) -> Result<(), Error> {
    let expected = {
        let s = state.peek();
        if !q.current(&s) {
            return Err(unavailable());
        }
        Expected::capture(&s, q.visit).ok_or_else(unavailable)?
    };
    snapshot(&expected, None).await?;
    if expected.current(&state.peek()) && q.current(&state.peek()) {
        Ok(())
    } else {
        Err(unavailable())
    }
}
pub(in crate::app_home::about::management::activity) fn choose(
    mut state: Signal<ShellState>,
    visit: u64,
    generation: u64,
    filter_revision: u64,
    id: String,
) {
    let expected = { Expected::capture(&state.peek(), visit) };
    let Some(expected) = expected.filter(|e| e.filters.revision == filter_revision) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if snapshot(&expected, Some(&id)).await.is_err() {
            return;
        }
        let mut s = state.write();
        if expected.current(&s) {
            lookup::choose(&mut s, visit, generation, filter_revision, id);
        }
    });
}
