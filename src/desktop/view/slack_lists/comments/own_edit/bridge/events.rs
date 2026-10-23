//! Native UI intents and finite clipboard work remain subordinate to the exact
//! successfully retained capture. They cannot advance the draft ACK themselves.
use super::*;
use crate::slack_lists::comments::delta_editor::{clipboard_host, clipboard_protocol::Request};

fn stamp(mounted: &Mounted, sequence: u64, revision: u64, pending: bool) -> bool {
    mounted.representation == "plain-delta"
        && !mounted.failed()
        && mounted.stamp.lock().ok().and_then(|v| *v).is_some_and(|s| {
            s.sequence == sequence
                && s.revision == revision
                && s.pending == pending
                && !s.composing
                && !s.detached
                && mounted.sequence.load(Ordering::SeqCst) == sequence
        })
}

fn current(shell: &ShellState, binding: &Binding, mounted: &Mounted) -> bool {
    binding.current(shell)
        && binding.representation(shell) == Some("plain-delta")
        && controller::native(shell, binding)
            .is_some_and(|m| m.mount == mounted.mount && m.matches(binding) && !m.failed())
}

pub(super) fn action(
    state: Signal<ShellState>,
    binding: &Binding,
    mounted: &Mounted,
    value: &Value,
) -> Result<(), Error> {
    let sequence = value["sequence"].as_u64().ok_or_else(unavailable)?;
    let revision = value["revision"].as_u64().ok_or_else(unavailable)?;
    let action = value["action"]
        .as_str()
        .filter(|v| matches!(*v, "save" | "cancel"))
        .ok_or_else(unavailable)?;
    if value["generation"].as_u64() != Some(mounted.generation)
        || !stamp(mounted, sequence, revision, false)
        || !current(&state.peek(), binding, mounted)
    {
        return Ok(());
    }
    // The controller applies its normal Save/Cancel capability and original
    // suspension checks, then obtains its own exact-node capture lease.
    match action {
        "save" => controller::save(state, binding.clone()),
        "cancel" => controller::cancel(state, binding.clone()),
        _ => unreachable!(),
    }
    Ok(())
}

struct Pending {
    request: Request,
    sequence: u64,
    revision: u64,
    task: dioxus::core::Task,
}
pub(super) struct Completion {
    id: String,
    result: Result<clipboard_host::Prepared, Error>,
}
pub(super) struct Clipboard {
    active: Option<Pending>,
    sender: mpsc::UnboundedSender<Completion>,
    receiver: mpsc::UnboundedReceiver<Completion>,
}
impl Clipboard {
    pub(super) fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            active: None,
            sender,
            receiver,
        }
    }
    pub(super) fn start(
        &mut self,
        state: Signal<ShellState>,
        binding: &Binding,
        mounted: &Mounted,
        value: Value,
    ) -> Result<Option<Value>, Error> {
        let request = Request::decode(value).ok_or_else(unavailable)?;
        let original = mounted
            .stamp
            .lock()
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        if !request.matches(
            &mounted.scope,
            &mounted.mount,
            mounted.generation,
            original.sequence,
            original.revision,
        ) || !stamp(mounted, original.sequence, original.revision, true)
            || !current(&state.peek(), binding, mounted)
        {
            return Ok(Some(request.rejection()));
        }
        if self
            .active
            .as_ref()
            .is_some_and(|p| p.request.request_id() == request.request_id())
        {
            return Ok(None);
        }
        self.stop();
        let owner = mounted.clone();
        let binding = binding.clone();
        let held = request.clone();
        let sender = self.sender.clone();
        let task = dioxus::core::spawn_forever(async move {
            let current = || {
                stamp(&owner, original.sequence, original.revision, true)
                    && current(&state.peek(), &binding, &owner)
            };
            // Canonical clipboard semantic selectors are not installed yet.
            // Keep this separate, cancellable dispatch boundary; no network,
            // empty rows or negative validity is fabricated for reached IDs.
            let result =
                clipboard_host::dispatch(held.clone(), current, async { Err(unavailable()) }).await;
            let _ = sender.send(Completion {
                id: held.request_id().to_owned(),
                result,
            });
        });
        self.active = Some(Pending {
            request,
            sequence: original.sequence,
            revision: original.revision,
            task,
        });
        Ok(None)
    }
    pub(super) async fn next(&mut self) -> Option<Completion> {
        self.receiver.recv().await
    }
    pub(super) fn finish(
        &mut self,
        state: Signal<ShellState>,
        binding: &Binding,
        mounted: &Mounted,
        done: Option<Completion>,
    ) -> Result<Option<Value>, Error> {
        let done = done.ok_or_else(unavailable)?;
        if self
            .active
            .as_ref()
            .is_none_or(|p| p.request.request_id() != done.id)
        {
            return Ok(None);
        }
        let pending = self.active.take().unwrap();
        let owned = || {
            stamp(mounted, pending.sequence, pending.revision, true)
                && current(&state.peek(), binding, mounted)
        };
        let receipt = done
            .result
            .and_then(|prepared| prepared.receipt(owned))
            .unwrap_or_else(|_| pending.request.rejection());
        Ok(Some(receipt))
    }
    pub(super) fn cancel(&mut self, value: &Value) {
        if self.active.as_ref().is_some_and(|p| {
            value["request"].as_str() == Some(p.request.request_id())
                && value["sequence"].as_u64() == Some(p.sequence)
                && value["generation"]
                    .as_u64()
                    .map(|g| g.to_string())
                    .as_deref()
                    == Some(p.request.clipboard.generation.as_str())
        }) {
            self.stop();
        }
    }
    pub(super) fn stop(&mut self) {
        if let Some(pending) = self.active.take() {
            pending.task.cancel();
        }
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_action_and_paste_require_exact_durable_capture_stamp() {
        let (mounted, _receiver) = super::super::tests::fixture_mount("plain-delta");
        assert!(!stamp(&mounted, 4, 7, false));
        *mounted.stamp.lock().unwrap() = Some(NativeStamp {
            sequence: 4,
            revision: 7,
            pending: false,
            composing: false,
            detached: false,
        });
        assert!(stamp(&mounted, 4, 7, false));
        assert!(!stamp(&mounted, 4, 7, true));
        assert!(!stamp(&mounted, 3, 7, false));
        assert!(!stamp(&mounted, 4, 8, false));
        mounted.stamp.lock().unwrap().as_mut().unwrap().pending = true;
        assert!(stamp(&mounted, 4, 7, true));
        assert!(!stamp(&mounted, 4, 7, false));
        mounted.stamp.lock().unwrap().as_mut().unwrap().detached = true;
        assert!(!stamp(&mounted, 4, 7, true));
    }
}
