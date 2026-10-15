//! Exhaustive identity from the real private command payload, never a drain label.
use super::*;
impl Request {
    pub(super) fn identity(&self) -> accounting::Identity<'_> {
        let (account, family, key) = match self {
            Self::StoreUpgrade(command) => (command.account(), "upgrade", None),
            Self::SlashJournal(command) => (command.account(), "slash-journal", None),
            Self::SlashProtected(command) => (
                command.account(),
                "protected-slash",
                Some(&command.scope().key),
            ),
            Self::Protected(command) => (command.account.as_str(), "protected", None),
            Self::Upload(command) => {
                let (account, family) = command.identity();
                (account, family, None)
            }
            Self::ReadLocalReview(scope, _) => {
                (scope.account_id.as_str(), "local-review", Some(&scope.key))
            }
            Self::SettleLocalVersions(review, _, _) => (
                review.scope.account_id.as_str(),
                "local-settle",
                Some(&review.scope.key),
            ),
            Self::ReconcileRemoteReview(review, _, _) => {
                let scope = &review.inventory().scope;
                (scope.account_id.as_str(), "remote-review", Some(&scope.key))
            }
            Self::CommitReviewedSchedule(review, _, _, _) => (
                review.scope.account_id.as_str(),
                "reviewed-schedule",
                Some(&review.scope.key),
            ),
            Self::ReviewRejectedSchedule(review, _, _, _) => (
                review.scope.account_id.as_str(),
                "rejected-schedule",
                Some(&review.scope.key),
            ),
            Self::Load(account, _) => (account.as_str(), "legacy-load", None),
            Self::LoadSynced(account, _, _) => (account.as_str(), "legacy-synced-load", None),
            Self::Commit(scope, _, _, _, _, _) => {
                (scope.account_id.as_str(), "legacy-commit", Some(&scope.key))
            }
            Self::Prepare(scope, _, _) => (
                scope.account_id.as_str(),
                "legacy-prepare",
                Some(&scope.key),
            ),
            Self::Release(scope, _, _) => (
                scope.account_id.as_str(),
                "legacy-release",
                Some(&scope.key),
            ),
            Self::Alternate(scope, _, _) => {
                (scope.account_id.as_str(), "alternate", Some(&scope.key))
            }
            Self::OwnedAlternate(scope, _, _, _, _) => (
                scope.account_id.as_str(),
                "owned-alternate",
                Some(&scope.key),
            ),
            Self::ConditionalLocal(scope, _, _, _) => (
                scope.account_id.as_str(),
                "conditional-local",
                Some(&scope.key),
            ),
            Self::OwnedConditionalLocal(scope, _, _, _, _, _) => (
                scope.account_id.as_str(),
                "owned-conditional-local",
                Some(&scope.key),
            ),
        };
        accounting::Identity {
            account,
            family,
            key,
        }
    }
}

impl Request {
    // Conservative owner-wide barrier: unrelated queued protected work may also
    // refuse a fresh command. Completion remains allowed to retain evidence.
    pub(super) fn slash_fence_account(&self) -> Option<&str> {
        match self {
            Self::SlashProtected(_) => None,
            _ => Some(self.identity().account),
        }
    }
}
