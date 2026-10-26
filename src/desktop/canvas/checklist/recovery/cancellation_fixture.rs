//! One named offline gate. Inactive in every signed-in and other fixture path.
use super::*;
use serde_json::{Value, json};
use std::sync::{LazyLock, Mutex};

struct Gate {
    ticket: Ticket,
    attempt: u64,
    boundary: String,
    read: Option<Read>,
    phase: &'static str,
    entered: u32,
    cancelled: u32,
    returned: u32,
    release: Option<tokio::sync::oneshot::Sender<()>>,
    wait: Option<tokio::sync::oneshot::Receiver<()>>,
}
static GATE: LazyLock<Mutex<Option<Gate>>> = LazyLock::new(|| Mutex::new(None));

pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-retirement-cancellation" | "canvas-retirement-cancellation-narrow")
        )
}
pub(in crate::canvas) fn arm(shell: &ShellState, boundary: &str) -> Result<(), String> {
    if !requested()
        || !matches!(boundary, "precommit" | "postcommit")
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
    {
        return Err("Gate is not isolated".into());
    }
    let op = shell
        .canvas_checklist
        .operations
        .iter()
        .find(|op| op.stage == Stage::AwaitingRead)
        .ok_or("No accepted replacement")?;
    let ticket = op.durable.clone().ok_or("No exact durable ticket")?;
    if ticket.owner.path.as_ref().is_none_or(|p| !p.is_absolute()) {
        return Err("No isolated journal".into());
    }
    let mut slot = GATE.lock().unwrap();
    if slot.is_some() {
        return Err("Gate already used".into());
    }
    let (send, receive) = tokio::sync::oneshot::channel();
    *slot = Some(Gate {
        ticket,
        attempt: op.attempt.serial,
        boundary: boundary.into(),
        read: None,
        phase: "armed",
        entered: 0,
        cancelled: 0,
        returned: 0,
        release: Some(send),
        wait: Some(receive),
    });
    Ok(())
}
pub(in crate::canvas) struct Waiter {
    attempt: u64,
    returned: bool,
}
impl Waiter {
    pub(in crate::canvas) fn returned(&mut self) {
        self.returned = true;
    }
}
impl Drop for Waiter {
    fn drop(&mut self) {
        if let Some(g) = GATE
            .lock()
            .unwrap()
            .as_mut()
            .filter(|g| g.attempt == self.attempt)
        {
            if self.returned {
                g.returned += 1;
            } else {
                g.cancelled += 1;
            }
        }
    }
}
pub(in crate::canvas) fn waiter(shell: &ShellState, read: &Read) -> Option<Waiter> {
    if !requested() || !read.current(shell) {
        return None;
    }
    let mut slot = GATE.lock().unwrap();
    let g = slot
        .as_mut()
        .filter(|g| g.phase == "armed" && g.read.is_none())?;
    let exact = shell.canvas_checklist.operations.iter().any(|op| {
        op.attempt.serial == g.attempt && op.durable.as_ref().is_some_and(|t| t.same(&g.ticket))
    });
    if !exact {
        return None;
    }
    g.read = Some(read.clone());
    Some(Waiter {
        attempt: g.attempt,
        returned: false,
    })
}
pub(super) async fn pause(state: Signal<ShellState>, ticket: &Ticket, boundary: &str) {
    if !requested() {
        return;
    }
    let receive = {
        let shell = state.peek();
        let mut slot = GATE.lock().unwrap();
        let Some(g) = slot
            .as_mut()
            .filter(|g| g.phase == "armed" && g.boundary == boundary && g.ticket.same(ticket))
        else {
            return;
        };
        if !g.read.as_ref().is_some_and(|r| {
            r.current(&shell)
                && shell.canvas_checklist.operations.iter().any(|op| {
                    op.attempt.serial == g.attempt
                        && op.durable_finishing
                        && op.durable_verified == Some(r.serial)
                        && op.durable.as_ref().is_some_and(|t| t.same(ticket))
                })
        }) {
            return;
        }
        g.phase = "held";
        g.entered += 1;
        g.wait.take()
    };
    if let Some(receive) = receive {
        let _ = receive.await;
    }
}
pub(in crate::canvas) fn release() -> Result<(), String> {
    let mut slot = GATE.lock().unwrap();
    let g = slot
        .as_mut()
        .filter(|g| g.phase == "held" && g.entered == 1 && g.cancelled == 1 && g.returned == 0)
        .ok_or("Exact caller cancellation not witnessed")?;
    let send = g.release.take().ok_or("Gate already released")?;
    g.phase = "released";
    send.send(())
        .map_err(|_| "Retained settlement was cancelled".into())
}
pub(in crate::canvas) fn proof(shell: &ShellState) -> Value {
    let slot = GATE.lock().unwrap();
    let Some(g) = slot.as_ref() else {
        return Value::Null;
    };
    let op = shell
        .canvas_checklist
        .operations
        .iter()
        .find(|op| op.attempt.serial == g.attempt);
    json!({"attempt":g.attempt,"operation":g.ticket.receipt.operation_id(),"boundary":g.boundary,
        "phase":g.phase,"entered":g.entered,"cancelled":g.cancelled,"returned":g.returned,
        "read":g.read.as_ref().map(|r|json!({"file":r.file,"visit":r.visit,"serial":r.serial,"revision":r.revision})),
        "operationPresent":op.is_some(),"durableFinishing":op.is_some_and(|o|o.durable_finishing),
        "ticketCurrent":op.and_then(|o|o.durable.as_ref()).is_some_and(|t|t.same(&g.ticket)),
        "viewPresent":shell.canvas_checklist.view.is_some()})
}
