use super::*;
use std::time::{Duration, Instant};
use super_platinum_core::{
    channel_workflows::{
        Read,
        details::{Failure, Trigger, Workflow},
    },
    slack::Error,
};

pub(super) fn start(mut state: Signal<ShellState>, selection: Arc<Selection>, initial: bool) {
    if !selection.selected(&state.peek()) || (!initial && cooling(&state.peek()).blocked) {
        return;
    }
    {
        let m = selection.model.lock().expect("workflow details model");
        if m.status == "loading" || initial && m.serial != 0 {
            return;
        }
    }
    let expected = selection
        .model
        .lock()
        .expect("workflow details model")
        .serial;
    spawn(async move {
        if !selection.selected(&state.peek())
            || !native::bind(&selection).await
            || !selection.selected(&state.peek())
            || cooling(&state.peek()).blocked
            || selection
                .model
                .lock()
                .expect("workflow details model")
                .serial
                != expected
        {
            return;
        }
        if !initial && !native::current(&selection, "retry").await {
            return;
        }
        if !selection.selected(&state.peek())
            || cooling(&state.peek()).blocked
            || selection
                .model
                .lock()
                .expect("workflow details model")
                .serial
                != expected
        {
            return;
        }
        let serial = {
            let _s = state.write();
            let mut m = selection.model.lock().expect("workflow details model");
            m.serial = m.serial.wrapping_add(1);
            m.status = "loading";
            m.phase = Some("trigger");
            m.error = None;
            m.trigger = None;
            m.workflow = None;
            m.projection = None;
            m.serial
        };
        let result = load(state, &selection, serial).await;
        if !admitted(&state.peek(), &selection, serial) {
            return;
        }
        let _s = state.write();
        let mut m = selection.model.lock().expect("workflow details model");
        m.phase = None;
        match result {
            Ok(p) => {
                m.projection = Some(p);
                m.status = "ready";
            }
            Err(error) => {
                m.status = "failed";
                m.error = Some(error);
            }
        }
    });
}
fn admitted(s: &ShellState, d: &Selection, serial: u64) -> bool {
    d.selected(s) && d.model.lock().expect("workflow details model").serial == serial
}
fn generic() -> (&'static str, &'static str) {
    (
        "Something’s gone wrong",
        "Oops, there was an error loading this workflow.",
    )
}
fn failure(e: Failure) -> (&'static str, &'static str) {
    failure_kind(e.kind())
}
fn failure_kind(
    kind: &super_platinum_core::channel_workflows::details::FailureKind,
) -> (&'static str, &'static str) {
    use super_platinum_core::channel_workflows::details::FailureKind;
    match kind {
        FailureKind::NotFound => ("Workflow not found", "This workflow no longer exists."),
        FailureKind::AccessDenied => (
            "Permission denied",
            "You don’t have permissions to view workflow details.",
        ),
        _ => generic(),
    }
}
fn transport_failure(error: Error) -> (&'static str, &'static str) {
    if let Error::Api(code) = error {
        failure_kind(
            &super_platinum_core::channel_workflows::details::FailureKind::from_code(&code),
        )
    } else {
        generic()
    }
}
async fn load(
    mut state: Signal<ShellState>,
    d: &Arc<Selection>,
    serial: u64,
) -> Result<Projection, (&'static str, &'static str)> {
    let read = Read::trigger_info(&d.trigger).map_err(|_| generic())?;
    let raw = request(state, d, serial, read)
        .await
        .map_err(transport_failure)?;
    if !admitted(&state.peek(), d, serial) {
        return Err(generic());
    }
    {
        let _s = state.write();
        d.model.lock().expect("workflow details model").trigger = Some(raw.clone());
    }
    let trigger = Trigger::decode(&d.trigger, raw).map_err(failure)?;
    let workflow = if let Some(id) = trigger.workflow_id() {
        {
            let _s = state.write();
            d.model.lock().expect("workflow details model").phase = Some("workflow");
        }
        let read = Read::workflow(id).map_err(|_| generic())?;
        let raw = request(state, d, serial, read)
            .await
            .map_err(transport_failure)?;
        if !admitted(&state.peek(), d, serial) {
            return Err(generic());
        }
        {
            let _s = state.write();
            d.model.lock().expect("workflow details model").workflow = Some(raw.clone());
        }
        Some(Workflow::decode(id, raw).map_err(failure)?)
    } else {
        None
    };
    Projection::new(&trigger, workflow.as_ref()).map_err(failure)
}
async fn request(
    mut state: Signal<ShellState>,
    d: &Arc<Selection>,
    serial: u64,
    read: Read,
) -> Result<Value, Error> {
    if !admitted(&state.peek(), d, serial)
        || cooling(&state.peek()).blocked
        || !native::current(d, "").await
        || !admitted(&state.peek(), d, serial)
    {
        return Err(unavailable());
    }
    let req = read.request(&d.owner.ctx.client, &d.owner.ctx.session);
    let result = tokio::select! {
        result=async {
            if crate::fixture::is_fixture(){fixture::read(state,&req).await.unwrap_or(Err(unavailable()))}
            else{d.owner.ctx.transport.execute(req).await}
        }=>result,
        _=async{loop{tokio::time::sleep(Duration::from_millis(50)).await;if !admitted(&state.peek(),d,serial){break;}}}=>return Err(unavailable()),
    };
    if !admitted(&state.peek(), d, serial) {
        return Err(unavailable());
    }
    let mut s = state.write();
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        if let Some(scope) = cooldown_scope(&s) {
            s.channel_workflows
                .record(scope, read.method(), *retry_after_secs, Instant::now());
        }
    }
    let receipt = match &result {
        Ok(raw) => json!({"serial":serial,"method":read.method(),"raw":raw}),
        Err(Error::RateLimited { retry_after_secs }) => {
            json!({"serial":serial,"method":read.method(),"failure":"rate-limited","retryAfterSeconds":retry_after_secs})
        }
        Err(Error::Api(code)) => {
            json!({"serial":serial,"method":read.method(),"failure":"api","code":code})
        }
        Err(_) => json!({"serial":serial,"method":read.method(),"failure":"read failed"}),
    };
    let mut m = d.model.lock().expect("workflow details model");
    if m.reads.len() == 64 {
        m.reads.remove(0);
    }
    m.reads.push(receipt);
    result
}
fn unavailable() -> Error {
    Error::Api("channel_workflow_details_owner_unavailable".into())
}
