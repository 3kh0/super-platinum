//! A typed query acquires candidates; only explicit selection opens a DM.
use super::*;
mod ranking;
mod read;

pub(super) struct State {
    pub(super) query: String,
    pub(super) revision: u64,
    pub(super) input_serial: u64,
    status: &'static str,
    pub(super) results: Option<Vec<String>>,
    pub(super) pinned: Option<(String, String)>,
    reads: Vec<Value>,
    failures: Vec<Value>,
    publications: Vec<Value>,
    ranking: Option<Value>,
    route: Option<Value>,
    history: Vec<Value>,
}
impl State {
    pub(super) fn new() -> Self {
        Self {
            query: String::new(),
            revision: next(),
            input_serial: next(),
            status: "searching",
            results: None,
            pinned: None,
            reads: vec![],
            failures: vec![],
            publications: vec![],
            ranking: None,
            route: None,
            history: vec![],
        }
    }
    pub(super) fn clear_for_recipient(&mut self) {
        self.history.push(json!({"query":self.query,"revision":self.revision,"status":self.status,"reads":self.reads,"failures":self.failures,"publications":self.publications,"ranking":self.ranking,"route":self.route}));
        self.query.clear();
        self.revision = next();
        self.input_serial = next();
        self.results = Some(vec![]);
        self.status = "recipient";
        self.pinned = None;
        self.reads.clear();
        self.failures.clear();
        self.publications.clear();
        self.ranking = None;
        self.route = None;
    }
    pub(super) fn busy(&self) -> bool {
        self.status == "searching"
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"query":self.query,"revision":self.revision,"status":self.status,
            "results":self.results,"reads":self.reads,"failures":self.failures,
            "publications":self.publications,"ranking":self.ranking,"route":self.route,"history":self.history})
    }
    pub(super) fn notice(&self) -> Option<&'static str> {
        match self.status {
            "searching" => Some("Searching conversations…"),
            "failed" => Some(
                "Search could not finish. Your previous destinations are still available. Refresh destinations to try again.",
            ),
            "partial" => {
                Some("Some results could not be verified. Refresh destinations to try again.")
            }
            "ready" if self.results.as_ref().is_some_and(Vec::is_empty) => {
                Some("No matching conversations found.")
            }
            _ => None,
        }
    }
}

