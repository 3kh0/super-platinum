//! Unwired protected action adapter for existing mounted composers. No component,
//! selector, enrollment, automatic request, or alternate legacy route lives here.
use super::{field, field_scope, protected_enrollment::Policy};
use crate::{
    drafts::{
        network::Binding,
        protected::{
            Action, Applied, ReviewLease, Source, Visit, editor::Editor, registry::Registry,
        },
    },
    state::{ComposerTarget, ShellState},
};
use dioxus::prelude::*;
use std::{rc::Rc, sync::Arc};
use super_platinum_core::drafts::{LocalDraft, store::protected::Error, sync::Scope};

pub(crate) struct Prepared {
    lease: ReviewLease,
    native: Rc<NativeVisit>,
    evidence: super::protected_admission::Evidence,
    operation: crate::drafts::protected::operation::Operation,
}
struct NativeVisit {
    policy: Policy,
    state: Signal<ShellState>,
    target: ComposerTarget,
    scope: Scope,
    epoch: u64,
    mount: Rc<field::MountLease>,
    registry: Arc<Registry>,
    editor: Arc<Editor>,
    local: Arc<LocalDraft>,
    book: Option<Arc<LocalDraft>>,
    serial: u64,
}
impl Prepared {
    pub(crate) fn operation(&self) -> &crate::drafts::protected::operation::Operation {
        &self.operation
    }
    pub(crate) fn belongs_to(&self, editor: &Arc<Editor>) -> bool {
        Arc::ptr_eq(&self.native.editor, editor)
    }
    pub(crate) fn visible(&self) -> &LocalDraft {
        &self.native.local
    }
    pub fn review(&self) -> &super_platinum_core::drafts::store::protected::Review {
        self.lease.review()
    }
    /// The same dialog visit is consumed by the Editor at final admission.
    /// Dismissal retires the caller-owned Visit; it is never silently renewed.
    pub async fn perform(
        self,
        action: Action,
        source: Option<Arc<Source>>,
        binding: Option<Binding>,
    ) -> Result<Box<Applied>, Error> {
        // Keep the original captured Source alive through the entire renderer
        // handoff, as before extraction. Dropping its last Arc revokes Stream.
        let source_guard = source.clone();
        let binding_guard = binding.clone();
        let result = self
            .perform_guarded(action, move |shell| {
                if binding_guard
                    .as_ref()
                    .is_some_and(|binding| !binding.current(shell))
                {
                    return Err(Error::Stale);
                }
                source_guard
                    .as_ref()
                    .map(|source| source.stream(shell))
                    .transpose()
            })
            .await;
        drop((source, binding));
        result
    }
    #[cfg(feature = "protected-native-fixture")]
    pub(crate) async fn consume_fixture(
        self,
        binding: Arc<crate::drafts::protected::native_fixture::Binding>,
    ) -> Result<Box<Applied>, Error> {
        let target = self.native.target;
        let editor = self.native.editor.clone();
        self.perform_guarded(
            Action::Consume(super_platinum_core::drafts::sync::ConsumeKind::Send),
            move |shell| binding.current_stream(shell, target, &editor),
        )
        .await
    }
    async fn perform_guarded<G>(self, action: Action, additional: G) -> Result<Box<Applied>, Error>
    where
        G: FnOnce(
                &ShellState,
            )
                -> Result<Option<super_platinum_core::drafts::store::protected::Stream>, Error>
            + 'static,
    {
        use crate::drafts::protected::operation::Phase;
        if !self.native.policy.permits_action(&action) {
            return Err(Error::Unsupported);
        }
        self.operation.phase(Phase::NativeAdmission)?;
        let native = self.native.clone();
        let admission = match self.evidence.lock(self.native.mount.clone()).await {
            Ok(lease) => Rc::new(lease),
            Err(super::protected_admission::Failure::Changed(current)) => {
                self.operation.native(&current, true)?;
                let preservation = self
                    .native
                    .preserve_changed(current, self.native.serial)
                    .await;
                self.operation.preservation(preservation)?;
                return Err(Error::ReviewRequired);
            }
            Err(_) => return Err(Error::Stale),
        };
        let admission_guard = admission.clone();
        let operation_guard = self.operation.clone();
        let mut result = self
            .native
            .editor
            .apply_review_owned(self.lease, action, self.operation.clone(), move || {
                let shell = native.state.try_peek().map_err(|_| Error::Stale)?;
                if !operation_guard.current()
                    || !admission_guard.current()
                    || !native.current(&shell)
                {
                    return Err(Error::Stale);
                }
                additional(&shell)
            })
            .await?;
        self.operation.applied(&result)?;
        self.operation.phase(Phase::NativeClear)?;
        if result.after.row().state().consumption().is_some() && result.after.local().empty() {
            #[cfg(feature = "protected-native-fixture")]
            crate::drafts::protected::native_fixture::checkpoint(&result).await;
            if !self.operation.current()
                || !self
                    .native
                    .state
                    .try_peek()
                    .is_ok_and(|shell| self.native.current(&shell))
                || self.native.editor.snapshot().serial != result.serial
            {
                result.native_clear = crate::drafts::protected::NativeClear::Deferred;
                self.operation.consumed(&result)?;
                return Ok(result);
            }
            result.native_clear = match admission.clear().await {
                super::protected_admission::Clear::Applied
                    if self.operation.current() && self.native.install_consumed(&result) =>
                {
                    crate::drafts::protected::NativeClear::Cleared
                }
                super::protected_admission::Clear::Changed(current) => {
                    self.operation.native(&current, true)?;
                    let preservation = self.native.preserve_changed(current, result.serial).await;
                    self.operation.preservation(preservation)?;
                    crate::drafts::protected::NativeClear::Deferred
                }
                _ => crate::drafts::protected::NativeClear::Deferred,
            };
        }
        self.operation.applied(&result)?;
        Ok(result)
    }
}

