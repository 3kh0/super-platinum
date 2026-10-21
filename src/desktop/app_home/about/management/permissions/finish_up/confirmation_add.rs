//! One completed confirmation owns its channel observations and Add receipt.
use super::*;
use super_platinum_core::slack::PreparedRequest;
mod channels;
mod native;
mod run;
mod view;
pub(super) fn panel(state: Signal<ShellState>, visit: u64, id: u64) -> Element {
    view::panel(state, visit, id)
}

pub(super) struct State {
    generation: u64,
    epoch: u64,
    selected: String,
    listing: Option<channels::Listing>,
    choice: Option<channels::Choice>,
    loading: bool,
    notice: Option<String>,
    reads: Vec<Value>,
    operation: Option<run::Operation>,
    initial_checked: bool,
    initial_candidate: Option<Value>,
}
impl State {
    pub(super) fn submitted(&self) -> bool {
        self.operation.is_some()
    }
    fn new() -> Self {
        Self {
            generation: next(),
            epoch: next(),
            selected: String::new(),
            listing: None,
            choice: None,
            loading: true,
            notice: None,
            reads: vec![],
            operation: None,
            initial_checked: false,
            initial_candidate: None,
        }
    }
    fn expected(&self) -> Value {
        json!({"epoch":self.epoch,"selected":self.selected,"rows":self.listing.as_ref().map(|l|l.rows().iter().map(|c|json!({"id":c.id(),"label":c.label()})).collect::<Vec<_>>()).unwrap_or_default()})
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"generation":self.generation,"epoch":self.epoch,"selected":self.selected,
            "status":if self.loading{"loading"}else if self.listing.is_some(){"ready"}else{"unavailable"},
            "notice":self.notice,"rows":self.listing.as_ref().map(|l|l.rows().iter().map(|c|json!({"id":c.id(),"label":c.label(),"raw":c.raw()})).collect::<Vec<_>>()).unwrap_or_default(),
            "incomplete":self.listing.as_ref().is_none_or(|l|l.incomplete()),"reads":self.reads,
            "operation":self.operation.as_ref().map(run::Operation::snapshot),"initialCandidate":self.initial_candidate})
    }
}

