use super::super::{Owner, other};
use super::*;
use std::{cell::RefCell, rc::Rc, time::Duration};

type Handles = (Signal<Operations>, Lease, Lease);

#[test]
fn borrowed_cleanup_is_deferred_and_stale_cleanup_cannot_release_new_visit() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let handles = Rc::new(RefCell::new(None::<Handles>));
            let mut dom = VirtualDom::new_with_props(
                |handles: Rc<RefCell<Option<Handles>>>| {
                    let mut operations = use_signal(Operations::default);
                    use_hook(move || {
                        let owner = Owner {
                            account: "A1".into(),
                            team: "T1".into(),
                            user: "U1".into(),
                            draft: "D1".into(),
                        };
                        let running = operations.write().reserve_other(owner.clone()).unwrap();
                        operations
                            .write()
                            .begin_other(&running, other::Action::Edit)
                            .unwrap();
                        let mut second_owner = owner;
                        second_owner.draft = "D2".into();
                        let stale = operations
                            .write()
                            .reserve_other(second_owner.clone())
                            .unwrap();
                        // Both cleanup calls must wait until this immutable borrow ends.
                        let borrowed = operations.read();
                        run(
                            operations,
                            running.clone(),
                            Action::Settle(Outcome::Unknown),
                        );
                        run(operations, stale.clone(), Action::Detach);
                        assert_eq!(
                            borrowed.other_snapshot(&running).unwrap().phase,
                            other::Phase::Running
                        );
                        drop(borrowed);
                        operations.write().detach_other(&stale).unwrap();
                        let replacement = operations.write().reserve_other(second_owner).unwrap();
                        *handles.borrow_mut() = Some((operations, running, replacement));
                    });
                    rsx! { div {} }
                },
                handles.clone(),
            );
            dom.rebuild_in_place();
            for _ in 0..8 {
                let _ = tokio::time::timeout(Duration::from_millis(10), dom.wait_for_work()).await;
                dom.render_immediate(&mut dioxus::core::NoOpMutations);
            }
            let (operations, running, replacement) = handles.borrow().as_ref().unwrap().clone();
            assert_eq!(
                operations.peek().other_snapshot(&running).unwrap().phase,
                other::Phase::Uncertain
            );
            assert_eq!(
                operations
                    .peek()
                    .other_snapshot(&replacement)
                    .unwrap()
                    .phase,
                other::Phase::Idle
            );
        });
}