pub(crate) enum Route {
    Delivery(crate::drafts::protected::delivery::Delivery, fn() -> u64),
    #[cfg(feature = "protected-native-fixture")]
    Fixture(Arc<crate::drafts::protected::native_fixture::Binding>),
}
pub(crate) struct Start {
    pub state: Signal<ShellState>,
    pub target: ComposerTarget,
    pub registry: Arc<Registry>,
    pub editor: Arc<Editor>,
    pub visit: Visit,
    pub request: crate::drafts::protected::operation::Request,
    pub route: Route,
}
pub(crate) struct Refused {
    pub error: Error,
    pub request: Start,
    // If Rust capture succeeded but Account reservation refused, retain the
    // exact original input/mount snapshot alongside the original request.
    captured: Option<Captured>,
}
/// Refusal returns the original typed request before any async preparation.
pub(crate) fn start(
    root: &super::protected_operations::Root,
    input: Start,
) -> Result<crate::drafts::protected::operation::Operation, Refused> {
    use crate::drafts::protected::operation::{Operation, Seed, Status};
    let input = root
        .admit_request(input)
        .map_err(|(error, request)| Refused {
            error,
            request,
            captured: None,
        })?;
    let capture_result = (|| {
        if let crate::drafts::protected::operation::Request::Schedule { snapshot, .. } =
            &input.request
        {
            crate::drafts::protected::operation::bounded(snapshot)?;
        }
        capture(
            input.state,
            input.target,
            input.registry.clone(),
            input.editor.clone(),
            input.visit.clone(),
            false,
        )
    })();
    let captured = match capture_result {
        Ok(value) => value,
        Err(error) => {
            return Err(Refused {
                error,
                request: input,
                captured: None,
            });
        }
    };
    let operation = match Operation::reserve(
        &input.editor,
        Seed {
            request: input.request.clone(),
            local: captured.native.local.clone(),
            book: captured.native.book.clone(),
            serial: captured.native.serial,
            epoch: captured.native.epoch,
        },
    ) {
        Ok(value) => value,
        Err(refused) => {
            return Err(Refused {
                error: refused.error,
                request: input,
                captured: Some(captured),
            });
        }
    };
    let Start { request, route, .. } = input;
    let owned = operation.clone();
    #[cfg(feature = "protected-native-fixture")]
    let installation_failure = match &route {
        Route::Fixture(binding) => Some(binding.clone()),
        _ => None,
    };
    // Heap-back the complete action before generic Root/Entry handoff. Passing
    // its inline future by value exceeded the measured native main stack.
    let installed = root.install(
        operation.clone(),
        Box::pin(async move {
            #[cfg(feature = "protected-native-fixture")]
            let fixture = match &route {
                Route::Fixture(binding) => Some(binding.clone()),
                _ => None,
            };
            let result = async {
                let prepared = prepare(captured, owned.clone()).await?;
                owned.phase(crate::drafts::protected::operation::Phase::Prepared)?;
                match route {
                    Route::Delivery(delivery, now) => {
                        let finished = delivery.run_owned(prepared, request, now).await?;
                        owned.delivered(finished)
                    }
                    #[cfg(feature = "protected-native-fixture")]
                    Route::Fixture(binding) => {
                        if !matches!(request, crate::drafts::protected::operation::Request::Send) {
                            return Err(Error::Unsupported);
                        }
                        let result = prepared.consume_fixture(binding.clone()).await;
                        match result {
                            Ok(applied) => {
                                binding.complete_operation(&owned, &applied);
                                Ok(Status::ConsumedAwaitingDelivery)
                            }
                            Err(error) => {
                                binding.fail_operation(error);
                                Err(error)
                            }
                        }
                    }
                }
            }
            .await;
            #[cfg(feature = "protected-native-fixture")]
            if let (Some(binding), Err(error)) = (fixture, &result) {
                binding.fail_operation(*error);
            }
            result
        }),
    );
    if let Err(error) = installed {
        // Account admission already exists; its armed completion guard records
        // Unknown. Return that exact retained operation rather than claiming a
        // clean pre-work refusal after transferring its renderer request.
        #[cfg(feature = "protected-native-fixture")]
        if let Some(binding) = installation_failure {
            binding.fail_operation(error);
        }
        #[cfg(not(feature = "protected-native-fixture"))]
        let _ = error;
    }
    Ok(operation)
}

