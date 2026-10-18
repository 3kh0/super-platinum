//! Human removal of an unknown-outcome lock; never a delivery acknowledgement.
use super::*;
use super_platinum_core::canvas::edit::structure::recovery::journal::{
    CommentDecision, CommentReview,
};
pub(in crate::canvas) struct Draft {
    id: u64,
    owner: Owner,
    serial: u64,
    receipt: CommentReceipt,
    disk: Disk,
    prepared: Option<CommentReview>,
    busy: bool,
    checked: bool,
    observed: Option<bool>,
    error: Option<&'static str>,
}
pub(in crate::canvas::comments) fn available(
    shell: &ShellState,
    model: &super::super::State,
) -> bool {
    let Some(owner) = model.owner.as_ref().filter(|owner| owner.current(shell)) else {
        return false;
    };
    !model.cooling()
        && super::super::rate::current(shell).is_none()
        && model.mutation_receipt.is_some()
        && index(shell, owner).is_none_or(|index| {
            shell.canvas_checklist.comment_operations.operations[index].stage == Stage::Uncertain
        })
}
pub(in crate::canvas::comments) fn open(
    state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
) {
    let request = {
        let shell = state.peek();
        let current = model.peek();
        if !available(&shell, &current) || current.review.as_ref().is_some_and(|draft| draft.busy) {
            return;
        }
        let owner = current.owner.clone().unwrap();
        let Some(disk) = Disk::capture(&shell, &owner) else {
            return;
        };
        (
            owner,
            current.serial,
            current.mutation_receipt.clone().unwrap(),
            disk,
        )
    };
    let review_id = {
        let mut value = model.write();
        value.review_serial = value.review_serial.wrapping_add(1);
        value.review_serial
    };
    model.write().review = Some(Draft {
        id: review_id,
        owner: request.0.clone(),
        serial: request.1,
        receipt: request.2.clone(),
        disk: request.3.clone(),
        prepared: None,
        busy: true,
        checked: false,
        observed: None,
        error: None,
    });
    dioxus::core::spawn_forever(async move {
        let ticket = Ticket {
            owner: request.0.clone(),
            serial: request.1,
            query: Query::Replies(request.2.record().root_ts().into()),
            cursor: None,
            direction: Direction::First,
        };
        let result = super::super::read(state, &ticket).await;
        let result = match result {
            Ok((Page::Replies(page), file)) => {
                if let Some(root) = page
                    .messages
                    .into_iter()
                    .find(|root| root.ts.as_deref() == Some(request.2.record().root_ts()))
                {
                    let observed = root
                        .extra
                        .get("document_comment")
                        .and_then(|v| v.get("is_archived"))
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let receipt = request.2.clone();
                    request
                        .3
                        .work(move |journal| journal.review_comment(&receipt, &file, &root))
                        .await
                        .map(|review| (review, observed))
                } else {
                    Err(())
                }
            }
            Err(Error::RateLimited { retry_after_secs }) => {
                if request.0.current(&state.peek()) {
                    super::super::rate::remember(&state.peek(), retry_after_secs);
                    let wait = super::super::rate::current(&state.peek());
                    if let Some(wait) = wait {
                        super::super::set_wait(state, model, request.0.clone(), wait);
                    }
                }
                Err(())
            }
            _ => Err(()),
        };
        if !request.0.current(&state.peek()) || model.peek().serial != request.1 {
            return;
        }
        let mut value = model.write();
        let Some(draft) = value.review.as_mut().filter(|draft| {
            draft.id == review_id
                && draft.receipt == request.2
                && draft.serial == request.1
                && draft.busy
        }) else {
            return;
        };
        draft.busy = false;
        match result {
            Ok((review, observed)) => {
                draft.prepared = Some(review);
                draft.observed = Some(observed);
            }
            Err(()) => {
                draft.error = Some("The thread could not be checked. The local lock is retained.")
            }
        }
    });
}
fn confirm(
    mut state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
    expected: &CommentReceipt,
    expected_serial: u64,
    expected_review: u64,
) {
    let pending = {
        let shell = state.peek();
        let mut current = model.write();
        if !available(&shell, &current) || current.serial != expected_serial {
            return;
        }
        let serial = current.serial;
        let Some(draft) = current.review.as_mut().filter(|draft| {
            draft.id == expected_review
                && !draft.busy
                && draft.checked
                && draft.serial == serial
                && &draft.receipt == expected
                && draft.owner.current(&shell)
        }) else {
            return;
        };
        let Some(prepared) = draft.prepared.take() else {
            return;
        };
        draft.busy = true;
        (
            draft.owner.clone(),
            draft.serial,
            draft.receipt.clone(),
            draft.disk.clone(),
            prepared,
        )
    };
    dioxus::core::spawn_forever(async move {
        let done = pending
            .3
            .work(move |journal| {
                journal.resolve_comment_locally(
                    pending.4,
                    CommentDecision::RemoveLocalLockOutcomeUnknown,
                )
            })
            .await;
        if done == Ok(true) {
            let mut shell = state.write();
            shell
                .canvas_checklist
                .comment_operations
                .operations
                .retain(|operation| {
                    operation.receipt.as_ref() != Some(&pending.2)
                        || operation.attempt.disk != pending.3
                });
        }
        if !pending.0.current(&state.peek()) || model.peek().serial != pending.1 {
            return;
        }
        let mut value = model.write();
        if value
            .review
            .as_ref()
            .is_none_or(|draft| draft.id != expected_review || draft.receipt != pending.2)
        {
            return;
        }
        if done == Ok(true) {
            value.review = None;
            value.mutation_receipt = None;
            value.error = None;
        } else if let Some(draft) = value.review.as_mut() {
            draft.busy = false;
            draft.error = Some(
                "The local record could not be updated. Close this review and check again; no request was sent.",
            );
        }
    });
}

