//! Shell state carries only release authority; Eval stays in its renderer task.
use super::*;
use std::{
    future::Future,
    sync::{Arc, Mutex},
};
use tokio::sync::oneshot;

#[derive(Clone)]
pub(crate) struct Release {
    sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}
impl Release {
    pub(crate) fn release(&self) {
        let sender = self.sender.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(sender) = sender {
            let _ = sender.send(());
        }
    }
}
fn pair() -> (Release, oneshot::Receiver<()>) {
    let (sender, receiver) = oneshot::channel();
    (
        Release {
            sender: Arc::new(Mutex::new(Some(sender))),
        },
        receiver,
    )
}

struct Unlock<F: FnOnce()>(Option<F>);
impl<F: FnOnce()> Drop for Unlock<F> {
    fn drop(&mut self) {
        if let Some(unlock) = self.0.take() {
            unlock();
        }
    }
}
// Construct the guard before spawning: even dropping an unpolled renderer task
// releases its exact mounted controls. No Send bound belongs on this owner.
fn owner(released: oneshot::Receiver<()>, unlock: impl FnOnce()) -> impl Future<Output = ()> {
    let guard = Unlock(Some(unlock));
    async move {
        let _ = released.await;
        drop(guard);
    }
}

pub(crate) async fn lock_owned(state: Signal<ShellState>, visit: actions::Key) -> Option<Release> {
    // Keep the existing mount/sequence/value capture and lock acknowledgment.
    let eval = super::lock(state, visit).await?;
    Some(retain(eval))
}

pub(crate) fn retain(eval: dioxus::document::Eval) -> Release {
    let (release, released) = pair();
    dioxus::core::spawn_forever(owner(released, move || {
        let _ = eval.send(());
    }));
    release
}

#[cfg(test)]
pub(crate) fn test_pair() -> (Release, oneshot::Receiver<()>) {
    pair()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn serial_shell_keeps_send_without_renderer_evaluator() {
        fn send<T: Send>() {}
        send::<Release>();
        send::<ShellState>();
    }

    #[tokio::test]
    async fn cloned_release_signals_only_its_owner_once() {
        let (release, released) = pair();
        let clone = release.clone();
        let count = Rc::new(Cell::new(0));
        let observed = count.clone();
        let task = owner(released, move || observed.set(observed.get() + 1));
        let (other, mut other_receiver) = pair();
        release.release();
        clone.release();
        task.await;
        assert_eq!(count.get(), 1);
        assert!(matches!(
            other_receiver.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        other.release();
        assert_eq!(other_receiver.try_recv(), Ok(()));
    }

    #[tokio::test]
    async fn last_handle_drop_and_unpolled_owner_drop_each_unlock() {
        let count = Rc::new(Cell::new(0));
        let (release, released) = pair();
        let observed = count.clone();
        let task = owner(released, move || observed.set(observed.get() + 1));
        drop(release);
        task.await;
        assert_eq!(count.get(), 1);
        let (release, released) = pair();
        let observed = count.clone();
        let task = owner(released, move || observed.set(observed.get() + 1));
        drop(task);
        assert_eq!(count.get(), 2);
        release.release();
        assert_eq!(count.get(), 2);
    }
}
