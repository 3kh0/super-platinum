//! Exercise the real hook subscriptions, not just the request-state reducer.
use super::*;
use std::{cell::RefCell, rc::Rc, time::Duration};

type Handles = (
    Signal<ShellState>,
    Signal<Picker>,
    Signal<Directory>,
    Signal<u64>,
);

async fn settle(dom: &mut VirtualDom) {
    for _ in 0..15 {
        let _ = tokio::time::timeout(Duration::from_millis(25), dom.wait_for_work()).await;
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
    }
}

#[test]
fn shell_ticks_do_not_restart_directory_search_but_query_and_retry_do() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let handles = Rc::new(RefCell::new(None::<Handles>));
            let mut dom = VirtualDom::new_with_props(
                |handles: Rc<RefCell<Option<Handles>>>| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "new-dm",
                        )
                    });
                    let picker = use_signal(|| Picker {
                        query: "charlotte".into(),
                        ..Default::default()
                    });
                    let (directory, retry) = use_directory(state, picker);
                    *handles.borrow_mut() = Some((state, picker, directory, retry));
                    let loading = directory.read().loading;
                    rsx! { div { "{loading}" } }
                },
                handles.clone(),
            );
            dom.rebuild_in_place();
            settle(&mut dom).await;
            let (mut state, mut picker, directory, mut retry) = handles.borrow().unwrap();
            assert_eq!(directory.peek().revision, 1);
            assert!(!directory.peek().loading);
            assert!(directory.peek().error.is_some()); // No transport: no network in tests.
            for _ in 0..3 {
                state.write().media_epoch += 1;
                settle(&mut dom).await;
                assert_eq!(
                    directory.peek().revision,
                    1,
                    "unrelated shell update restarted search"
                );
                assert!(!directory.peek().loading);
            }
            picker.write().query = "maya".into();
            settle(&mut dom).await;
            assert_eq!(directory.peek().revision, 2);
            retry += 1;
            settle(&mut dom).await;
            assert_eq!(directory.peek().revision, 3);
            assert!(!directory.peek().loading);
        });
}
