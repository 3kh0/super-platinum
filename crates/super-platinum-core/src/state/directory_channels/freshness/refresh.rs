//! Actual zuPe iY/ri split-refresh settlement. No IO, payload or store authority.
//! Native starts all three reads together, dispatches each admitted payload in
//! its settlement turn, and owns the original account/token plus 30s deadline.
use super::{Result, Scope};
pub mod request;
use std::sync::Arc;

pub const TIMEOUT_MS: u64 = 30_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    Init,
    Channels,
    Extras,
}
impl Branch {
    fn index(self) -> usize {
        match self {
            Self::Init => 0,
            Self::Channels => 1,
            Self::Extras => 2,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Pending,
    Publishing,
    Succeeded,
    Failed,
}
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Effects {
    /// Apply before this payload's actual synchronous dispatch, or in the
    /// current failure/timeout catch. A later fatal failure may advance twice.
    pub advance_major: bool,
    /// Clear only this original current team refresh token; never its successor.
    pub clear: bool,
    pub optional_error: Option<Branch>,
}
pub struct Publication {
    token: Arc<()>,
    branch: Branch,
}
pub struct Refresh {
    scope: Scope,
    token: Arc<()>,
    states: [State; 3],
    before_dispatch: bool,
    finished: bool,
}
impl Refresh {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            token: Arc::new(()),
            states: [State::Pending; 3],
            before_dispatch: false,
            finished: false,
        }
    }
    pub fn current(&self, scope: &Scope) -> bool {
        self.scope.matches(scope) && !self.finished
    }
    /// Reserve only an actually accepted successful read. Caller must check its
    /// live original native token before this and perform dispatch synchronously.
    pub fn publication(&mut self, scope: &Scope, branch: Branch) -> Result<(Publication, Effects)> {
        self.check(scope)?;
        if self.states[branch.index()] != State::Pending {
            return Err(unavailable());
        }
        self.states[branch.index()] = State::Publishing;
        let advance_major = !self.before_dispatch;
        self.before_dispatch = true;
        Ok((
            Publication {
                token: self.token.clone(),
                branch,
            },
            Effects {
                advance_major,
                ..Effects::default()
            },
        ))
    }
    /// Success or thrown error from the actual synchronous payload dispatcher.
    /// Optional dispatcher errors are swallowed exactly like optional read errors.
    pub fn published(
        &mut self,
        scope: &Scope,
        permit: Publication,
        success: bool,
    ) -> Result<Effects> {
        self.check(scope)?;
        if !Arc::ptr_eq(&permit.token, &self.token)
            || self.states[permit.branch.index()] != State::Publishing
        {
            return Err(unavailable());
        }
        self.settle(permit.branch, success)
    }
    pub fn failed(&mut self, scope: &Scope, branch: Branch) -> Result<Effects> {
        self.check(scope)?;
        if self.states[branch.index()] != State::Pending {
            return Err(unavailable());
        }
        self.settle(branch, false)
    }
    /// BaWM's outer race does not cancel issued requests. Native retires their
    /// publication, retains issued error accounting, and refuses unsent retries.
    pub fn timeout(&mut self, scope: &Scope) -> Result<Effects> {
        self.check(scope)?;
        self.finished = true;
        Ok(Effects {
            advance_major: true,
            clear: true,
            optional_error: None,
        })
    }
    fn check(&self, scope: &Scope) -> Result<()> {
        if self.current(scope) {
            Ok(())
        } else {
            Err(unavailable())
        }
    }
    fn settle(&mut self, branch: Branch, success: bool) -> Result<Effects> {
        self.states[branch.index()] = if success {
            State::Succeeded
        } else {
            State::Failed
        };
        if branch == Branch::Init && !success {
            self.finished = true;
            return Ok(Effects {
                advance_major: true,
                clear: true,
                optional_error: None,
            });
        }
        let clear = self.states[0] == State::Succeeded
            && self
                .states
                .iter()
                .skip(1)
                .all(|s| matches!(s, State::Succeeded | State::Failed));
        self.finished = clear;
        Ok(Effects {
            clear,
            optional_error: (!success).then_some(branch),
            ..Effects::default()
        })
    }
}
fn unavailable() -> super::Error {
    super::Error::MissingObservation
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> Scope {
        let owner = serde_json::from_value(serde_json::json!({
            "team_id":"TONE","user_id":"U00000001","name":"Fixture",
            "url":"https://fixture.slack.com","token":"synthetic"
        }))
        .unwrap();
        Scope::new("A", &owner, 0).unwrap()
    }
    fn apply(r: &mut Refresh, s: &Scope, b: Branch) -> (Effects, Effects) {
        let (p, before) = r.publication(s, b).unwrap();
        let after = r.published(s, p, true).unwrap();
        (before, after)
    }
    #[test]
    fn optional_first_publishes_before_init_and_advances_only_once() {
        let s = scope();
        let mut r = Refresh::new(s.clone());
        let (before, after) = apply(&mut r, &s, Branch::Channels);
        assert!(before.advance_major);
        assert!(!after.clear);
        let (before, after) = apply(&mut r, &s, Branch::Init);
        assert!(!before.advance_major);
        assert!(!after.clear);
        let (before, after) = apply(&mut r, &s, Branch::Extras);
        assert!(!before.advance_major);
        assert!(after.clear);
    }
    #[test]
    fn required_failure_after_optional_dispatch_advances_again_and_retires() {
        let s = scope();
        let mut r = Refresh::new(s.clone());
        assert!(apply(&mut r, &s, Branch::Channels).0.advance_major);
        let failed = r.failed(&s, Branch::Init).unwrap();
        assert!(failed.advance_major && failed.clear);
        assert!(r.publication(&s, Branch::Extras).is_err());
    }
    #[test]
    fn optional_failure_does_not_finish_required_or_successful_sibling() {
        let s = scope();
        let mut r = Refresh::new(s.clone());
        let e = r.failed(&s, Branch::Channels).unwrap();
        assert_eq!(e.optional_error, Some(Branch::Channels));
        assert!(!e.advance_major && !e.clear);
        assert!(apply(&mut r, &s, Branch::Init).0.advance_major);
        assert!(apply(&mut r, &s, Branch::Extras).1.clear);
    }
    #[test]
    fn timeout_retires_publication_and_original_permit_cannot_cross_refresh() {
        let s = scope();
        let mut old = Refresh::new(s.clone());
        let mut new = Refresh::new(s.clone());
        let (permit, _) = old.publication(&s, Branch::Channels).unwrap();
        assert!(new.published(&s, permit, true).is_err());
        assert!(old.timeout(&s).unwrap().advance_major);
        assert!(old.publication(&s, Branch::Init).is_err());
        assert!(new.current(&s));
    }
    #[test]
    fn required_dispatch_throw_advances_in_before_and_failure_catch() {
        let s = scope();
        let mut r = Refresh::new(s.clone());
        let (p, before) = r.publication(&s, Branch::Init).unwrap();
        assert!(before.advance_major);
        let after = r.published(&s, p, false).unwrap();
        assert!(after.advance_major && after.clear);
    }
}
