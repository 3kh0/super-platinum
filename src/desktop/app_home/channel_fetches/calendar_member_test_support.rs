//! Test-only original assignments, initialization, scheduler and renderer contracts.
//! Closed actual response settlement, never synthetic Ready or transport execution.
use super::*;
use dioxus::{
    core::{ScopeId, internal::generational_box},
    document::{Document, Eval, EvalError, Evaluator},
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Context as TaskContext, Poll},
};

/// Remove actual observed registry facts; this never supplies readiness defaults.
pub(crate) fn retire_member_flags(shell: &mut ShellState) {
    shell.app_home.channel_fetches = Default::default();
}

pub(crate) fn connect(shell: &mut ShellState) -> Result<(), Error> {
    let team = shell.core.active_team.clone().ok_or_else(unavailable)?;
    // Fixture auth is final now. Register through the production producer;
    // the earlier installer intent cannot survive a client-mode replacement.
    events::request_initialization(shell, &team, true);
    let generation = shell
        .core
        .workspaces
        .get(&team)
        .ok_or_else(unavailable)?
        .rt_generation
        .checked_add(1)
        .ok_or_else(unavailable)?;
    shell.core.workspaces.get_mut(&team).unwrap().rt_generation = generation;
    events::connected(shell, &team, generation)
        .is_some_and(|work| work.startup.is_some())
        .then_some(())
        .ok_or_else(unavailable)
}

pub(crate) struct Initialization {
    owner: Owner,
}
pub(crate) fn assignments(shell: &mut ShellState, raw: &Value) -> Result<Initialization, Error> {
    let team = shell.core.active_team.clone().ok_or_else(unavailable)?;
    let ctx = Arc::new(Context::capture(shell, &team)?);
    let transformations = Arc::new(members::Transformations::decode(raw)?);
    let owner = initial::begin(shell, ctx, transformations, true, false)?;
    Ok(Initialization { owner })
}
impl Initialization {
    pub(crate) async fn drain(&self, state: Signal<ShellState>) -> Result<(), Error> {
        members::drain(state, &self.owner).await
    }
}

fn stage(stage: &str, error: Error) -> Error {
    Error::Api(format!("offline split-init {stage}: {error:?}"))
}
impl Initialization {
    pub(crate) fn diagnostic(&self, shell: &ShellState) -> Value {
        let slot = shell
            .app_home
            .channel_fetches
            .slots
            .get(&self.owner.context.key());
        serde_json::json!({
            "ownerCurrent": self.owner.current(shell),
            "publicationCurrent": self.owner.publication_current(shell),
            "initialPresent":slot.is_some_and(|s|s.initial.is_some()),
            "settlements":slot.map(|s|s.payload.settlements.iter().map(|r|serde_json::json!({
                "kind":format!("{:?}",r.kind()),"result":format!("{:?}",r.result()),
                "failedEffect":r.failed_effect().map(|e|format!("{:?}",e.kind))
            })).collect::<Vec<_>>())
        })
    }
    /// Complete the original split initialization, including the real lifecycle
    /// callback and channel queue. Caller drives the actual VirtualDom scheduler.
    pub(crate) async fn complete_channels(
        &self,
        mut state: Signal<ShellState>,
        init: Value,
        channels: Value,
    ) -> Result<Arc<canonical::Canonical>, Error> {
        members::drain(state, &self.owner)
            .await
            .map_err(|e| stage("member drain", e))?;
        drain::wait(state, &self.owner)
            .await
            .map_err(|e| stage("channel drain", e))?;
        let read = {
            let s = state.peek();
            let key = self.owner.context.key();
            let token = s
                .app_home
                .channel_fetches
                .slots
                .get(&key)
                .and_then(|slot| slot.initial.as_ref())
                .ok_or_else(unavailable)?
                .token
                .clone();
            initial::Read {
                owner: self.owner.clone(),
                token,
                channel_revision: s.core.workspaces[&self.owner.context.session.team_id]
                    .channel_lifecycle
                    .revision(),
            }
        };
        let prepared = payload::prepare(
            state,
            &self.owner,
            super_platinum_core::state::cfm_payload::PayloadKind::Init,
            &init,
            init.to_string(),
        )
        .await
        .map_err(|e| stage("prepare Init", e))?;
        initial::settle_payload(
            &mut state.write(),
            &read,
            super_platinum_core::state::directory_channels::freshness::refresh::Branch::Init,
            prepared,
        )
        .map_err(|e| stage("settle Init", e))?;
        payload::kick(state, self.owner.clone());
        members::drain(state, &self.owner)
            .await
            .map_err(|e| stage("member drain", e))?;
        drain::wait(state, &self.owner)
            .await
            .map_err(|e| stage("channel drain", e))?;
        initial::settle_channels(&mut state.write(), &read, &channels)
            .map_err(|e| stage("settle Channels", e))?;
        drain::wait(state, &self.owner)
            .await
            .map_err(|e| stage("channel drain", e))?;
        let extra = serde_json::json!({"ok":true});
        let prepared = payload::prepare(
            state,
            &self.owner,
            super_platinum_core::state::cfm_payload::PayloadKind::Extras,
            &extra,
            extra.to_string(),
        )
        .await
        .map_err(|e| stage("prepare Extras", e))?;
        initial::settle_payload(
            &mut state.write(),
            &read,
            super_platinum_core::state::directory_channels::freshness::refresh::Branch::Extras,
            prepared,
        )
        .map_err(|e| stage("settle Extras", e))?;
        payload::kick(state, self.owner.clone());
        members::drain(state, &self.owner)
            .await
            .map_err(|e| stage("member drain", e))?;
        drain::wait(state, &self.owner)
            .await
            .map_err(|e| stage("channel drain", e))?;
        let result = canonical::capture(&mut state.write(), &self.owner.context.session.team_id);
        result.map_err(|e| stage("canonical capture", e))
    }
}

