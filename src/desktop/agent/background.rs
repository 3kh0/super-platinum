//! Keep the explicitly enabled automation server responsive under macOS App Nap.
//! This does not activate windows, prevent idle sleep, or change agent-off runs.

pub(super) struct Activity {
    // A one-shot release owns the exact native beginActivity token. Keeping this
    // non-Clone and !Send matches the local Dioxus server future's lifetime.
    finish: Option<Box<dyn FnOnce()>>,
}
impl Activity {
    #[cfg(any(target_os = "macos", test))]
    fn with_release(finish: impl FnOnce() + 'static) -> Self {
        Self {
            finish: Some(Box::new(finish)),
        }
    }
    fn acquire() -> Self {
        #[cfg(target_os = "macos")]
        {
            use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
            let process = NSProcessInfo::processInfo();
            let token = process.beginActivityWithOptions_reason(
                NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
                &NSString::from_str("Super Platinum explicit agent control server"),
            );
            Self::with_release(move || {
                // SAFETY: This is the retained opaque object returned by this
                // process's beginActivity call. Drop invokes this closure once,
                // before either retained object is released.
                unsafe { process.endActivity(&token) };
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self { finish: None }
        }
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        if let Some(finish) = self.finish.take() {
            finish();
        }
    }
}
fn acquire_if(enabled: bool, acquire: impl FnOnce() -> Activity) -> Option<Activity> {
    enabled.then(acquire)
}
pub(super) fn begin_if(enabled: bool) -> Option<Activity> {
    acquire_if(enabled, Activity::acquire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn disabled_agent_never_acquires_and_enabled_scope_releases_once() {
        let begins = Arc::new(AtomicUsize::new(0));
        let ends = Arc::new(AtomicUsize::new(0));
        for enabled in [false, true] {
            let ended = ends.clone();
            let activity = acquire_if(enabled, || {
                begins.fetch_add(1, Ordering::SeqCst);
                Activity::with_release(move || {
                    ended.fetch_add(1, Ordering::SeqCst);
                })
            });
            assert_eq!(ends.load(Ordering::SeqCst), 0);
            drop(activity);
        }
        assert_eq!(begins.load(Ordering::SeqCst), 1);
        assert_eq!(ends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancelling_a_started_server_future_releases_its_activity() {
        use std::{
            future::Future,
            task::{Context, Poll, Waker},
        };
        let ends = Arc::new(AtomicUsize::new(0));
        let ended = ends.clone();
        let mut server = Box::pin(async move {
            let _activity = acquire_if(true, || {
                Activity::with_release(move || {
                    ended.fetch_add(1, Ordering::SeqCst);
                })
            });
            std::future::pending::<()>().await;
        });
        assert!(matches!(
            server
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        assert_eq!(ends.load(Ordering::SeqCst), 0);
        drop(server);
        assert_eq!(ends.load(Ordering::SeqCst), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn activity_allows_idle_system_and_display_sleep() {
        use objc2_foundation::NSActivityOptions as Options;
        let flags = Options::UserInitiatedAllowingIdleSystemSleep;
        assert!(
            !flags.intersects(Options::IdleSystemSleepDisabled | Options::IdleDisplaySleepDisabled)
        );
        assert!(!flags.contains(Options::LatencyCritical));
    }
}
