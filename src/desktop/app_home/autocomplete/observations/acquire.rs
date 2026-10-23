//! One complete producer shared by startup and Share. No spawned tasks, polling
//! timer, disk work, or partial publication. A query waiter owns no producer.
use super::*;
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use serde_json::json;
use std::future::Future;
use super_platinum_core::slack::{PreparedRequest, api};

pub(crate) struct Acquired {
    pub(crate) store: Store,
    pub(crate) source: &'static str,
    /// Only reads dispatched by this call. Reuse/join consumes no new requests.
    pub(crate) reads: Vec<Value>,
}

pub(crate) async fn acquire(
    state: Signal<ShellState>,
    context: Arc<Context>,
    authority: Authority,
    current: &impl Fn(&ShellState) -> bool,
    online: &impl Fn(&ShellState) -> bool,
) -> Result<Acquired, Error> {
    let admitted = || {
        let s = state.peek();
        context.current(&s) && current(&s) && online(&s)
    };
    let read = |request| dispatch(state, &context, request, &admitted);
    acquire_with(state, context.clone(), authority, current, online, &read).await
}

/// Share supplies its original async native-authorized reader here. Startup
/// supplies account transport. Both use exactly the same complete decoders.
pub(crate) async fn acquire_with<F, Fut>(
    mut state: Signal<ShellState>,
    context: Arc<Context>,
    authority: Authority,
    current: &impl Fn(&ShellState) -> bool,
    online: &impl Fn(&ShellState) -> bool,
    read: &F,
) -> Result<Acquired, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let waiting = {
        let s = state.peek();
        if !current(&s) {
            return Err(unavailable());
        }
        // Verify the supplied original credential/Authority even on reuse.
        Owner::capture(&s, context.clone(), authority.clone())?;
        if let Some(store) = Store::capture(&s) {
            return Ok(Acquired {
                store,
                source: "reused",
                reads: vec![],
            });
        }
        if !online(&s) {
            return Err(at("observations.account-unavailable", unavailable()));
        }
        let r = &s.app_home.share_ranking_observations;
        r.owner.as_ref().filter(|o| o.current(&s)).and_then(|_| {
            r.pending
                .as_ref()
                .filter(|v| !v.finished())
                .map(|v| v.completed.subscribe())
        })
    };
    if let Some(mut completion) = waiting {
        if !*completion.borrow() {
            completion.changed().await.map_err(|_| unavailable())?;
        }
        let s = state.peek();
        if !current(&s) {
            return Err(unavailable());
        }
        Owner::capture(&s, context, authority)?;
        let store = Store::capture(&s).ok_or_else(unavailable)?;
        return Ok(Acquired {
            store,
            source: "reused",
            reads: vec![],
        });
    }
    let pending = {
        let mut s = state.write();
        if !current(&s) || !online(&s) {
            return Err(unavailable());
        }
        Pending::capture(&mut s, context.clone(), authority.clone())?
    };
    let admitted = || {
        let s = state.peek();
        current(&s) && online(&s) && pending.current(&s)
    };
    let guarded_read = |request| async {
        if !admitted() {
            return Err(unavailable());
        }
        let result = read(request).await;
        if !admitted() {
            return Err(unavailable());
        }
        result
    };
    let names = names::capture(state, &context, &authority, &guarded_read).await?;
    let request = api::list_experiments(&context.client, &context.session);
    let method = request.method;
    let assignments = guarded_read(request)
        .await
        .map_err(|e| at("observations.experiments-read", e))?;
    for key in [
        "autocomplete_flannel_fuzzy_match_passthrough",
        "autocomplete_affinity_score",
        "autocomplete_affinity_ml_model",
    ] {
        assignment(&assignments, key)?;
    }
    let mut reads = names.reads.clone();
    reads.push(json!({"method":method,"raw":assignments}));
    if !admitted() {
        return Err(unavailable());
    }
    let store = pending.publish(
        &mut state.write(),
        Complete {
            names,
            assignments,
            reads: reads.clone(),
        },
    )?;
    Ok(Acquired {
        store,
        source: "fresh",
        reads,
    })
}

async fn dispatch(
    state: Signal<ShellState>,
    context: &Context,
    request: PreparedRequest,
    current: &impl Fn() -> bool,
) -> Result<Value, Error> {
    if !current() {
        return Err(unavailable());
    }
    if crate::fixture::is_fixture() {
        // No fixture may fall through to a real account transport.
        return crate::app_home::about::management::fixture::first_publish::confirmation::share::startup::read(state, &request)
            .await.unwrap_or_else(|| Err(Error::Offline("closed account observations fixture has no response".into())));
    }
    context.transport.execute_current(request, current).await
}

pub(super) fn at(stage: &str, error: Error) -> Error {
    if !crate::fixture::is_fixture()
        || matches!(&error, Error::Api(code) if code.starts_with("share_ranking_stage:"))
    {
        error
    } else {
        Error::Api(format!("share_ranking_stage:{stage}"))
    }
}