/// Keep the latest input serial separate from the committed model/native
/// epoch. A second keystroke retires remote work immediately, but commits only
/// after the trailing debounce and original-node/value check.
pub(super) fn input(mut state: Signal<ShellState>, visit: u64, id: u64, value: String) {
    if value.len() > 4096 || value.contains('\0') {
        return;
    }
    let serial = {
        let mut s = state.write();
        let Some(q) = Query::capture(&s, visit, id) else {
            return;
        };
        let Some(v) = owned_mut(&mut s, &q) else {
            return;
        };
        if v.loading || v.operation.is_some() {
            return;
        }
        v.search.input_serial = next();
        v.search.input_serial
    };
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let Some(q) = Query::capture(&state.peek(), visit, id) else {
            return;
        };
        if !current(&state.peek(), &q, serial)
            || !native::current(&q, Some(&json!({"kind":"query","value":value}))).await
            || !current(&state.peek(), &q, serial)
        {
            return;
        }
        {
            let mut s = state.write();
            if !current(&s, &q, serial) {
                return;
            }
            let v = owned_mut(&mut s, &q).unwrap();
            let pinned = v.rows().into_iter().find(|(id, _)| id == &v.selected);
            let old = json!({"query":v.search.query,"revision":v.search.revision,
                "status":v.search.status,"reads":v.search.reads,"failures":v.search.failures,
                "publications":v.search.publications,"ranking":v.search.ranking,"route":v.search.route});
            if !v.search.query.is_empty()
                || v.search.ranking.is_some()
                || !v.search.reads.is_empty()
                || !v.search.failures.is_empty()
            {
                v.search.history.push(old);
            }
            v.search.query = value;
            v.selected_recipient = None;
            v.search.revision = next();
            v.search.pinned = pinned;
            v.search.reads.clear();
            v.search.failures.clear();
            v.search.publications.clear();
            v.search.ranking = None;
            v.search.route = None;
            // Clear is a new search, preserving the selected recipient and
            // last verified rows until its own observations settle.
            v.search.status = "searching";
            v.epoch = next();
            v.bound = false;
        }
        bind(state, visit, id).await;
        let Some(q) = Query::capture(&state.peek(), visit, id) else {
            return;
        };
        if !current(&state.peek(), &q, serial) {
            return;
        }
        let typed = owned_mut(&mut state.write(), &q)
            .map(|v| v.search.query.clone())
            .unwrap_or_default();
        run(state, q, serial, typed).await;
    });
}
fn current(s: &ShellState, q: &Query, serial: u64) -> bool {
    q.current(s)
        && attempt(s, q.id)
            .and_then(|a| a.confirmation_share.as_ref())
            .is_some_and(|v| v.search.input_serial == serial)
}
async fn run(mut state: Signal<ShellState>, q: Query, serial: u64, typed: String) {
    let result = read::acquire(state, &q, &typed, &|s| current(s, &q, serial)).await;
    if !current(&state.peek(), &q, serial) {
        if let Some(v) = owned_mut(&mut state.write(), &q) {
            v.search.history.push(
                json!({"query":typed,"status":"retired","reads":result.reads,
                "failures":result.failures,"publications":result.publications,"route":result.route}),
            );
        }
        return;
    }
    let mut failures = result.failures.clone();
    let seeds = (|| {
        let s = state.peek();
        if !current(&s, &q, serial) || !result.current(&s) {
            return Err(unavailable());
        }
        let plan = super_platinum_core::workflow_share::search::Query::new(&typed)?;
        let source =
            crate::app_home::channel_fetches::canonical::observed(&s, &q.ctx().session.team_id)?;
        let people = if plan.includes(super_platinum_core::workflow_share::search::Kind::Members) {
            peer::ranking_candidates(&s, &q.ctx().session.team_id)?
        } else {
            vec![]
        };
        let channels = if plan.includes(super_platinum_core::workflow_share::search::Kind::Channels)
        {
            source
                .rows()
                .iter()
                .filter(|raw| super_platinum_core::workflow_share::joined_channel_candidate(raw))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        Ok::<_, Error>((source, people, channels))
    })();
    let (ordered, mut people) = match seeds {
        Ok((source, people, channels)) => {
            let ordered = ranking::order(
                state,
                &q,
                source,
                &people,
                &channels,
                &result.member_ids,
                &result.channel_ids,
                &|s| current(s, &q, serial) && result.current(s),
            )
            .await;
            (ordered, people)
        }
        Err(error) => (Err(error), vec![]),
    };
    if !current(&state.peek(), &q, serial) {
        return;
    }
    let ordered = match ordered {
        Ok(value) => Some(value),
        Err(error) => {
            let mut failure =
                json!({"stage":"ranking","failure":"search observations unavailable"});
            if crate::fixture::is_fixture() {
                if let Error::Api(code) = &error {
                    if code.starts_with("share_ranking_stage:") {
                        failure["diagnostic"] = code.clone().into();
                    }
                }
            }
            failures.push(failure);
            None
        }
    };
    let mut ids = ordered.as_ref().map(|v| v.ids.clone()).unwrap_or_default();
    people.retain(|p| ids.iter().any(|id| id == p.id()));
    let mut choices = Vec::new();
    for id in &ids {
        if people.iter().any(|p| p.id() == id) {
            continue;
        }
        if !current(&state.peek(), &q, serial)
            || !result.current(&state.peek())
            || ordered
                .as_ref()
                .is_none_or(|v| !v.current(&state.peek(), &q, &|s| current(s, &q, serial)))
        {
            break;
        }
        let choice = if result.online() {
            channels::validate(state, &q, id).await
        } else {
            channels::known(state, &q, id)
        };
        match choice {
            Ok(choice) => choices.push(choice),
            Err(_) => failures
                .push(json!({"channel":id,"stage":"posting","failure":"destination unavailable"})),
        }
    }
    let mut s = state.write();
    if !current(&s, &q, serial) {
        return;
    }
    let source_current = result.current(&s)
        && ordered
            .as_ref()
            .is_some_and(|v| v.current(&s, &q, &|s| current(s, &q, serial)));
    if !source_current {
        failures.push(json!({"stage":"adoption","failure":"source retired"}));
        choices.clear();
        people.clear();
    }
    ids.retain(|id| people.iter().any(|p| p.id() == id) || choices.iter().any(|c| c.id() == id));
    let previous = attempt(&s, q.id).and_then(|a| a.confirmation_share.as_ref());
    let selected = previous.map(|v| v.selected.clone()).unwrap_or_default();
    let merged = channels::Listing::merge_search(
        previous.and_then(|v| v.listing.as_ref()),
        &s,
        &q.ctx().session.team_id,
        choices,
        &selected,
    );
    if merged.is_err() {
        failures.push(json!({"stage":"adoption","failure":"posting source retired"}));
        ids.retain(|id| people.iter().any(|p| p.id() == id));
    }
    let mut prior = previous
        .map(|v| {
            v.people
                .iter()
                .filter(|p| p.current(&s))
                .take(50)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let v = owned_mut(&mut s, &q).unwrap();
    if let Ok(listing) = merged {
        v.listing = listing;
    }
    if !people.is_empty() {
        prior.retain(|p| !people.iter().any(|new| new.id() == p.id()));
        prior.extend(people);
        v.people = prior;
    }
    let failed = !failures.is_empty() && ids.is_empty();
    v.search.status = if failed {
        "failed"
    } else if failures.is_empty() {
        "ready"
    } else {
        "partial"
    };
    if !failed {
        v.search.results = Some(ids);
        // The ranked adoption has its own current partial/ready guidance. A
        // warning from the initial known-destination load is no longer current.
        v.notice = None;
    }
    v.search.reads = result.reads;
    v.search.failures = failures;
    v.search.publications = result.publications;
    v.search.ranking = ordered.map(|v| v.proof);
    v.search.route = Some(result.route);
    v.epoch = next();
    v.bound = false;
    drop(s);
    bind(state, q.original.visit, q.id).await;
}
