//! Original plain initialization and Save observations remain alive until the
//! actual issuance check; neither is an inert restored-draft authority.
use super::*;

pub(super) struct Proof<'a> {
    initial: plain_observations::Captured,
    unchanged: bool,
    effects: Option<bridge::NoAttachmentEffects>,
    prepared: plain_save::Captured<'a>,
}
impl Proof<'_> {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.initial.current(shell) && self.prepared.current(shell)
    }
    pub(super) fn unchanged(&self, shell: &ShellState, binding: &Binding) -> bool {
        self.unchanged
            && self.current(shell)
            && self
                .effects
                .as_ref()
                .is_some_and(|e| e.current(shell, binding))
    }
    fn returning_current(&self, shell: &ShellState) -> bool {
        self.initial.current(shell) && self.prepared.returning_current(shell)
    }
}
pub(super) async fn prepare<'a>(
    state: Signal<ShellState>,
    binding: &Binding,
    serial: u64,
    seed: &OwnEditSeed,
    delta: &PlainDelta,
    loaded: &network::Loaded,
    policy: &'a own_edit_policy::Observation,
    effects: Option<bridge::NoAttachmentEffects>,
) -> Result<(Option<OwnEditIntent>, Proof<'a>), Error> {
    let original = super::super::super::plain::original(&loaded.page, &binding.source.ts)?;
    let initial =
        plain_observations::capture(state, &binding.source, serial, policy, &original).await?;
    let mut prepared = plain_save::capture(state, binding, serial, policy, delta).await?;
    let shell = state.peek();
    if !initial.current(&shell) || !prepared.current(&shell) || !suspension_current(&shell, binding)
    {
        return Err(unavailable());
    }
    let known = selected(&loaded.page, &binding.source.ts)?;
    seed.revalidate_plain_initialization(&initial.seed)
        .map_err(|_| unavailable())?;
    seed.revalidate(&loaded.page, known, policy.policy(), millis())
        .map_err(|_| unavailable())?;
    if delta.ops() != prepared.prepared.original_ops() {
        return Err(unavailable());
    }
    let unchanged = if known.files.is_empty()
        && known.attachments.is_empty()
        && effects.as_ref().is_some_and(|e| e.current(&shell, binding))
    {
        // This installed editor has no attachment-side-effect producers. Its
        // original native capability plus admitted attachmentless source proves
        // the bounded qszY undefined branch; this is not an unavailable default.
        prepared.unfurled = Some(Value::Null);
        prepared
            .prepared
            .comparison(initial.seed.delta_ops(), &Value::Null)
            .map_err(|_| unavailable())?
            .unchanged()
    } else {
        false
    };
    // A no-op creates no mutation intent, request or journal attempt.
    let intent = if unchanged {
        None
    } else {
        Some(
            OwnEditIntent::capture_plain(
                seed,
                &initial.seed,
                &loaded.page,
                known,
                policy.policy(),
                delta,
                &prepared.prepared,
                prepared.headers,
                &prepared.encoding,
                millis(),
            )
            .map_err(|e| Error::Api(e.to_string()))?,
        )
    };
    Ok((
        intent,
        Proof {
            initial,
            prepared,
            unchanged,
            effects,
        },
    ))
}

/// Read authority stays alive through native finish and expected-inventory
/// cleanup. After finish consumes the editor, only owner/content/policy facts
/// are checked; the consumed native ticket is not recaptured from a successor.
pub(super) async fn finish_noop(
    mut state: Signal<ShellState>,
    binding: &Binding,
    serial: u64,
    lease: bridge::Lease,
    proof: &Proof<'_>,
) {
    if !proof.unchanged(&state.peek(), binding) {
        failed_preparation(state, binding, serial, &unavailable());
        return;
    }
    restore_checked(state, binding, lease, &|shell| {
        proof.returning_current(shell) && suspension_current(shell, binding)
    })
    .await;
    publish::finish_read(&mut state.write(), &binding.source.owner, serial, None);
}
