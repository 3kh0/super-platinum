use super::*;
use crate::resource_reads::render_test::{button, click, mount, settle, text};
fn render(state: Signal<ShellState>) -> Element {
    thread_list(state, &state.read())
}

#[tokio::test(flavor = "current_thread")]
async fn generated_thread_panel_retries_exact_failure_and_separates_refresh_older_initial_and_cycle_controls()
 {
    for alias in [
        "thread-feed-refresh-retry",
        "thread-feed-refresh-retry-narrow",
        "thread-feed-older-retry",
        "thread-feed-older-retry-narrow",
        "thread-feed-initial-retry",
        "thread-feed-initial-retry-narrow",
    ] {
        let (mut dom, mut state, captured) = mount(alias, render);
        let initial = alias.contains("initial");
        let older = alias.contains("older");
        let before = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
        let cursor = state.peek().core.threads_view.next_cursor.clone();
        let rendered = text(captured.borrow().as_ref().unwrap());
        assert!(rendered.contains(if initial {
            "Could not load threads."
        } else if older {
            "Could not load older threads."
        } else {
            "Could not refresh threads."
        }));
        if initial {
            assert!(!rendered.contains("Your loaded conversations"));
        }
        let sequence = state.peek().core.threads_view.load_seq;
        click(
            &dom,
            button(captured.borrow().as_ref().unwrap(), "thread-feed-retry").unwrap(),
        );
        settle(&mut dom, || {
            state.peek().core.threads_view.load_seq != sequence
                && !state.peek().core.threads_view.loading
        })
        .await;
        assert_eq!(
            state.peek().core.threads_view.failed_read().unwrap().cursor,
            if older { cursor.clone() } else { None }
        );
        assert_eq!(
            serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
            before
        );
        let sequence = state.peek().core.threads_view.load_seq;
        click(
            &dom,
            button(captured.borrow().as_ref().unwrap(), "thread-feed-refresh").unwrap(),
        );
        settle(&mut dom, || {
            state.peek().core.threads_view.load_seq != sequence
                && !state.peek().core.threads_view.loading
        })
        .await;
        assert!(
            state
                .peek()
                .core
                .threads_view
                .failed_read()
                .unwrap()
                .cursor
                .is_none()
        );
        if !initial {
            let sequence = state.peek().core.threads_view.load_seq;
            click(
                &dom,
                button(captured.borrow().as_ref().unwrap(), "thread-feed-older").unwrap(),
            );
            settle(&mut dom, || {
                state.peek().core.threads_view.load_seq != sequence
                    && !state.peek().core.threads_view.loading
            })
            .await;
            assert_eq!(
                state.peek().core.threads_view.failed_read().unwrap().cursor,
                cursor
            );
        }
        let busy = state.write().core.threads_view.begin_retry().unwrap();
        dom.render_immediate_to_vec();
        assert!(
            button(captured.borrow().as_ref().unwrap(), "thread-feed-refresh")
                .unwrap()
                .disabled
        );
        assert!(
            button(captured.borrow().as_ref().unwrap(), "thread-feed-retry").is_none(),
            "Core admission clears failed intent, retiring Retry during the active request"
        );
        if let Some(older) = button(captured.borrow().as_ref().unwrap(), "thread-feed-older") {
            assert!(older.disabled);
        }
        {
            let mut shell = state.write();
            assert!(shell.core.threads_view.cancel(&busy));
            shell.core.threads_view.failed = true;
            shell.core.threads_view.has_more = false;
            shell.core.threads_view.next_cursor = None;
        }
        dom.render_immediate_to_vec();
        assert!(button(captured.borrow().as_ref().unwrap(), "thread-feed-retry").is_none());
        assert!(button(captured.borrow().as_ref().unwrap(), "thread-feed-older").is_none());
        assert!(
            !button(captured.borrow().as_ref().unwrap(), "thread-feed-refresh")
                .unwrap()
                .disabled
        );
        assert!(
            text(captured.borrow().as_ref().unwrap()).contains("Threads could not finish loading.")
        );
        assert_eq!(
            serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
            before
        );
    }
}
