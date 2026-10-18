//! Exact fixture or configured discovery generation shared by native and worker.
//! No public raw catalog constructor and no generic dispatch callback.
use super::*;
use crate::{drafts::network::Binding, state::ShellState};
use super_platinum_core::slash_commands::catalog::{Catalog, Choice};

pub(crate) struct Context {
    scope: Scope,
    backend: Backend,
}
enum Backend {
    #[cfg(any(test, feature = "protected-native-fixture"))]
    Fixture(Arc<o::SlashFixture>),
    Configured {
        context: Arc<o::SlashContext>,
        binding: Binding,
        stream: durable::Stream,
    },
}
impl Context {
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub(crate) fn fixture(fixture: Arc<o::SlashFixture>) -> Arc<Self> {
        Arc::new(Self {
            scope: fixture.scope().clone(),
            backend: Backend::Fixture(fixture),
        })
    }
    pub(crate) fn configured(
        context: o::SlashContext,
        binding: Binding,
        stream: durable::Stream,
    ) -> Arc<Self> {
        let owner = context.catalog().owner();
        let scope = Scope {
            account_id: owner.account.clone(),
            key: DraftKey {
                team: owner.team.clone(),
                channel: owner.channel.clone(),
                thread: owner.thread.clone(),
            },
        };
        Arc::new(Self {
            scope,
            backend: Backend::Configured {
                context: Arc::new(context),
                binding,
                stream,
            },
        })
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn catalog(&self) -> &Catalog {
        match &self.backend {
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Backend::Fixture(f) => f.catalog(),
            Backend::Configured { context, .. } => context.catalog(),
        }
    }
    pub(crate) fn is_live(&self) -> bool {
        match &self.backend {
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Backend::Fixture(f) => f.is_live(),
            Backend::Configured {
                context, stream, ..
            } => context.is_live() && stream.is_live(),
        }
    }
    pub(crate) fn revoke(&self) {
        match &self.backend {
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Backend::Fixture(f) => f.revoke(),
            Backend::Configured { context, .. } => context.revoke(),
        }
    }
    pub(crate) fn current(&self, shell: &ShellState, stream: &durable::Stream) -> bool {
        if !self.is_live()
            || !stream.is_live()
            || stream.account() != self.scope.account_id
            || stream.team() != self.scope.key.team
        {
            return false;
        }
        match &self.backend {
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Backend::Fixture(f) => {
                shell.core.session.is_none()
                    && shell
                        .core
                        .workspaces
                        .get(&self.scope.key.team)
                        .is_some_and(|w| w.self_user_id == f.catalog().owner().user)
                    && shell
                        .core
                        .transport
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, &f.transport()))
                    && shell.core.cache.is_none()
            }
            Backend::Configured {
                binding,
                stream: captured,
                ..
            } => binding.current(shell) && captured.is_live() && captured.epoch() == stream.epoch(),
        }
    }
    pub(crate) fn prepare(
        &self,
        ops: &o::Operations,
        stream: durable::Stream,
        review: durable::Review,
        writer: durable::Writer,
        serial: u64,
        choice: &Choice,
        token: &str,
    ) -> Result<o::SlashInput, o::AdmissionError> {
        match &self.backend {
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Backend::Fixture(f) => f.prepare(ops, stream, review, writer, serial, choice, token),
            Backend::Configured {
                context,
                stream: captured,
                ..
            } => {
                if !stream.is_live()
                    || !captured.is_live()
                    || stream.account() != captured.account()
                    || stream.team() != captured.team()
                    || stream.epoch() != captured.epoch()
                {
                    return Err(o::AdmissionError::Stale);
                }
                context.prepare(ops, review, writer, serial, choice, token)
            }
        }
    }
    pub(super) async fn execute(
        &self,
        completion: &o::SlashCompletion,
    ) -> Result<(), o::SlashExecuteError> {
        match &self.backend {
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Backend::Fixture(f) => f.execute(completion).await,
            Backend::Configured { context, .. } => context.execute(completion).await,
        }
    }
    pub(crate) fn definitely_unattempted(&self) -> bool {
        match &self.backend {
            #[cfg(any(test, feature = "protected-native-fixture"))]
            Backend::Fixture(f) => f.requests().is_empty(),
            // Do not infer NotSent from an HTTP error or canceled visible waiter.
            Backend::Configured { .. } => false,
        }
    }
}
