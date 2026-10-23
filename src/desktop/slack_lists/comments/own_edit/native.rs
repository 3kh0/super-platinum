use super::*;
const RECOVERING: &str = "Recovering native edit capture… Both drafts are retained.";
pub(super) fn can_retry_capture(shell: &ShellState, binding: &Binding) -> bool {
    binding.visible_mount(shell)
        && session(shell, binding).is_some_and(|s| {
            !s.busy
                && s.capture_failed
                && s.native
                    .as_ref()
                    .is_some_and(|m| m.matches(binding) && m.failed())
        })
}
pub(crate) fn retry_capture(state: Signal<ShellState>, binding: Binding) {
    if !can_retry_capture(&state.peek(), &binding) {
        return;
    }
    // The existing bridge retains the original exact node and opaque blocks.
    // Recovery never mounts a replacement or refreshes action authority.
    bridge::retry(state, binding);
}
pub(crate) fn install_native(
    shell: &mut ShellState,
    binding: &Binding,
    mounted: bridge::Mounted,
) -> bool {
    if !binding.visible_mount(shell) && !binding.retained_mount(shell) {
        return false;
    }
    if session(shell, binding).is_some_and(|s| !titles::readiness(shell, s).0 || s.finishing) {
        return false;
    }
    let Some(s) = session_mut(shell, binding) else {
        return false;
    };
    let recovery = s
        .native
        .as_ref()
        .is_some_and(|old| old.failed() && old.mount() == mounted.mount());
    if !mounted.matches(binding) || (s.native.is_some() && !recovery) || s.busy {
        return false;
    }
    s.native = Some(mounted);
    s.native_ready = false;
    s.capture_failed = recovery;
    if recovery {
        s.notice = Some(RECOVERING.into());
    }
    if !recovery {
        s.native_sequence = 0;
    }
    true
}
pub(crate) fn native(shell: &ShellState, binding: &Binding) -> Option<bridge::Mounted> {
    session(shell, binding)?.native.clone()
}
pub(crate) fn retain_capture(
    shell: &mut ShellState,
    binding: &Binding,
    mount: &str,
    sequence: u64,
    blocks: Vec<Value>,
    composing: bool,
    detached: bool,
) -> Result<(), Error> {
    retain_content(
        shell,
        binding,
        mount,
        sequence,
        Content::RichBlocks(blocks),
        composing,
        false,
        detached,
    )
}

pub(crate) fn retain_content(
    shell: &mut ShellState,
    binding: &Binding,
    mount: &str,
    sequence: u64,
    content: Content,
    composing: bool,
    pending: bool,
    detached: bool,
) -> Result<(), Error> {
    let visible = binding.visible_mount(shell);
    let s = session_mut(shell, binding).ok_or_else(unavailable)?;
    let previous = s.content()?;
    let values = match &content {
        Content::RichBlocks(blocks) => blocks.as_slice(),
        Content::PlainDelta(delta) => {
            delta.validate().map_err(|_| unavailable())?;
            delta.ops()
        }
    };
    if s.native.as_ref().is_none_or(|m| m.mount() != mount)
        || sequence <= s.native_sequence
        || sequence > 9_007_199_254_740_991
        || content.representation() != previous.representation()
        || serde_json::to_vec(values).map_err(|_| unavailable())?.len() > 512 * 1024
    {
        return Err(unavailable());
    }
    if previous != content && s.busy && (visible || !detached) {
        return Err(unavailable());
    }
    // Validate the next revision before mutating any capture custody.
    let revision = if previous != content {
        s.revision.checked_add(1).ok_or_else(unavailable)?
    } else {
        s.revision
    };
    s.native_sequence = sequence;
    s.composing = composing;
    s.native_pending = pending;
    if previous != content {
        match content {
            Content::RichBlocks(blocks) => s.blocks = blocks,
            Content::PlainDelta(delta) => s.plain = Some(delta),
        }
        s.revision = revision;
        if !matches!(
            attempt_phase(s),
            Some(Phase::Unknown | Phase::Confirmed | Phase::Observed)
        ) {
            s.notice = None;
        }
    }
    s.native_ready = !detached;
    s.capture_failed = false;
    if s.notice.as_deref() == Some(RECOVERING) {
        s.notice=match attempt_phase(s) {
            Some(Phase::Unknown)=>Some("The edit may have reached Slack. Check the result before saving again; both drafts are retained.".into()),
            Some(Phase::Confirmed|Phase::Observed)=>Some("The save result is retained. Cancel to return to your reply draft.".into()),
            _=>None,
        };
    }
    Ok(())
}
pub(crate) fn native_done(shell: &mut ShellState, binding: &Binding, mount: &str, success: bool) {
    let Some(s) = session_mut(shell, binding)
        .filter(|s| s.native.as_ref().is_some_and(|m| m.mount() == mount))
    else {
        return;
    };
    s.native_ready = false;
    if success {
        s.native = None;
        s.composing = false;
        s.native_pending = false;
        s.capture_failed = false;
    } else {
        s.capture_failed = true;
        s.notice = Some(
            "The original edit capture stopped. Retry capture before saving or leaving.".into(),
        );
    }
}
