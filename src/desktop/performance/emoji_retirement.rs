//! Opt-in diagnostic observation only. No deferred destruction or strong readers.
#[cfg(feature = "emoji-retirement-observation")]
mod actions;
#[cfg(feature = "emoji-retirement-observation")]
mod check;
#[cfg(feature = "emoji-retirement-observation")]
mod control;
#[cfg(feature = "emoji-retirement-observation")]
mod fixture;
#[cfg(feature = "emoji-retirement-observation")]
mod ledger;
#[cfg(feature = "emoji-retirement-observation")]
mod native;

#[cfg(feature = "emoji-retirement-observation")]
pub(crate) use ledger::{action, building, emit, event, generation, scalar_id, scope};
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) mod event {
    pub const LOADED_FINAL: u64 = 6;
    pub const TARGET: u64 = 9;
    pub const PUBLISHED: u64 = 12;
    pub const REJECTED: u64 = 13;
    pub const WORKER_WAIT: u64 = 16;
}
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) fn emit(_: u64, _: u64, _: u64, _: u64) -> u64 {
    0
}
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) fn scope(_: u64, _: u64, _: u64) {}
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) fn building(_: u64) {}
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) fn generation(_: u64, _: u64, _: bool) -> u64 {
    0
}
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) fn scalar_id(_: &str) -> u64 {
    0
}

pub(crate) fn synchronous<T>(context: u64, write: u64, mount: u64, run: impl FnOnce() -> T) -> T {
    let _scope = scope(context, write, mount);
    run()
}
pub(crate) fn hold(generation: u64, revision: u64) {
    #[cfg(feature = "emoji-retirement-observation")]
    if enabled() {
        control::hold(generation, revision);
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    let _ = (generation, revision);
}
pub(crate) fn outcome(generation: u64, published: bool) {
    #[cfg(feature = "emoji-retirement-observation")]
    if enabled() {
        control::outcome(generation, published);
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    let _ = (generation, published);
}
pub(crate) fn worker_wait(generation: u64, mount: u64) {
    #[cfg(feature = "emoji-retirement-observation")]
    if enabled() {
        control::worker_wait(generation, mount);
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    let _ = (generation, mount);
}
pub(crate) fn worker_active() {
    #[cfg(feature = "emoji-retirement-observation")]
    if enabled() {
        control::worker_active();
    }
}
pub(crate) fn remote_allowed(account: u64, revision: u64) -> bool {
    #[cfg(feature = "emoji-retirement-observation")]
    {
        enabled() && control::remote_allowed(account, revision)
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    {
        let _ = (account, revision);
        false
    }
}
pub(crate) async fn remote_response(
    account: u64,
    revision: u64,
    ticket: u64,
) -> Option<Result<Vec<super_platinum_core::slack::models::Emoji>, super_platinum_core::slack::Error>>
{
    #[cfg(feature = "emoji-retirement-observation")]
    {
        if enabled() && control::remote_allowed(account, revision) {
            tokio::task::yield_now().await;
            control::remote_response(account, revision, ticket)
        } else {
            None
        }
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    {
        let _ = (account, revision, ticket);
        None
    }
}

pub(crate) fn enabled() -> bool {
    #[cfg(all(test, feature = "emoji-retirement-observation"))]
    if let Some(enabled) = ledger::test_enabled() {
        return enabled;
    }
    #[cfg(feature = "emoji-retirement-observation")]
    {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            cfg!(target_os = "windows")
                && std::env::var("SUPER_PLATINUM_EMOJI_RETIREMENT").as_deref() == Ok("1")
                && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref()
                    == Ok("emoji-retirement-64000")
        })
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    {
        false
    }
}
pub(crate) fn instrument(source: &str) -> Result<String, &'static str> {
    super::emoji_lifecycle::instrument(source)
        .map(|s| s.replace("__spEmojiLifecycle", "__spEmojiRetirement"))
}
#[cfg(feature = "emoji-retirement-observation")]
pub(crate) fn use_close_observer() {
    if !enabled() {
        return;
    }
    let id = dioxus::desktop::window().window.id();
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if matches!(event,dioxus::desktop::tao::event::Event::WindowEvent {
            window_id,event:dioxus::desktop::WindowEvent::CloseRequested,..} if *window_id==id)
        {
            ledger::emit(ledger::event::CLOSE_REQUESTED, 0, 0, 0);
        }
    });
}

#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) type Payload<T> = T;
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) fn payload<T>(value: T, _: u64) -> T {
    value
}
#[cfg(not(feature = "emoji-retirement-observation"))]
pub(crate) fn payload_id<T>(_: &T) -> u64 {
    0
}

#[cfg(feature = "emoji-retirement-observation")]
pub(crate) struct Payload<T> {
    value: Option<T>,
    generation: u64,
    child: u64,
}
#[cfg(feature = "emoji-retirement-observation")]
impl<T> std::ops::Deref for Payload<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().expect("live observed payload")
    }
}
#[cfg(feature = "emoji-retirement-observation")]
impl<T> Drop for Payload<T> {
    fn drop(&mut self) {
        if self.generation == 0 {
            drop(self.value.take());
            return;
        }
        emit(event::OUTER_START, self.generation, self.child, 0);
        let value = self.value.take();
        emit(event::PAYLOAD_START, self.generation, self.child, 0);
        drop(value);
        emit(event::PAYLOAD_END, self.generation, self.child, 0);
        emit(event::OUTER_END, self.generation, self.child, 0);
    }
}
#[cfg(feature = "emoji-retirement-observation")]
pub(crate) fn payload<T>(value: T, child: u64) -> Payload<T> {
    let generation = if enabled() { ledger::build_id() } else { 0 };
    let result = Payload {
        value: Some(value),
        generation,
        child,
    };
    if generation != 0 {
        emit(event::CONSTRUCTED, generation, child, 0);
    }
    result
}
#[cfg(feature = "emoji-retirement-observation")]
pub(crate) fn payload_id<T>(value: &Payload<T>) -> u64 {
    value.generation
}

pub(crate) fn install(shell: &mut crate::state::ShellState) {
    #[cfg(feature = "emoji-retirement-observation")]
    if enabled() {
        fixture::install(shell, 0);
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    let _ = shell;
}
pub(crate) fn start(
    state: dioxus::prelude::Signal<crate::state::ShellState>,
    desktop: dioxus::desktop::DesktopContext,
) {
    #[cfg(feature = "emoji-retirement-observation")]
    if enabled() {
        check::start(state, desktop);
    }
    #[cfg(not(feature = "emoji-retirement-observation"))]
    let _ = (state, desktop);
}

#[cfg(all(test, feature = "emoji-retirement-observation"))]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Counted(Arc<AtomicUsize>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn independent_child_arcs_destroy_once_on_actual_final_reader() {
        let counts = Arc::new(AtomicUsize::new(0));
        let catalog = Arc::new(payload(Counted(counts.clone()), 1));
        let picker = Arc::new(payload(Counted(counts.clone()), 2));
        let independent = picker.clone();
        assert_eq!(Arc::strong_count(&catalog), 1);
        assert_eq!(Arc::strong_count(&picker), 2);
        drop(catalog);
        drop(picker);
        assert_eq!(counts.load(Ordering::SeqCst), 1);
        drop(independent);
        assert_eq!(counts.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn payload_is_send_when_its_value_is_send() {
        fn send<T: Send>() {}
        send::<Payload<Vec<String>>>();
    }
}

#[cfg(all(test, feature = "emoji-retirement-observation"))]
#[path = "emoji_retirement/overhead.rs"]
mod overhead;
