//! Real Discovery flow; gates delay the caller only, never fabricate a receipt.
use super::*;
use crate::view::composer::protected_enrollment as native;
use std::rc::Rc;

pub(super) struct Dialog {
    pub id: String,
    pub target: ComposerTarget,
    pub visit: Visit,
    pub capture: RefCell<Option<native::Capture>>,
    pub views: Vec<View>,
    pub pending: Vec<LocalDraft>,
}
#[derive(Default)]
pub(super) struct Panel {
    pub dialog: Option<Rc<Dialog>>,
    pub busy: bool,
    // Async Host mutations do not themselves invalidate a Dioxus signal.
    pub publication: u64,
    pub message: Option<String>,
    pub adopting: bool,
}
pub(super) fn publish(mut ui: Signal<Panel>) {
    let mut panel = ui.write();
    panel.publication = panel
        .publication
        .checked_add(1)
        .expect("fixture publication exhausted");
}
fn visible(s: &ShellState, target: ComposerTarget) -> LocalDraft {
    match target {
        ComposerTarget::Channel => LocalDraft {
            composer: s.core.composer.clone(),
            attachments: s.core.composer_attachments.clone(),
        },
        ComposerTarget::Thread => LocalDraft {
            composer: s.core.thread_composer.clone(),
            attachments: s.core.thread_composer_attachments.clone(),
        },
    }
}
pub(super) fn open(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if ui.peek().busy || ui.peek().dialog.is_some() {
        return;
    }
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    ui.write().busy = true;
    b.record(json!({"kind":"open-click"}));
    let task = spawn(Box::pin(async move {
        let result=async {
            b.reload(state).await?;
            let (views,pending,local)={let s=state.peek(); if !b.current(&s){return Err("Draft owner changed".to_owned());}
                let d=b.host.lock().unwrap();
                let views=d.discovery().keys(&s).ok_or("Current inventory missing")?.iter().map(|key|d.discovery().entry(&s,key).cloned().ok_or("Whole inventory changed".to_owned())).collect::<Result<Vec<_>,_>>()?;
                let pending=d.discovery().retained(&s).map_err(err)?.iter().filter_map(|e|e.snapshot().pending).collect::<Vec<_>>();
                (views,pending,visible(&s,b.target()))};
            let capture=native::capture(state,b.target(),&local).await.map_err(err)?;
            let dialog=Rc::new(Dialog {id:barrier_id(),target:b.target(),visit:Visit::new(),capture:RefCell::new(Some(capture)),views,pending});
            let s=state.peek(); if !b.current(&s){return Err("Draft owner changed".into());}
            b.record(json!({"kind":"dialog","id":dialog.id,"displayed":inventory::displayed(&s,&b),"pending":dialog.pending}));
            Ok(dialog)
        }.await;
        let mut panel = ui.write();
        panel.busy = false;
        match result {
            Ok(dialog) => {
                panel.message = None;
                panel.dialog = Some(dialog);
            }
            Err(error) => {
                panel.message = Some(format!("Cannot review this draft: {error}"));
                b.result(json!({"kind":"open-refused","error":error}));
            }
        }
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}
pub(super) fn submit(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if ui.peek().busy {
        return;
    }
    let Some(dialog) = ui.peek().dialog.clone() else {
        return;
    };
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    let Some(capture) = dialog.capture.borrow_mut().take() else {
        return;
    };
    b.record(json!({"kind":"consent-click","id":dialog.id}));
    let request = {
        let s = state.peek();
        b.host
            .lock()
            .unwrap()
            .enrollment(&s, &key(b.target()), dialog.visit.clone())
    };
    let request = match request {
        Ok(value) => value,
        Err(error) => {
            dialog.visit.retire();
            ui.write().dialog = None;
            ui.write().message = Some(refusal(error).into());
            b.result(json!({"kind":"refused","phase":"capture","error":err(error)}));
            return;
        }
    };
    ui.write().busy = true;
    let task = spawn(Box::pin(async move {
        let result=async {
            let expected=request.expected().clone();
            let prepared=request.prepare().await?;
            b.record(json!({"kind":"prepared","expected":super::super::native_fixture::local_json(&expected)}));
            let admission=capture.lock().await?;
            let ready={let s=state.try_peek().map_err(|_|durable::Error::Stale)?;
                let mut d=b.host.lock().unwrap();
                d.enqueue_enrollment(prepared,&s,&admission)?};
            b.record(json!({"kind":"enqueue","expected":super::super::native_fixture::local_json(&expected)}));
            // Enrollment does not clear a native field. The exact consent was
            // spent at enqueue; release only our temporary readOnly lease.
            drop(admission);
            ready.ready().await;
            let (outcome,current)={let s=state.peek();let mut d=b.host.lock().unwrap();let outcome=d.take_enrollment(&ready)?;let current=outcome.current(d.discovery(),&s);(outcome,current)};
            let review=outcome.result()?;
            Ok(json!({"kind":"enrolled","current":current,"account":outcome.account(),"expected":super::super::native_fixture::local_json(outcome.expected()),"review":super::super::native_fixture::row_json(review)}))
        }.await;
        let mut panel = ui.write();
        panel.busy = false;
        panel.dialog = None;
        match result {
            Ok(value) => {
                panel.message = Some(
                    "Enrollment finished. Reload saved drafts to see the current result.".into(),
                );
                b.result(value);
            }
            Err(error) => {
                panel.message = Some(refusal(error).into());
                b.result(json!({"kind":"refused","phase":"prepare-or-admit","error":err(error)}));
            }
        }
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}
fn refusal(error: durable::Error) -> &'static str {
    match error {
        durable::Error::ReviewRequired => {
            "This draft has changed or has input to recover. Review it again before enabling recovery."
        }
        _ => "The draft or its location changed. Reload saved drafts and review it again.",
    }
}
pub(super) fn cancel(mut ui: Signal<Panel>) {
    if ui.peek().adopting {
        super::adoption::cancel(ui);
        return;
    }
    let Some(b) = BINDING.get() else {
        return;
    };
    if let Some(dialog) = ui.peek().dialog.as_ref() {
        dialog.visit.retire();
    }
    if let Some(task) = WAITER.with_borrow_mut(Option::take) {
        task.cancel();
    }
    let pending = b.host.lock().unwrap().pending_enrollment().is_some();
    {
        let mut panel = ui.write();
        panel.dialog = None;
        panel.busy = false;
        panel.message = Some(
            if pending {
                "Stopped waiting. Check status to recover the enrollment result."
            } else {
                "Enrollment canceled."
            }
            .into(),
        );
    }
    b.record(json!({"kind":"cancel-click","pending":pending}));
    b.result(json!({"kind":"canceled","pending":pending}));
}
pub(super) fn check(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if BINDING
        .get()
        .is_some_and(|b| b.adoption.lock().unwrap().is_some())
    {
        super::adoption::check(state, ui);
        return;
    }
    if ui.peek().busy {
        return;
    }
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    let Some(ready) = b.host.lock().unwrap().pending_enrollment() else {
        return;
    };
    ui.write().busy = true;
    *b.result.lock().unwrap() = None;
    b.record(json!({"kind":"check-click"}));
    let task = spawn(Box::pin(async move {
        ready.ready().await;
        let result = {
            let s = state.peek();
            let mut d = b.host.lock().unwrap();
            d.take_enrollment(&ready).and_then(|outcome|{
            let review=outcome.result()?;
            Ok(json!({"kind":"enrolled","current":outcome.current(d.discovery(),&s),"account":outcome.account(),"expected":super::super::native_fixture::local_json(outcome.expected()),"review":super::super::native_fixture::row_json(review)}))})
        };
        let mut panel = ui.write();
        panel.busy = false;
        match result {
            Ok(value) => {
                panel.message=Some("The earlier enrollment finished. Reload saved drafts to see the current result.".into());
                b.result(value);
            }
            Err(error) => {
                panel.message = Some(refusal(error).into());
                b.result(json!({"kind":"refused","phase":"check","error":err(error)}));
            }
        }
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}
pub(super) fn reload(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if ui.peek().busy {
        return;
    }
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    ui.write().busy = true;
    b.record(json!({"kind":"reload-click"}));
    let task = spawn(Box::pin(async move {
        let result = b.reload(state).await;
        let mut panel = ui.write();
        panel.busy = false;
        panel.message = Some(match result {
            Ok(()) => "Saved drafts reloaded.".into(),
            Err(error) => format!("Cannot reload saved drafts: {error}"),
        });
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}
