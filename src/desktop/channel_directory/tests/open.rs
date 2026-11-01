//! Actual canonical producer plus explicit native navigation, no live transport.
mod input;
use super::*;
use crate::app_home::channel_fetches::{calendar_member_test_support as support, canonical};
use std::{cell::Cell, rc::Rc};
const TEAM: &str = "TINIT0001";
const FIRST: &str = "CINIT0001";
const SECOND: &str = "CINIT0002";

async fn initialize(dom: &mut VirtualDom, mut state: Signal<ShellState>) {
    let initial = {
        let mut s = state.write();
        crate::app_home::channel_fetches::fixture::install_people(&mut s);
        support::connect(&mut s).unwrap();
        support::assignments(
            &mut s,
            &json!({"ok":true,"assignments":{
                "channel_tabs_threads":{"group":"on"},
                "search_normalize_visible_external_users":{"group":"off"},
                "preserve_omitted_user_change_member_fields":{"group":"on"}
            }}),
        )
        .unwrap()
    };
    support::provide(dom, Rc::new(support::Renderer::default()));
    support::run_pumped(dom, initial.drain(state))
        .await
        .unwrap();
    let init = json!({"ok":true,"self":{"id":"UINIT0001","name":"fixture-self","team_id":TEAM,"profile":{}},
        "team":{"id":TEAM,"name":"Channel navigation"},"workspaces":[{"id":TEAM}],
        "onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{}});
    let channels = json!({"ok":true,"channels":{"channels":[
        {"id":FIRST,"name":"warm-harbor","is_channel":true,"is_member":true},
        {"id":SECOND,"name":"second-harbor","is_channel":true,"is_member":true}
    ],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    support::run_pumped(dom, initial.complete_channels(state, init, channels))
        .await
        .unwrap();
    state.write().refresh_from_core();
    assert!(
        canonical::observed(&state.peek(), TEAM)
            .unwrap()
            .current(&state.peek())
    );
    assert!(state.peek().channels.iter().any(|c| c.id == SECOND));
}
async fn metadata(entry: &Entry, state: Signal<ShellState>) {
    let (request, lease) = entry.begin(state, read::Mode::Search, None).unwrap();
    Entry::run_with(request, lease, |request| async move {
        Ok(Some(
            api::decode(
                json!({"ok":true,"items":[
            {"id":SECOND,"name":"Metadata display label","is_member":true,"member_count":42},
            {"id":"CUNKNOWN0001","name":"Metadata without canonical ownership","is_member":true}
        ],"pagination":{"page":1,"page_count":1}}),
                request.query(),
            )
            .unwrap(),
        ))
    })
    .await;
}
fn select(dom: &mut VirtualDom, captured: &crate::resource_reads::render_test::Captured, id: &str) {
    dom.render_immediate_to_vec();
    click(
        dom,
        button(
            captured.borrow().as_ref().unwrap(),
            &format!("your-channel-{id}"),
        )
        .unwrap(),
    );
    dom.render_immediate_to_vec();
}

#[tokio::test(flavor = "current_thread")]
async fn actual_open_requires_original_canonical_member_and_preserves_global_navigation_drafts() {
    let (mut dom, mut state, captured) = mount("your-channels", render);
    initialize(&mut dom, state).await;
    state.write().core.composer.text = "Keep the original channel draft".into();
    let entry = enter(state);
    metadata(&entry, state).await;
    let canonical_before =
        serde_json::to_value(&state.peek().core.workspaces[TEAM].channels).unwrap();
    select(&mut dom, &captured, "CUNKNOWN0001");
    let unavailable = button(captured.borrow().as_ref().unwrap(), "your-channels-open").unwrap();
    assert!(
        unavailable.disabled,
        "positive metadata membership is not canonical authority"
    );
    click(&dom, unavailable);
    assert_eq!(state.peek().core.active_channel.as_deref(), Some(FIRST));
    select(&mut dom, &captured, SECOND);
    let retired_open = button(captured.borrow().as_ref().unwrap(), "your-channels-open").unwrap();
    select(&mut dom, &captured, "CUNKNOWN0001");
    select(&mut dom, &captured, SECOND);
    click(&dom, retired_open);
    assert_eq!(
        state.peek().core.active_channel.as_deref(),
        Some(FIRST),
        "equal row reentry cannot revive its old handler"
    );
    let open = button(captured.borrow().as_ref().unwrap(), "your-channels-open").unwrap();
    assert!(!open.disabled);
    let old_open = button(captured.borrow().as_ref().unwrap(), "your-channels-open").unwrap();
    click(&dom, open);
    assert_eq!(state.peek().core.active_channel.as_deref(), Some(SECOND));
    assert!(state.peek().overlay.is_none());
    assert!(state.peek().main_view == MainView::Home);
    let generation = state.peek().channel_generation;
    click(&dom, old_open);
    assert_eq!(
        state.peek().channel_generation,
        generation,
        "original handler cannot reopen after consuming its dialog"
    );
    let index = state
        .peek()
        .channels
        .iter()
        .position(|c| c.id == FIRST)
        .unwrap();
    state
        .write()
        .select_channel(index, crate::state::ChannelOpen::Global);
    assert_eq!(
        state.peek().core.composer.text,
        "Keep the original channel draft"
    );
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces[TEAM].channels).unwrap(),
        canonical_before
    );
    // Poll the queued old continuation only after the replacement visit exists.
    support::run_pumped(&mut dom, async { tokio::task::yield_now().await }).await;
    assert_eq!(state.peek().core.active_channel.as_deref(), Some(FIRST));
    assert_eq!(
        state.peek().core.composer.text,
        "Keep the original channel draft"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn original_canonical_snapshot_and_queued_open_work_cannot_adopt_a_successor() {
    let (mut dom, mut state, captured) = mount("your-channels", render);
    initialize(&mut dom, state).await;
    let entry = enter(state);
    metadata(&entry, state).await;
    select(&mut dom, &captured, SECOND);
    let page = state.peek().channel_directory.loaded.clone().unwrap();
    let revision = state.peek().channel_directory.revision;
    let action =
        crate::channel_directory::open::capture(&state.peek(), page, revision, SECOND).unwrap();
    let work = action.apply(state).unwrap();
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    work.run_with(state, move |_| async move {
        counted.set(counted.get() + 1);
    })
    .await;
    assert_eq!(
        calls.get(),
        1,
        "current original continuation reaches the existing history loader seam"
    );
    for replacement_epoch in [false, true] {
        let entry = enter(state);
        metadata(&entry, state).await;
        select(&mut dom, &captured, SECOND);
        let page = state.peek().channel_directory.loaded.clone().unwrap();
        let revision = state.peek().channel_directory.revision;
        let action =
            crate::channel_directory::open::capture(&state.peek(), page, revision, SECOND).unwrap();
        let work = action.apply(state).unwrap();
        if replacement_epoch {
            state.write().core.account_epoch += 1;
        } else {
            state.write().reconnect_generation += 1;
        }
        let counted = calls.clone();
        work.run_with(state, move |_| async move {
            counted.set(counted.get() + 1);
        })
        .await;
        assert_eq!(
            calls.get(),
            1,
            "queued work cannot start history/navigation-history under replacement epoch or restore"
        );
    }
    let entry = enter(state);
    metadata(&entry, state).await;
    // Reinitialize through the original production ingress after epoch change.
    // The old canonical snapshot remains retired; no field/vector conversion.
    assert!(canonical::observed(&state.peek(), TEAM).is_err());
    select(&mut dom, &captured, SECOND);
    assert!(
        button(captured.borrow().as_ref().unwrap(), "your-channels-open")
            .unwrap()
            .disabled
    );
    initialize(&mut dom, state).await;
    let entry = enter(state);
    metadata(&entry, state).await;
    select(&mut dom, &captured, SECOND);
    let page = state.peek().channel_directory.loaded.clone().unwrap();
    let revision = state.peek().channel_directory.revision;
    let action =
        crate::channel_directory::open::capture(&state.peek(), page, revision, SECOND).unwrap();
    let before = (
        state.peek().core.active_channel.clone(),
        state.peek().channel_generation,
        state.peek().overlay,
    );
    let boot = crate::app_home::cfm_lifecycle::boot_started(&mut state.write(), TEAM);
    assert!(action.apply(state).is_none());
    crate::app_home::cfm_lifecycle::boot_settled(&mut state.write(), boot);
    assert!(
        action.apply(state).is_none(),
        "equal settled fields cannot revive the original canonical visit"
    );
    assert_eq!(
        (
            state.peek().core.active_channel.clone(),
            state.peek().channel_generation,
            state.peek().overlay
        ),
        before
    );
}