struct Returned {
    reply: Option<Value>,
    held: Option<tokio::sync::oneshot::Receiver<Value>>,
}
impl Evaluator for Returned {
    fn send(&self, _: Value) -> Result<(), EvalError> {
        Err(EvalError::Unsupported)
    }
    fn poll_recv(&mut self, _: &mut TaskContext<'_>) -> Poll<Result<Value, EvalError>> {
        Poll::Ready(Err(EvalError::Unsupported))
    }
    fn poll_join(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<Value, EvalError>> {
        if let Some(receiver) = self.held.as_mut() {
            let result = match Pin::new(receiver).poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(reply)) => Ok(reply),
                Poll::Ready(Err(_)) => Err(EvalError::Unsupported),
            };
            self.held = None;
            self.reply = None;
            return Poll::Ready(result);
        }
        Poll::Ready(self.reply.take().ok_or(EvalError::Unsupported))
    }
}
#[derive(Default)]
pub(crate) struct Renderer {
    owner: generational_box::Owner,
    requests: RefCell<Vec<Value>>,
    holds: RefCell<VecDeque<tokio::sync::oneshot::Receiver<Value>>>,
}
impl Renderer {
    pub(crate) fn requests(&self) -> Vec<Value> {
        self.requests.borrow().clone()
    }
    pub(crate) fn hold_next(&self) -> tokio::sync::oneshot::Sender<Value> {
        let (send, recv) = tokio::sync::oneshot::channel();
        self.holds.borrow_mut().push_back(recv);
        send
    }
    pub(crate) fn reply(request: &Value) -> Value {
        let fields = request["fields"]
            .as_array()
            .expect("original fields request");
        assert!(fields.len() <= 128);
        let values: Vec<_> = fields
            .iter()
            .map(|field| {
                let field = field.as_str().unwrap();
                assert!(field.is_ascii(), "modeled renderer is ASCII only");
                field.to_ascii_lowercase()
            })
            .collect();
        serde_json::json!({"generation":request["generation"],"stage":request["stage"],"fields":fields,"values":values})
    }
}
impl Document for Renderer {
    fn eval(&self, js: String) -> Eval {
        let prefix = concat!(include_str!("render.js"), "\nreturn channelOrdinaryFields(");
        let raw = js
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(");"))
            .expect("only original member-fields evaluator");
        let request: Value = serde_json::from_str(raw).unwrap();
        assert!(
            request["generation"]
                .as_str()
                .is_some_and(|g| !g.is_empty())
        );
        assert!(request["stage"].as_u64().is_some());
        let reply = Self::reply(&request);
        self.requests.borrow_mut().push(request);
        let held = self.holds.borrow_mut().pop_front();
        let evaluator: Box<dyn Evaluator> = Box::new(Returned {
            reply: Some(reply),
            held,
        });
        Eval::new(self.owner.insert(evaluator))
    }
}
pub(crate) fn provide(dom: &VirtualDom, renderer: Rc<Renderer>) {
    dom.in_scope(ScopeId::ROOT, || {
        let document: Rc<dyn Document> = renderer;
        provide_context(document);
    });
}
pub(crate) async fn run<F: Future>(dom: &VirtualDom, work: F) -> F::Output {
    let mut work = Box::pin(work);
    poll_fn(|cx| dom.in_scope(ScopeId::ROOT, || work.as_mut().poll(cx))).await
}
pub(crate) async fn pending<F: Future>(dom: &VirtualDom, mut work: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(
            dom.in_scope(ScopeId::ROOT, || work.as_mut().poll(cx))
                .is_pending()
        );
        Poll::Ready(())
    })
    .await
}

/// Drive separately spawned production tasks while the original operation waits.
/// Runtime is retained independently of the mutable DOM wait/render borrow.
pub(crate) async fn run_pumped<F: Future>(dom: &mut VirtualDom, work: F) -> F::Output {
    let runtime = dom.runtime();
    let mut work = Box::pin(work);
    loop {
        tokio::select! {
            output=poll_fn(|cx|runtime.in_scope(ScopeId::ROOT,||work.as_mut().poll(cx)))=>return output,
            _=dom.wait_for_work()=>dom.render_immediate(&mut dioxus::core::NoOpMutations),
        }
    }
}
