//! Native text/selection/composition proof and short consume-admission lease.
use super::{
    field,
    protected_projection::{self as native, Direction, NativeReceipt, NativeSnapshot},
};
use dioxus::prelude::*;
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
pub(super) enum Failure {
    Changed(NativeSnapshot),
    Deferred,
}

pub(super) struct Evidence {
    before: NativeSnapshot,
}
pub(super) struct Lease {
    token: String,
    before: NativeSnapshot,
    mount: Rc<field::MountLease>,
    started: Instant,
}
pub(super) enum Clear {
    Applied,
    Changed(NativeSnapshot),
    Deferred,
}
impl Evidence {
    pub(super) fn snapshot(&self) -> &NativeSnapshot {
        &self.before
    }
    pub async fn capture(
        id: &str,
        mount: &Rc<field::MountLease>,
        text: &str,
    ) -> Result<Self, Failure> {
        let args = serde_json::to_string(&(id, &mount.scope, &mount.token, text)).unwrap();
        let result = native::evaluate(&format!(
            "(()=>{{{};return window.superPlatinumProtectedAdmission.capture(...{args});}})()",
            include_str!("protected_admission.js")
        ))
        .await;
        match result {
            NativeReceipt::Captured { current, .. }
                if matches(&current, id, mount) && current.wire == text =>
            {
                #[cfg(feature = "protected-native-fixture")]
                crate::drafts::protected::native_fixture::record_native(
                    "capture",
                    serde_json::json!({"before":current}),
                );
                Ok(Self { before: current })
            }
            NativeReceipt::Changed { current } if matches(&current, id, mount) => {
                Err(Failure::Changed(current))
            }
            _ => Err(Failure::Deferred),
        }
    }
    pub async fn lock(&self, mount: Rc<field::MountLease>) -> Result<Lease, Failure> {
        let started = Instant::now();
        let expected = serde_json::to_string(&self.before).unwrap();
        match native::evaluate(&format!(
            "window.superPlatinumProtectedAdmission?.lock({expected})"
        ))
        .await
        {
            NativeReceipt::Captured { token, current }
                if native::valid_token(&token)
                    && current == self.before
                    && matches(&current, &self.before.id, &mount)
                    && started.elapsed() < Duration::from_secs(3) =>
            {
                #[cfg(feature = "protected-native-fixture")]
                crate::drafts::protected::native_fixture::record_native(
                    "lock",
                    serde_json::json!({"token":token,"before":current}),
                );
                Ok(Lease {
                    token,
                    before: current,
                    mount,
                    started,
                })
            }
            NativeReceipt::Captured { token, .. } => {
                release(&token);
                Err(Failure::Deferred)
            }
            NativeReceipt::Changed { current } if matches(&current, &self.before.id, &mount) => {
                Err(Failure::Changed(current))
            }
            _ => Err(Failure::Deferred),
        }
    }
}
fn matches(value: &NativeSnapshot, id: &str, mount: &field::MountLease) -> bool {
    value.valid()
        && !value.composing
        && mount.current()
        && value.id == id
        && value.scope == mount.scope
        && value.visit == mount.token
}
impl Lease {
    // Conservative renderer deadline precedes JS expiry by at least 3s. The
    // native field stays readOnly between this check and synchronous enqueue.
    pub fn current(&self) -> bool {
        self.mount.current() && self.started.elapsed() < Duration::from_secs(3)
    }
    pub async fn clear(&self) -> Clear {
        let token = serde_json::to_string(&self.token).unwrap();
        match native::evaluate(&format!(
            "window.superPlatinumProtectedAdmission?.clear({token})"
        ))
        .await
        {
            NativeReceipt::Applied {
                token,
                before,
                after,
            } if token == self.token
                && before == self.before
                && matches(&after, &before.id, &self.mount)
                && after.input == before.input
                && after.composition == before.composition
                && after.wire.is_empty()
                && after.start == 0
                && after.end == 0
                && after.direction == Direction::Forward =>
            {
                #[cfg(feature = "protected-native-fixture")]
                crate::drafts::protected::native_fixture::record_native(
                    "clear-ack",
                    serde_json::json!({"token":token,"before":before,"after":after}),
                );
                Clear::Applied
            }
            NativeReceipt::Changed { current }
                if matches(&current, &self.before.id, &self.mount) =>
            {
                #[cfg(feature = "protected-native-fixture")]
                crate::drafts::protected::native_fixture::record_native(
                    "clear-changed",
                    serde_json::json!({"current":current}),
                );
                Clear::Changed(current)
            }
            _ => Clear::Deferred,
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        release(&self.token);
    }
}
fn release(token: &str) {
    if native::valid_token(token) {
        #[cfg(feature = "protected-native-fixture")]
        crate::drafts::protected::native_fixture::record_native(
            "release-requested",
            serde_json::json!({"token":token}),
        );
        let token = serde_json::to_string(token).unwrap();
        document::eval(&format!(
            "window.superPlatinumProtectedAdmission?.release({token});"
        ));
    }
}
