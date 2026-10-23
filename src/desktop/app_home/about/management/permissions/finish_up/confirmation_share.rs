//! Share owns its editor and send independently of the channel/thread composers.
use super::*;
use super_platinum_core::slack::PreparedRequest;
mod cancel;
mod channels;
pub(crate) use cancel::row as delivery_row;
mod group;
mod native;
mod peer;
mod person;
mod posting;
mod read;
mod recipients;
mod resend;
mod rows;
mod run;
mod search;
mod view;
pub(super) use view::panel;

pub(super) fn bind_entry(state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        attempt(&s, id)
            .filter(|a| a.original.visit == visit && a.complete && !a.closed)
            .map(|a| {
                (
                    a.dialog_original.as_ref().unwrap_or(&a.original).clone(),
                    a.local.expected(),
                )
            })
    };
    if let Some((o, e)) = captured {
        dioxus::core::spawn_forever(async move {
            native::bind_entry(&o, &e).await;
        });
    }
}

pub(super) struct State {
    active: bool,
    generation: u64,
    epoch: u64,
    text: String,
    selected: String,
    loading: bool,
    bound: bool,
    listing: Option<channels::Listing>,
    people: Vec<peer::Candidate>,
    opening: Option<person::Opening>,
    opening_history: Vec<person::Opening>,
    recipients: Vec<peer::Candidate>,
    recipient_revision: u64,
    selected_recipient: Option<usize>,
    group_opening: Option<group::Opening>,
    group_history: Vec<group::Opening>,
    notice: Option<String>,
    reads: Vec<Value>,
    operation: Option<run::Operation>,
    retry_after: Option<tokio::time::Instant>,
    search: search::State,
}
impl State {
    fn new() -> Self {
        Self {
            active: true,
            generation: next(),
            epoch: next(),
            text: String::new(),
            selected: String::new(),
            loading: true,
            bound: false,
            listing: None,
            people: vec![],
            opening: None,
            opening_history: vec![],
            recipients: vec![],
            recipient_revision: next(),
            selected_recipient: None,
            group_opening: None,
            group_history: vec![],
            notice: None,
            reads: vec![],
            operation: None,
            retry_after: None,
            search: search::State::new(),
        }
    }
    fn expected(&self) -> Value {
        json!({"generation":self.generation,"epoch":self.epoch,"text":self.text,"selected":self.selected,
            "recipients":self.recipient_rows(),"recipientRevision":self.recipient_revision,"selectedRecipient":self.selected_recipient,
            "query":self.search.query,"queryRevision":self.search.revision,"queryInput":self.search.input_serial,
            "rows":self.rows().into_iter().map(|(id,label)|json!({"id":id,"label":label})).collect::<Vec<_>>()})
    }
    fn recipient_rows(&self) -> Vec<Value> {
        self.recipients
            .iter()
            .map(|p| json!({"id":p.id(),"label":p.label()}))
            .collect()
    }
    fn rows(&self) -> Vec<(String, String)> {
        let conversations = self
            .listing
            .as_ref()
            .map(|l| {
                l.rows()
                    .iter()
                    .map(|r| {
                        (
                            r.id().to_owned(),
                            r.label().to_owned(),
                            (r.raw()["is_im"] == true)
                                .then(|| r.raw()["user"].as_str().map(str::to_owned))
                                .flatten(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let people = self
            .people
            .iter()
            .map(|p| (p.id().to_owned(), p.label().to_owned()))
            .collect::<Vec<_>>();
        let pinned = self
            .recipients
            .iter()
            .find(|p| p.id() == self.selected)
            .map(|p| (p.id().to_owned(), p.label().to_owned()))
            .or_else(|| self.search.pinned.clone());
        rows::project(
            &conversations,
            &people,
            &self.selected,
            pinned.as_ref(),
            self.search.results.as_deref(),
        )
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"active":self.active,"generation":self.generation,"epoch":self.epoch,"text":self.text,
            "selected":self.selected,"loading":self.loading,"bound":self.bound,"notice":self.notice,
            "rows":self.listing.as_ref().map(|l|l.rows().iter().map(|r|json!({"id":r.id(),"label":r.label(),"raw":r.raw(),"peer":r.peer_evidence(),"group":r.group_evidence()})).collect::<Vec<_>>()).unwrap_or_default(),
            "incomplete":self.listing.as_ref().is_none_or(|l|l.incomplete()),"reads":self.reads,
            "operation":self.operation.as_ref().map(run::Operation::snapshot),
            "people":self.people.iter().map(peer::Candidate::evidence).collect::<Vec<_>>(),
            "options":self.rows().into_iter().map(|(id,label)|json!({"id":id,"label":label})).collect::<Vec<_>>(),
            "opening":self.opening.as_ref().map(person::Opening::snapshot),
            "openingHistory":self.opening_history.iter().map(person::Opening::snapshot).collect::<Vec<_>>(),
            "recipients":self.recipients.iter().map(|p|json!({"id":p.id(),"label":p.label(),"evidence":p.evidence()})).collect::<Vec<_>>(),
            "recipientRevision":self.recipient_revision,"selectedRecipient":self.selected_recipient,
            "groupOpening":self.group_opening.as_ref().map(group::Opening::snapshot),
            "groupOpeningHistory":self.group_history.iter().map(group::Opening::snapshot).collect::<Vec<_>>(),"search":self.search.snapshot(),
            "retryAfterMillis":self.retry_after.map(|until|until.saturating_duration_since(tokio::time::Instant::now()).as_millis())})
    }
}
#[derive(Clone)]
pub(super) struct Query {
    original: Original,
    id: u64,
    generation: u64,
    local: Value,
    expected: Value,
}
impl Query {
    fn capture(s: &ShellState, visit: u64, id: u64) -> Option<Self> {
        let a = attempt(s, id)
            .filter(|a| a.original.visit == visit && a.finished && a.complete && !a.closed)?;
        let original = a.dialog_original.as_ref().unwrap_or(&a.original).clone();
        let share = a.confirmation_share.as_ref().filter(|v| v.active)?;
        if !a.local.share || !original.current(s, Some(id)) {
            return None;
        }
        Some(Self {
            original,
            id,
            generation: share.generation,
            local: a.local.expected(),
            expected: share.expected(),
        })
    }
    pub(super) fn ctx(&self) -> &crate::scheduled::Context {
        &self.original.owner.ctx
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.original.current(s, Some(self.id))
            && attempt(s, self.id).is_some_and(|a| {
                !a.closed
                    && a.finished
                    && a.complete
                    && a.local.expected() == self.local
                    && a.confirmation_share.as_ref().is_some_and(|v| {
                        v.active && v.generation == self.generation && v.expected() == self.expected
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
        while self.current(&state.peek()) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
fn owned_mut<'a>(s: &'a mut ShellState, q: &Query) -> Option<&'a mut State> {
    attempt_mut(s, q.id)?
        .confirmation_share
        .as_mut()
        .filter(|v| v.generation == q.generation)
}
pub(super) fn open(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let captured = {
        let s = state.peek();
        attempt(&s, id)
            .filter(|a| {
                a.original.visit == visit
                    && a.finished
                    && a.complete
                    && !a.closed
                    && !a.local.share
                    && a.confirmation_add.as_ref().is_none_or(|v| !v.submitted())
            })
            .map(|a| {
                (
                    a.dialog_original.as_ref().unwrap_or(&a.original).clone(),
                    a.local.expected(),
                )
            })
    };
    let Some((o, e)) = captured else { return };
    dioxus::core::spawn_forever(async move {
        if !super::native::current(&o, &e, None).await || !native::entry(&o, &e).await {
            return;
        }
        {
            let mut s = state.write();
            if !o.current(&s, Some(id)) {
                return;
            }
            let Some(a) = attempt_mut(&mut s, id).filter(|a| {
                !a.closed
                    && a.local.expected() == e
                    && a.confirmation_add.as_ref().is_none_or(|v| !v.submitted())
            }) else {
                return;
            };
            if a.confirmation_share
                .as_ref()
                .is_some_and(|v| v.operation.is_some())
            {
                return;
            }
            a.local.share = true;
            a.local.epoch = next();
            let retry_after = a.confirmation_share.as_ref().and_then(|v| v.retry_after);
            let mut share = State::new();
            share.retry_after = retry_after;
            a.confirmation_share = Some(share);
        }
        bind(state, visit, id).await;
        load(state, visit, id).await;
    });
}
async fn bind(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let q = Query::capture(&state.peek(), visit, id);
    let Some(q) = q else { return };
    let ready = native::bind(&q).await;
    if q.current(&state.peek()) {
        if let Some(v) = owned_mut(&mut state.write(), &q) {
            v.bound = ready;
        }
    }
}
async fn load(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let q = Query::capture(&state.peek(), visit, id);
    let Some(q) = q else { return };
    let result = channels::listing(state, &q).await;
    let people = peer::candidates(&state.peek(), &q.ctx().session.team_id).unwrap_or_default();
    {
        let mut s = state.write();
        if !q.current(&s) {
            return;
        }
        let v = owned_mut(&mut s, &q).unwrap();
        v.loading = false;
        v.epoch = next();
        v.bound = false;
        v.people = people;
        match result {
            Ok(listing) => {
                v.notice = listing.incomplete().then(|| {
                    "Some destinations could not be verified. Refresh to try again.".into()
                });
                v.listing = Some(listing);
            }
            Err(_) => v.notice = Some("Destinations could not be loaded. Try again.".into()),
        }
    }
    bind(state, visit, id).await;
    // Mount searches the empty query through the same owned ranking path as
    // typing and clearing. The loading phase never exposes an unranked ready
    // selector between its original destination reads and this search.
    search::input(state, visit, id, String::new());
}
pub(super) fn edit(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    kind: &'static str,
    value: String,
) {
    if kind == "select" {
        recipients::select(state, visit, id, value);
        return;
    }
    let q = Query::capture(&state.peek(), visit, id);
    let Some(q) = q else { return };
    dioxus::core::spawn_forever(async move {
        if !q.current(&state.peek())
            || !native::current(&q, Some(&json!({"kind":kind,"value":value}))).await
        {
            return;
        }
        {
            let mut s = state.write();
            if !q.current(&s) {
                return;
            }
            let v = owned_mut(&mut s, &q).unwrap();
            if v.operation.is_some() || v.loading || v.search.busy() {
                return;
            }
            match kind {
                "text" => v.text = value,
                "refresh" => {
                    let value = v.search.query.clone();
                    drop(s);
                    search::input(state, visit, id, value);
                    return;
                }
                _ => return,
            }
            v.epoch = next();
            v.bound = false;
            v.notice = None;
        }
        bind(state, visit, id).await;
    });
}
pub(super) fn cancel(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let q = Query::capture(&state.peek(), visit, id);
    let Some(q) = q else { return };
    dioxus::core::spawn_forever(async move {
        if !q.current(&state.peek()) || !native::current(&q, Some(&json!({"kind":"cancel"}))).await
        {
            return;
        }
        let mut s = state.write();
        if !q.current(&s) {
            return;
        }
        close_current(&mut s, &q);
    });
}
fn close_current(s: &mut ShellState, q: &Query) {
    if let Some(a) = attempt_mut(s, q.id) {
        a.local.share = false;
        a.local.epoch = next();
        if let Some(v) = a.confirmation_share.as_mut() {
            v.active = false;
        }
    }
    super::readback::close_current(s, q.original.visit, q.id, None);
}
