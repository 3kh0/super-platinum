//! Delivery-only synchronization tests. Real Worker/SQLite flows are separately
//! exercised by owned_wait_tests; these do not claim durable admission authority.
use super::*;
use std::time::Duration;

fn waiting_step() -> (Step<(), u64>, mpsc::Sender<Result<u64, p::Error>>) {
    let owner = crate::drafts::store::protected::upgrade::Owned::fresh().unwrap();
    let mut state = State::new(owner, "delivery-test-only");
    let (send, receiver) = mpsc::channel();
    let held = Arc::new(StepHeld {
        input: Arc::new(()),
        changed: Condvar::new(),
        status: Mutex::new(StepStatus {
            request: None,
            receiver: Some(receiver),
            result: None,
            failures: Vec::new(),
            disconnected: false,
            // Delivery primitive only: never submitted to an executor and never
            // passed to wait_step/transfer (which require the actual Book).
            submitted: Some(1),
            next: None,
            reserved: None,
            tail: Default::default(),
        }),
    });
    let key = state.register(1, None, "test-reply-only", held.clone());
    (Step { key, held }, send)
}
fn receiver_owned(step: &Step<(), u64>) {
    // A bounded test synchronization only; production uses the actual Condvar.
    let end = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if step.held.status.lock().unwrap().receiver.is_none() {
            return;
        }
        assert!(std::time::Instant::now() < end);
        std::thread::yield_now();
    }
}
#[test]
fn receiver_custody_keeps_state_and_input_nonblocking_and_wakes_both_waiters() {
    let (step, send) = waiting_step();
    let first = step.clone();
    let waiter = std::thread::spawn(move || first.wait_reply());
    receiver_owned(&step);
    let (done, received) = mpsc::channel();
    let other = step.clone();
    let second = std::thread::spawn(move || {
        assert_eq!(other.state(), StepState::Waiting);
        assert_eq!(*other.input(), ());
        done.send(()).unwrap();
        other.wait_reply()
    });
    received.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(step.state(), StepState::Waiting);
    send.send(Ok(42)).unwrap();
    assert_eq!(waiter.join().unwrap().unwrap(), (StepState::Ready, Some(1)));
    assert_eq!(second.join().unwrap().unwrap(), (StepState::Ready, Some(1)));
    assert_eq!(step.value().unwrap(), 42);
}
#[test]
fn dropping_ui_wait_handle_does_not_drop_receiver_or_replay_result() {
    let (step, send) = waiting_step();
    let (finished, completion) = mpsc::channel();
    let owned = step.clone();
    let waiter = std::thread::spawn(move || {
        finished.send(owned.wait_reply()).unwrap();
    });
    receiver_owned(&step);
    drop(waiter);
    send.send(Err(p::Error::Storage)).unwrap();
    assert_eq!(
        completion
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        (StepState::Failed(p::Error::Storage), Some(1))
    );
    assert_eq!(
        step.wait_reply().unwrap(),
        (StepState::Failed(p::Error::Storage), Some(1))
    );
    assert_eq!(step.value(), Err(p::Error::Storage));
}
#[test]
fn disconnected_sender_is_terminal_only_after_actual_receive() {
    let (step, send) = waiting_step();
    let owned = step.clone();
    let waiter = std::thread::spawn(move || owned.wait_reply());
    receiver_owned(&step);
    assert_eq!(step.state(), StepState::Waiting);
    drop(send);
    assert_eq!(
        waiter.join().unwrap().unwrap(),
        (StepState::Disconnected, Some(1))
    );
}
