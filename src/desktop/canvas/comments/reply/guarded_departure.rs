//! A file continuation keeps the original editor locked through the native
//! opener. Cancellation releases that entry and remounts its captured tail.
use super::*;

pub(in crate::canvas::comments) struct Departure {
    state: Signal<ShellState>,
    model: Signal<State>,
    binding: Option<Binding>,
    completion: Option<std::sync::Arc<tokio::sync::Notify>>,
    captured: bool,
    committed: bool,
}

impl Departure {
    pub(in crate::canvas::comments) fn current(&self) -> bool {
        let Ok(shell) = self.state.try_read() else {
            return false;
        };
        let Ok(model) = self.model.try_read() else {
            return false;
        };
        self.binding.as_ref().is_none_or(|binding| {
            binding.current(&shell, &model)
                && entry(&shell, &binding.id).is_some_and(|e| {
                    e.native_busy && e.native.is_none() && !e.saving && departure::saved(e)
                })
        })
    }

    pub(in crate::canvas::comments) fn committed(mut self) {
        self.committed = true;
    }
}

impl Drop for Departure {
    fn drop(&mut self) {
        let Some(binding) = &self.binding else {
            return;
        };
        let Ok(mut shell) = self.state.try_write() else {
            return;
        };
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        let resume = !self.committed && self.captured;
        let remount = resume && e.native.is_none();
        // An uncertain finish still owns the bridge. Another finish must not
        // race that request or consume its completion notification.
        e.native_busy = resume && e.native.is_some() && self.completion.is_some();
        if !self.committed {
            e.error
                .get_or_insert("This file could not be opened. Your reply draft is retained here.");
        }
        if remount {
            if let Ok(mut model) = self.model.try_write() {
                if binding.current(&shell, &model) {
                    file_source::remount(&mut model);
                }
            }
        } else if resume {
            // A finish request can time out before its retained bridge completes.
            // Wait for that exact attempt's terminal notification, not a timer or
            // replacement editor. A failed holder remains charged for Retry.
            if let Some(completion) = self.completion.clone() {
                let binding = binding.clone();
                let mut model = self.model;
                drop(shell);
                let mut state = self.state;
                dioxus::core::spawn_forever(async move {
                    completion.notified().await;
                    let Ok(mut shell) = state.try_write() else {
                        return;
                    };
                    let Some(e) = entry_mut(&mut shell, &binding.id) else {
                        return;
                    };
                    e.native_busy = false;
                    let Ok(mut value) = model.try_write() else {
                        return;
                    };
                    if binding.current(&shell, &value)
                        && entry(&shell, &binding.id).is_some_and(|e| e.native.is_none())
                    {
                        file_source::remount(&mut value);
                    }
                });
            }
        }
    }
}

pub(in crate::canvas::comments) async fn prepare(
    mut state: Signal<ShellState>,
    model: Signal<State>,
    current: impl Fn(&ShellState, &State) -> bool,
) -> Result<Departure, ()> {
    let binding = {
        let shell = state.try_read().map_err(|_| ())?;
        let value = model.try_read().map_err(|_| ())?;
        if !current(&shell, &value)
            || annotation::pending_for_current_file(&shell)
            || mutation::pending_for_current_file(&shell)
            || value.review.is_some()
            || value.loading
        {
            return Err(());
        }
        if let Some(Query::Replies(root)) = &value.query {
            let owner = value
                .owner
                .clone()
                .filter(|o| o.current(&shell))
                .ok_or(())?;
            let disk = Disk::capture(&shell, &owner).ok_or(())?;
            shell
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
                })
        } else {
            None
        }
    };
    let needs_finish = if let Some(binding) = &binding {
        let mut shell = state.try_write().map_err(|_| ())?;
        let e = entry_mut(&mut shell, &binding.id).ok_or(())?;
        if e.sending || e.native_busy || e.loading || e.composing || e.inventory.is_none() {
            return Err(());
        }
        e.native_busy = true;
        e.native.is_some()
    } else {
        false
    };
    let completion = binding.as_ref().and_then(|b| native::completion(state, b));
    let mut departure = Departure {
        state,
        model,
        binding,
        completion,
        captured: false,
        committed: false,
    };
    let Some(binding) = departure.binding.clone() else {
        return Ok(departure);
    };
    if needs_finish {
        if departure.completion.is_none() {
            return Err(());
        }
        match native::finish(state, &binding).await {
            Ok(()) => departure.captured = true,
            Err(native::FinishError::Refused) => return Err(()),
            Err(native::FinishError::Uncertain) => {
                departure.captured = true;
                return Err(());
            }
        }
    }
    let settled = tokio::time::timeout(std::time::Duration::from_secs(6), async {
        loop {
            let ready = {
                let shell = state.try_read().map_err(|_| ())?;
                let value = model.try_read().map_err(|_| ())?;
                if !current(&shell, &value) || !binding.current(&shell, &value) {
                    return Err(());
                }
                let e = entry(&shell, &binding.id).ok_or(())?;
                if e.native.is_none() && !e.saving {
                    Some(departure::saved(e))
                } else {
                    None
                }
            };
            if let Some(saved) = ready {
                return saved.then_some(()).ok_or(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| ())?;
    settled?;
    Ok(departure)
}
