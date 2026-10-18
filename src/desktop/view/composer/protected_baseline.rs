//! One explicit remote choice under existing Root, exact native lease and
//! guarded projection. This route never dispatches a message or mints evidence.
use super::*;
use crate::drafts::protected::{
    Evidence,
    operation::{Operation, Request, Seed, Status},
};
use super_platinum_core::drafts::store::sync::RemoteChoice;

pub(crate) struct StartBaseline {
    pub state: Signal<ShellState>,
    pub target: ComposerTarget,
    pub registry: Arc<Registry>,
    pub editor: Arc<Editor>,
    pub visit: Visit,
    pub evidence: Arc<Evidence>,
    pub choice: RemoteChoice,
}
pub(crate) struct RefusedBaseline {
    pub error: Error,
    pub request: StartBaseline,
    captured: Option<Captured>,
}
pub(crate) fn start_baseline(
    root: &super::super::protected_operations::Root,
    input: StartBaseline,
) -> Result<Operation, RefusedBaseline> {
    let input = root
        .admit_request(input)
        .map_err(|(error, request)| RefusedBaseline {
            error,
            request,
            captured: None,
        })?;
    let captured = match capture_baseline(
        input.state,
        input.target,
        input.registry.clone(),
        input.editor.clone(),
        input.visit.clone(),
    ) {
        Ok(captured) => captured,
        Err(error) => {
            return Err(RefusedBaseline {
                error,
                request: input,
                captured: None,
            });
        }
    };
    let operation = match Operation::reserve(
        &input.editor,
        Seed {
            request: Request::Baseline {
                evidence: input.evidence.clone(),
                choice: input.choice.clone(),
            },
            local: captured.native.local.clone(),
            book: captured.native.book.clone(),
            serial: captured.native.serial,
            epoch: captured.native.epoch,
        },
    ) {
        Ok(operation) => operation,
        Err(refused) => {
            return Err(RefusedBaseline {
                error: refused.error,
                request: input,
                captured: Some(captured),
            });
        }
    };
    let owned = operation.clone();
    let _ = root.install(
        operation.clone(),
        Box::pin(async move {
            let prepared = prepare(captured, owned.clone()).await?;
            let applied = prepared
                .perform(
                    Action::Baseline {
                        evidence: input.evidence.clone(),
                        choice: input.choice,
                        state: input.state,
                    },
                    Some(input.evidence.source()),
                    Some(input.evidence.binding()),
                )
                .await?;
            #[cfg(feature = "protected-native-fixture")]
            crate::drafts::protected::production::baseline_native::projection_checkpoint(
                input.state,
                &owned,
                &applied,
            )
            .await?;
            let barrier = applied.barrier.as_ref().ok_or(Error::Stale)?;
            let ticket = input.editor.prepare_projection(barrier.id()).await?;
            if !owned.current() {
                return Err(Error::Stale);
            }
            use super::super::protected_projection::{
                ProjectionResult, capture_baseline_projection,
            };
            let attempt = match capture_baseline_projection(
                input.state,
                input.target,
                input.editor.clone(),
                ticket,
                input.registry,
            )
            .await
            {
                ProjectionResult::Ready(attempt) => attempt,
                ProjectionResult::Changed(changed) => {
                    changed.preserve(&input.editor).await?;
                    return Err(Error::ReviewRequired);
                }
                ProjectionResult::Deferred(_) => return Err(Error::ReviewRequired),
            };
            if !owned.current() {
                return Err(Error::Stale);
            }
            let applied = match attempt.apply().await {
                ProjectionResult::Ready(applied) => applied,
                ProjectionResult::Changed(changed) => {
                    changed.preserve(&input.editor).await?;
                    return Err(Error::ReviewRequired);
                }
                ProjectionResult::Deferred(_) => return Err(Error::ReviewRequired),
            };
            if !owned.current() {
                return Err(Error::Stale);
            }
            let receipt = applied.install()?;
            let proof = input
                .editor
                .acknowledge_projection_observed(receipt)
                .await?;
            owned.record_baseline_projection(proof)?;
            Ok(Status::Finished)
        }),
    );
    Ok(operation)
}