#[derive(Clone)]
pub(super) struct Query {
    original: Original,
    id: u64,
    generation: u64,
    local: Value,
    expected: Value,
    account: Option<String>,
}
impl Query {
    fn capture(s: &ShellState, visit: u64, id: u64) -> Option<Self> {
        let a = attempt(s, id)
            .filter(|a| a.original.visit == visit && a.finished && a.complete && !a.closed)?;
        let original = a.dialog_original.as_ref().unwrap_or(&a.original).clone();
        if !original.current(s, Some(id)) {
            return None;
        }
        let add = a.confirmation_add.as_ref()?;
        Some(Self {
            original,
            id,
            generation: add.generation,
            local: a.local.expected(),
            expected: add.expected(),
            account: s.core.active_account.clone(),
        })
    }
    pub(super) fn ctx(&self) -> &crate::scheduled::Context {
        &self.original.owner.ctx
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        s.core.active_account == self.account
            && self.original.current(s, Some(self.id))
            && attempt(s, self.id).is_some_and(|a| {
                a.finished
                    && a.complete
                    && !a.closed
                    && a.local.expected() == self.local
                    && a.confirmation_add.as_ref().is_some_and(|v| {
                        v.generation == self.generation && v.expected() == self.expected
                    })
            })
    }
    async fn authorized(&self, state: Signal<ShellState>) -> bool {
        self.current(&state.peek())
            && native::bind(self).await
            && self.current(&state.peek())
            && native::current(self, None).await
            && self.current(&state.peek())
    }
    async fn retired(&self, state: Signal<ShellState>) {
        loop {
            if !self.current(&state.peek()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    pub(super) async fn read(
        &self,
        mut state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if !self.authorized(state).await {
            return Err(unavailable());
        }
        let result = tokio::select! {
            result=async {
                if fixture::first_publish::confirmation::requested() {
                    fixture::first_publish::confirmation::read(state,&request).await.unwrap_or_else(||Err(unavailable()))
                } else if crate::fixture::is_fixture() {Err(unavailable())}
                else {self.ctx().transport.execute(request).await}
            }=>result,
            _=self.retired(state)=>return Err(unavailable()),
        };
        if !self.current(&state.peek())
            || !native::current(self, None).await
            || !self.current(&state.peek())
        {
            return Err(unavailable());
        }
        if let Some(v) =
            attempt_mut(&mut state.write(), self.id).and_then(|a| a.confirmation_add.as_mut())
        {
            v.reads.push(match &result {
                Ok(raw) => json!({"generation":self.generation,"raw":raw}),
                Err(_) => json!({"generation":self.generation,"failure":"read failed"}),
            });
        }
        result
    }
}

fn mount(mut state: Signal<ShellState>, visit: u64, id: u64) {
    {
        let mut s = state.write();
        if !attempt(&s, id).is_some_and(|a| {
            a.dialog_original
                .as_ref()
                .unwrap_or(&a.original)
                .current(&s, Some(id))
        }) {
            return;
        }
        let Some(a) = attempt_mut(&mut s, id)
            .filter(|a| a.finished && a.complete && !a.closed && a.original.visit == visit)
        else {
            return;
        };
        if a.confirmation_add.is_some() {
            return;
        }
        a.confirmation_add = Some(State::new());
    }
    load(state, visit, id);
}
fn load(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let Some(q) = Query::capture(&state.peek(), visit, id) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = channels::listing(state, &q).await;
        let mut s = state.write();
        if !q.current(&s) {
            return;
        }
        let candidate = {
            let a = attempt(&s, id).unwrap();
            a.followup
                .as_ref()
                .and_then(|f| f.current_trigger())
                .and_then(|t| match &a.publication_reply {
                    Some(Reply::Raw(raw)) => {
                        protocol::confirmation_add::initial_candidate(t, &raw["decorated_workflow"])
                    }
                    _ => None,
                })
        };
        let v = attempt_mut(&mut s, id)
            .unwrap()
            .confirmation_add
            .as_mut()
            .unwrap();
        v.loading = false;
        match result {
            Ok(listing) => {
                if !v.initial_checked {
                    v.initial_checked = true;
                    v.initial_candidate = candidate.clone();
                    if let Some(choice) = candidate
                        .as_ref()
                        .and_then(Value::as_str)
                        .and_then(|id| listing.find(id))
                    {
                        v.selected = choice.id().into();
                        v.choice = Some(choice.clone());
                    }
                }
                v.notice = listing.incomplete().then(|| {
                    "Some channels could not be verified. Refresh channel choices.".into()
                });
                v.listing = Some(listing);
            }
            Err(_) => {
                v.notice =
                    Some("Channel choices could not be verified. Refresh channel choices.".into())
            }
        }
        v.epoch = next();
        drop(s);
        bind_visible(state, visit, id);
    });
}
fn bind_visible(state: Signal<ShellState>, visit: u64, id: u64) {
    if let Some(q) = Query::capture(&state.peek(), visit, id) {
        dioxus::core::spawn_forever(async move {
            if q.current(&state.peek()) {
                native::bind(&q).await;
            }
        });
    }
}
fn refresh(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let Some(q) = Query::capture(&state.peek(), visit, id) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !native::current(&q, Some(&json!({"kind":"refresh"}))).await {
            return;
        }
        let mut s = state.write();
        if !q.current(&s) {
            return;
        }
        let v = attempt_mut(&mut s, id)
            .unwrap()
            .confirmation_add
            .as_mut()
            .unwrap();
        if v.loading || v.operation.is_some() {
            return;
        }
        v.generation = next();
        v.epoch = next();
        v.loading = true;
        v.selected.clear();
        v.choice = None;
        v.listing = None;
        v.notice = None;
        drop(s);
        load(state, visit, id);
    });
}
fn select(mut state: Signal<ShellState>, visit: u64, id: u64, value: String) {
    let Some(q) = Query::capture(&state.peek(), visit, id) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !native::current(&q, Some(&json!({"kind":"select","value":value}))).await {
            return;
        }
        {
            let mut s = state.write();
            if !q.current(&s) {
                return;
            }
            let a = attempt(&s, id).unwrap().confirmation_add.as_ref().unwrap();
            if a.loading
                || a.operation.is_some()
                || (!value.is_empty()
                    && !a.listing.as_ref().is_some_and(|l| l.find(&value).is_some()))
            {
                return;
            }
            let v = attempt_mut(&mut s, id)
                .unwrap()
                .confirmation_add
                .as_mut()
                .unwrap();
            v.generation = next();
            v.epoch = next();
            v.selected = value.clone();
            v.choice = None;
            v.loading = !value.is_empty();
            v.notice = None;
        }
        let Some(q) = Query::capture(&state.peek(), visit, id) else {
            return;
        };
        if value.is_empty() {
            native::bind(&q).await;
            return;
        }
        let result = channels::validate(state, &q, &value).await;
        let mut s = state.write();
        if !q.current(&s) {
            return;
        }
        let v = attempt_mut(&mut s, id)
            .unwrap()
            .confirmation_add
            .as_mut()
            .unwrap();
        v.loading = false;
        v.epoch = next();
        match result {
            Ok(choice) => v.choice = Some(choice),
            Err(_) => {
                v.notice =
                    Some("This channel could not be verified. Refresh channel choices.".into())
            }
        };
        drop(s);
        bind_visible(state, visit, id);
    });
}
