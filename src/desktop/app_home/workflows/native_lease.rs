//! Renderer-local lock retention with an observed JS release completion fence.
use tokio::sync::oneshot;

pub(super) struct Completion(oneshot::Receiver<bool>);
impl Completion {
    pub(super) async fn current_after(self, current: impl FnOnce() -> bool) -> bool {
        self.0.await.unwrap_or(false) && current()
    }
}
pub(super) fn retain(eval: dioxus::document::Eval) -> (oneshot::Sender<()>, Completion) {
    let (release, released) = oneshot::channel();
    let (complete, completion) = oneshot::channel();
    dioxus::core::spawn_forever(finish_release(released, complete, move || async move {
        if eval.send(()).is_err() {
            return false;
        }
        // Not an enqueue acknowledgment: lockHomeCalendars' release() has run
        // before the original evaluation can return its result.
        eval.join::<serde_json::Value>().await.is_ok()
    }));
    (release, Completion(completion))
}
async fn finish_release<F: std::future::Future<Output = bool>>(
    released: oneshot::Receiver<()>,
    complete: oneshot::Sender<bool>,
    release: impl FnOnce() -> F,
) {
    // Shell replacement drops the unique sender and also releases this lock.
    let _ = released.await;
    let _ = complete.send(release().await);
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    fn pending<F: std::future::Future>(f: std::pin::Pin<&mut F>) -> bool {
        f.poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            .is_pending()
    }
    #[tokio::test]
    async fn release_enqueue_cannot_authorize_follow_on_before_js_completion_and_current_owner() {
        let (release, released) = oneshot::channel();
        let (complete, completion) = oneshot::channel();
        let (js_done, js) = oneshot::channel();
        let owner = Rc::new(Cell::new(true));
        let checks = Rc::new(Cell::new(0));
        let task = finish_release(released, complete, move || async move {
            js.await.unwrap_or(false)
        });
        tokio::pin!(task);
        let observed = owner.clone();
        let counted = checks.clone();
        let follow = Completion(completion).current_after(move || {
            counted.set(counted.get() + 1);
            observed.get()
        });
        tokio::pin!(follow);
        assert!(pending(task.as_mut()));
        release.send(()).unwrap();
        assert!(pending(task.as_mut()));
        assert!(pending(follow.as_mut()));
        assert_eq!(checks.get(), 0);
        // The account/visit can change during the genuine JS completion await.
        owner.set(false);
        js_done.send(true).unwrap();
        task.await;
        assert!(!follow.await);
        assert_eq!(checks.get(), 1);
    }
    #[tokio::test]
    async fn replacement_drop_releases_original_but_failed_join_never_authorizes_continuation() {
        let (release, released) = oneshot::channel();
        let (complete, completion) = oneshot::channel();
        let calls = Rc::new(Cell::new(0));
        let captured = calls.clone();
        let task = finish_release(released, complete, move || async move {
            captured.set(captured.get() + 1);
            false
        });
        tokio::pin!(task);
        assert!(pending(task.as_mut()));
        drop(release);
        task.await;
        assert_eq!(calls.get(), 1);
        assert!(
            !Completion(completion)
                .current_after(|| panic!("failed join grants no owner check or follow-on"))
                .await
        );
    }
    #[tokio::test]
    async fn successful_original_release_allows_current_follow_on() {
        let (release, released) = oneshot::channel();
        let (complete, completion) = oneshot::channel();
        release.send(()).unwrap();
        finish_release(released, complete, || async { true }).await;
        assert!(Completion(completion).current_after(|| true).await);
    }
    #[test]
    fn retained_release_handle_is_send() {
        fn send<T: Send>() {}
        send::<oneshot::Sender<()>>();
        send::<Completion>();
    }
}
