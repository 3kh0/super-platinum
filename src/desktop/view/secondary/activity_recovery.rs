use super::*;
use crate::resource_reads::render_test::{button, click, mount, settle, text};

fn render(state: Signal<ShellState>) -> Element {
    activity_list_panel(state, &state.read())
}

#[tokio::test(flavor = "current_thread")]
async fn generated_activity_retry_distinguishes_refresh_from_older_even_when_filters_hide_retained_rows()
 {
    for alias in [
        "activity-refresh-retry",
        "activity-refresh-retry-narrow",
        "activity-older-retry",
        "activity-older-retry-narrow",
    ] {
        let (mut dom, mut state, captured) = mount(alias, render);
        let older = alias.contains("older");
        let expected = older.then(|| "opaque/+?=&older".to_owned());
        let rendered = text(captured.borrow().as_ref().unwrap());
        assert!(rendered.contains(if older {
            "Could not load older activity"
        } else {
            "Could not refresh activity"
        }));
        assert!(rendered.contains(if older {
            "Retry loading older activity"
        } else {
            "Retry refresh"
        }));
        assert!(!rendered.contains("No unread activity."));
        assert!(!rendered.contains("No activity yet."));
        assert!(state.peek().core.activity.unread_only);
        assert_eq!(state.peek().core.activity.items.len(), 1);
        assert!(!state.peek().core.activity.items[0].is_pending());
        assert_eq!(
            state.peek().core.activity.selected.as_deref(),
            Some("retained-activity-read")
        );
        let before = serde_json::to_value(&state.peek().core.activity.items).unwrap();
        let generation = state.peek().core.activity.load_seq;
        let retry = button(captured.borrow().as_ref().unwrap(), "activity-read-retry").unwrap();
        assert!(!retry.disabled);
        click(&dom, retry);
        settle(&mut dom, || {
            state.peek().core.activity.load_seq != generation && !state.peek().core.activity.loading
        })
        .await;
        assert_eq!(
            state.peek().core.activity.failed_read().unwrap().cursor,
            expected
        );
        assert_eq!(
            serde_json::to_value(&state.peek().core.activity.items).unwrap(),
            before
        );
        assert!(state.peek().core.activity.unread_only);
        assert_eq!(
            state.peek().core.activity.selected.as_deref(),
            Some("retained-activity-read")
        );
        // The actual Refresh listener deliberately starts page one, including
        // after an older-page failure. Disconnected transport fails it locally.
        let generation = state.peek().core.activity.load_seq;
        let refresh = button(captured.borrow().as_ref().unwrap(), "activity-read-refresh").unwrap();
        click(&dom, refresh);
        settle(&mut dom, || {
            state.peek().core.activity.load_seq != generation && !state.peek().core.activity.loading
        })
        .await;
        assert!(
            state
                .peek()
                .core
                .activity
                .failed_read()
                .unwrap()
                .cursor
                .is_none()
        );
        let generation = state.peek().core.activity.load_seq;
        click(
            &dom,
            button(captured.borrow().as_ref().unwrap(), "activity-read-older").unwrap(),
        );
        settle(&mut dom, || {
            state.peek().core.activity.load_seq != generation && !state.peek().core.activity.loading
        })
        .await;
        assert_eq!(
            state
                .peek()
                .core
                .activity
                .failed_read()
                .unwrap()
                .cursor
                .as_deref(),
            Some("opaque/+?=&older")
        );
        {
            let mut shell = state.write();
            shell.core.activity.begin_retry().unwrap();
        }
        dom.render_immediate_to_vec();
        assert!(
            button(captured.borrow().as_ref().unwrap(), "activity-read-refresh")
                .unwrap()
                .disabled
        );
        assert!(
            button(captured.borrow().as_ref().unwrap(), "activity-read-older")
                .unwrap()
                .disabled
        );
        assert_eq!(
            serde_json::to_value(&state.peek().core.activity.items).unwrap(),
            before
        );
        {
            let mut shell = state.write();
            shell.core.activity = Default::default();
            let epoch = shell.core.account_epoch;
            let team = shell.core.active_team.clone().unwrap();
            shell.core.activity.ensure_scope(epoch, &team);
            let first = shell.core.activity.begin(false).unwrap();
            shell.core.activity.fail(&first);
        }
        dom.render_immediate_to_vec();
        let initial = text(captured.borrow().as_ref().unwrap());
        assert!(initial.contains("Could not load activity."));
        assert!(initial.contains("Retry loading activity"));
        assert!(!initial.contains("Your loaded notifications"));
        let generation = state.peek().core.activity.load_seq;
        click(
            &dom,
            button(captured.borrow().as_ref().unwrap(), "activity-read-retry").unwrap(),
        );
        settle(&mut dom, || {
            state.peek().core.activity.load_seq != generation && !state.peek().core.activity.loading
        })
        .await;
        assert!(
            state
                .peek()
                .core
                .activity
                .failed_read()
                .unwrap()
                .cursor
                .is_none()
        );
        assert!(!state.peek().core.activity.loaded);
    }
}
