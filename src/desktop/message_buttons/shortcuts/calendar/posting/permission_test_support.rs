//! Test-only suspension at the original acquisition boundary. No fake response,
//! permission, authority or source fact; real reads and guards continue afterwards.
use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
struct Gate {
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    entered: Rc<Cell<bool>>,
    receiver: RefCell<Option<tokio::sync::oneshot::Receiver<()>>>,
}
thread_local! {static ACTIVE:RefCell<Option<Rc<Gate>>>=const{RefCell::new(None)};}
pub(in crate::message_buttons::shortcuts::calendar) struct Hold {
    gate: Rc<Gate>,
}
impl Hold {
    pub(in crate::message_buttons::shortcuts::calendar) fn entered(&self) -> bool {
        self.gate.entered.get()
    }
}
impl Drop for Hold {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.as_ref().is_some_and(|v| Rc::ptr_eq(v, &self.gate)) {
                *active = None;
            }
        });
    }
}
pub(in crate::message_buttons::shortcuts::calendar) fn hold(
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
) -> (Hold, tokio::sync::oneshot::Sender<()>) {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let gate = Rc::new(Gate {
        id,
        generation,
        ctx,
        entered: Rc::new(Cell::new(false)),
        receiver: RefCell::new(Some(receiver)),
    });
    ACTIVE.with(|active| {
        assert!(
            active.borrow().is_none(),
            "one owned fixture gate per thread"
        );
        *active.borrow_mut() = Some(gate.clone());
    });
    (Hold { gate }, sender)
}
pub(super) async fn before_acquisition(query: &Query) {
    let receiver = ACTIVE.with(|active| {
        let active = active.borrow();
        let gate = active.as_ref()?;
        if gate.id != query.id
            || gate.generation != query.generation
            || !Arc::ptr_eq(&gate.ctx, &query.ctx)
        {
            return None;
        }
        let receiver = gate.receiver.borrow_mut().take();
        if receiver.is_some() {
            gate.entered.set(true);
        }
        receiver
    });
    if let Some(receiver) = receiver {
        receiver.await.expect("owned permission fixture release");
    }
}