fn capture(
    state: Signal<ShellState>,
    target: ComposerTarget,
    registry: Arc<Registry>,
    editor: Arc<Editor>,
    visit: Visit,
    whole_inventory: bool,
) -> Result<Captured, Error> {
    capture_with_policy(
        state,
        target,
        registry,
        editor,
        visit,
        whole_inventory,
        Policy::Plain,
    )
}
fn capture_baseline(
    state: Signal<ShellState>,
    target: ComposerTarget,
    registry: Arc<Registry>,
    editor: Arc<Editor>,
    visit: Visit,
) -> Result<Captured, Error> {
    capture_with_policy(
        state,
        target,
        registry,
        editor,
        visit,
        true,
        Policy::Baseline,
    )
}
fn capture_with_policy(
    state: Signal<ShellState>,
    target: ComposerTarget,
    registry: Arc<Registry>,
    editor: Arc<Editor>,
    visit: Visit,
    whole_inventory: bool,
    policy: Policy,
) -> Result<Captured, Error> {
    let native = {
        let shell = state.try_peek().map_err(|_| Error::Stale)?;
        let visible = local(&shell, target);
        if !policy.admits(&visible) || visible.composer.text.encode_utf16().count() > 131_072 {
            return Err(Error::Unsupported);
        }
        crate::drafts::protected::operation::bounded(&visible)?;
        if let Some(book) = shell
            .composer_key(target)
            .and_then(|key| shell.core.drafts.items.get(&key))
        {
            crate::drafts::protected::operation::bounded(book)?;
        }
        let scope = editor.scope().clone();
        let mount = field::current(&shell, target).ok_or(Error::Stale)?;
        let native = Rc::new(NativeVisit {
            policy,
            state,
            target,
            epoch: shell.core.account_epoch,
            scope,
            mount,
            registry,
            editor: editor.clone(),
            serial: editor.snapshot().serial,
            local: Arc::new(visible),
            book: shell
                .composer_key(target)
                .and_then(|key| shell.core.drafts.items.get(&key).cloned())
                .map(Arc::new),
        });
        if !native.current(&shell) {
            return Err(Error::Stale);
        }
        native
    };
    Ok(Captured {
        native,
        editor,
        visit,
        whole_inventory,
    })
}
struct Captured {
    native: Rc<NativeVisit>,
    editor: Arc<Editor>,
    visit: Visit,
    whole_inventory: bool,
}
async fn prepare(
    captured: Captured,
    operation: crate::drafts::protected::operation::Operation,
) -> Result<Prepared, Error> {
    use crate::drafts::protected::operation::Phase;
    let Captured {
        native,
        editor,
        visit,
        whole_inventory,
    } = captured;
    let state = native.state;
    let target = native.target;
    operation.phase(Phase::NativeCapture)?;
    let evidence = match super::protected_admission::Evidence::capture(
        match target {
            ComposerTarget::Channel => "channel-composer",
            ComposerTarget::Thread => "thread-composer",
        },
        &native.mount,
        &native.local.composer.text,
    )
    .await
    {
        Ok(evidence) => evidence,
        Err(super::protected_admission::Failure::Changed(current)) => {
            operation.native(&current, true)?;
            let preservation = native.preserve_changed(current, native.serial).await;
            operation.preservation(preservation)?;
            return Err(Error::ReviewRequired);
        }
        Err(_) => return Err(Error::Stale),
    };
    operation.native(evidence.snapshot(), false)?;
    operation.phase(Phase::Review)?;
    let lease = if whole_inventory {
        editor.prepare_settlement(visit, &native.local).await?
    } else {
        editor.prepare_review(visit, &native.local).await?
    };
    operation.review(lease.review())?;
    if !state.try_peek().is_ok_and(|shell| native.current(&shell)) {
        return Err(Error::Stale);
    }
    // Whole-inventory decisions may start from a preserved alternate. The
    // visible predecessor must be one of the exact versions being reviewed.
    let review = lease.review();
    if !native.policy.same_saved(&native.local, review.local())
        && !review
            .blocked()
            .is_some_and(|blocked| native.policy.same_saved(&native.local, &blocked.draft))
        && !review
            .alternates()
            .iter()
            .any(|alternate| native.policy.same_saved(&native.local, &alternate.draft))
    {
        return Err(Error::ReviewRequired);
    }
    Ok(Prepared {
        lease,
        native,
        evidence,
        operation,
    })
}
impl NativeVisit {
    // Deliberately does not lock Editor state: apply_review invokes this while
    // holding that lock. Its exact serial/writer/full Review check is adjacent.
    fn current(&self, shell: &ShellState) -> bool {
        self.mount.current()
            && field_scope(shell, self.target) == self.mount.scope
            && shell.core.active_account.as_deref() == Some(self.scope.account_id.as_str())
            && shell.core.account_epoch == self.epoch
            && shell.composer_key(self.target).as_ref() == Some(&self.scope.key)
            && self.registry.current(
                &self.scope.account_id,
                self.epoch,
                &self.scope.key,
                &self.editor,
            )
            && self.policy.same(&local(shell, self.target), &self.local)
            && match (
                shell.core.drafts.items.get(&self.scope.key),
                self.book.as_ref(),
            ) {
                (None, None) => true,
                (Some(a), Some(b)) => self.policy.same(a, b),
                _ => false,
            }
    }
    fn install_consumed(&self, applied: &Applied) -> bool {
        let mut state = self.state;
        let mut shell = match state.try_write() {
            Ok(shell) => shell,
            Err(_) => return false,
        };
        let snapshot = self.editor.snapshot();
        if !self.current(&shell)
            || snapshot.serial != applied.serial
            || snapshot.pending.is_some()
            || snapshot.running
            || snapshot.view.inventory().managed().is_err()
            || !snapshot
                .view
                .inventory()
                .managed()
                .unwrap()
                .same(&applied.after)
            || applied.before.scope() != &self.scope
            || !applied.after.local().empty()
        {
            return false;
        }
        if shell
            .core
            .drafts
            .project_clean(&self.scope.key, self.book.as_deref(), applied.after.local())
            .is_err()
        {
            return false;
        }
        match self.target {
            ComposerTarget::Channel => shell.core.composer = applied.after.local().composer.clone(),
            ComposerTarget::Thread => {
                shell.core.thread_composer = applied.after.local().composer.clone()
            }
        }
        true
    }
    async fn preserve_changed(
        &self,
        current: super::protected_projection::NativeSnapshot,
        serial: u64,
    ) -> Result<(), Error> {
        preserve_native_changed(&self.editor, &self.local, current, serial).await
    }
}
pub(super) async fn preserve_native_changed(
    editor: &Arc<Editor>,
    original: &LocalDraft,
    current: super::protected_projection::NativeSnapshot,
    serial: u64,
) -> Result<(), Error> {
    if current.wire == original.composer.text {
        return Ok(());
    }
    let mut local = original.clone();
    let start = super::utf16_to_byte(&current.wire, current.start);
    let end = super::utf16_to_byte(&current.wire, current.end);
    local.composer.text = current.wire;
    local.composer.set_selection(start, end);
    editor.preserve_native_input(serial, local).await
}

fn local(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
    match target {
        ComposerTarget::Channel => LocalDraft {
            composer: shell.core.composer.clone(),
            attachments: shell.core.composer_attachments.clone(),
        },
        ComposerTarget::Thread => LocalDraft {
            composer: shell.core.thread_composer.clone(),
            attachments: shell.core.thread_composer_attachments.clone(),
        },
    }
}

#[cfg(test)]
#[path = "protected_actions/tests.rs"]
mod tests;

#[path = "protected_slash.rs"]
mod slash;
pub(crate) use slash::{StartSlash, start_slash};

#[path = "protected_baseline.rs"]
mod baseline;
pub(crate) use baseline::{StartBaseline, start_baseline};