pub(in crate::canvas::comments) fn view(
    state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
) -> Element {
    let current = model.read();
    let Some(draft) = current
        .review
        .as_ref()
        .filter(|draft| draft.owner.current(&state.read()) && draft.serial == current.serial)
    else {
        return rsx! {};
    };
    let busy = draft.busy;
    let serial = draft.serial;
    let id = draft.id;
    let checked_receipt = draft.receipt.clone();
    rsx! {section{class:"canvas-comment-review",role:"region","aria-label":"Review unconfirmed thread change",
        h3{"Review unconfirmed thread change"}
        p{if draft.receipt.record().desired(){"Requested: resolve thread. "}else{"Requested: re-open thread. "}
          if let Some(archived)=draft.observed {if archived{"The thread is currently resolved."}else{"The thread is currently open."}}}
        p{"This does not prove delivery. Removing the local lock retains the original receipt and never resends it."}
        if let Some(error)=draft.error {p{role:"alert","{error}"}}
        label{input{id:"canvas-comment-review-check",r#type:"checkbox",disabled:busy,checked:draft.checked,onchange:move|event|{
            let shell=state.peek(); let mut current=model.write(); if current.serial==serial {if let Some(draft)=current.review.as_mut().filter(|draft|draft.id==id && draft.receipt==checked_receipt && draft.owner.current(&shell)){if !draft.busy{draft.checked=event.checked();}}}
        }}"I reviewed this thread and want to allow a new deliberate change."}
    }}
}
pub(in crate::canvas::comments) fn actions(
    state: Signal<ShellState>,
    mut model: Signal<super::super::State>,
) -> Element {
    let current = model.read();
    let Some(draft) = current
        .review
        .as_ref()
        .filter(|draft| draft.owner.current(&state.read()) && draft.serial == current.serial)
    else {
        return rsx! {};
    };
    let busy = draft.busy;
    let serial = draft.serial;
    let id = draft.id;
    let canceled_receipt = draft.receipt.clone();
    let confirmed_receipt = draft.receipt.clone();
    let disabled = busy || !draft.checked || draft.prepared.is_none();
    rsx! {div{class:"canvas-comments-toolbar",
        button{id:"canvas-comment-review-cancel","data-canvas-comment-action":"review-cancel",disabled:busy,onclick:move |_|{
            let shell=state.peek();let mut current=model.write();if current.serial==serial && current.review.as_ref().is_some_and(|draft|draft.id==id && !draft.busy && draft.receipt==canceled_receipt && draft.owner.current(&shell)){current.review=None;}
        },"Cancel review"}
        button{id:"canvas-comment-review-confirm","data-canvas-comment-action":"review-confirm",disabled,onclick:move |_|confirm(state,model,&confirmed_receipt,serial,id),"Remove local lock"}
    }}
}
