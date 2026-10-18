//! Cooperative panel departure consumes the final original-node capture. Other
//! Canvas departure guards keep a mounted rich editor charged until this step.
use super::*;
fn saved(e: &Entry) -> bool {
    // A never-edited empty plain fixture/readonly panel has no local tail.
    if e.serial == 0
        && e.text.is_empty()
        && e.blocks.is_none()
        && e.inventory.as_ref().is_some_and(|i| i.drafts().is_empty())
    {
        return true;
    }
    if retired(e) && e.locked.is_none() && e.outcome.is_none() {
        return true;
    }
    e.inventory.as_ref().is_some_and(|i| {
        i.drafts().iter().any(|d| {
            d.capture() == e.capture && d.text() == e.text && d.blocks() == e.blocks.as_deref()
        })
    })
}
pub(in crate::canvas::comments) fn back_to_canvas(
    mut state: Signal<ShellState>,
    mut model: Signal<State>,
    mut opened: Signal<bool>,
) {
    let binding = {
        let shell = state.peek();
        let value = model.peek();
        let Some(Query::Replies(root)) = &value.query else {
            drop(value);
            drop(shell);
            model.write().next_serial();
            opened.set(false);
            return;
        };
        let Some(owner) = value.owner.clone().filter(|o| o.current(&shell)) else {
            return;
        };
        let Some(disk) = Disk::capture(&shell, &owner) else {
            return;
        };
        let binding = shell
            .canvas_checklist
            .comment_replies
            .entries
            .iter()
            .find(|e| e.disk == disk && e.root == *root)
            .map(|e| Binding {
                id: e.id.clone(),
                owner,
                read: value.serial,
                root: root.clone(),
            });
        if binding.is_none() {
            drop(value);
            drop(shell);
            model.write().next_serial();
            opened.set(false);
            return;
        }
        binding
    };
    let Some(binding) = binding else { return };
    let needs_finish = {
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.sending || e.native_busy || e.loading || e.composing {
            return;
        }
        e.native_busy = true;
        e.native.is_some()
    };
    dioxus::core::spawn_forever(async move {
        let captured = if needs_finish {
            native::finish(state, &binding).await.is_ok()
        } else {
            true
        };
        let settled = if captured {
            tokio::time::timeout(std::time::Duration::from_secs(6), async {
                loop {
                    let ready = {
                        let shell = state.peek();
                        if !binding.current(&shell, &model.peek()) {
                            return false;
                        }
                        let Some(e) = entry(&shell, &binding.id) else {
                            return false;
                        };
                        if e.native.is_none() && !e.saving {
                            Some(saved(e))
                        } else {
                            None
                        }
                    };
                    if let Some(done) = ready {
                        return done;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap_or(false)
        } else {
            false
        };
        {
            let mut shell = state.write();
            let Some(e) = entry_mut(&mut shell, &binding.id) else {
                return;
            };
            e.native_busy = false;
            if !settled {
                e.error = Some(
                    "The final reply draft could not be saved. It is retained here; retry local save before leaving.",
                );
            }
        }
        if settled && binding.current(&state.peek(), &model.peek()) {
            let mut value = model.write();
            value.next_serial();
            value.loading = false;
            drop(value);
            opened.set(false);
        } else if captured
            && binding.current(&state.peek(), &model.peek())
            && entry(&state.peek(), &binding.id).is_some_and(|e| e.native.is_none())
        {
            // Final native custody succeeded but disk persistence did not. A
            // new exact mount starts from the retained full blocks, permitting
            // continued editing while the original local error remains visible.
            model.write().next_serial();
        }
    });
}
