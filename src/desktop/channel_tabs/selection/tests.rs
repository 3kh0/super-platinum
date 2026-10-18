use super::*;
use serde_json::json;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::channel_tabs::fixture::install(&mut shell);
    ordinary_open(&mut shell);
    shell
}
fn canvas(shell: &ShellState) -> Route {
    Route::Canvas(CanvasRoute::capture(&inventory(shell).unwrap(), "Ct00000001").unwrap())
}
fn choose(shell: &mut ShellState, route: Route) {
    assert!(record(shell, route.clone(), None));
    shell.channel_tab = route.tab();
}
#[test]
fn explicit_canvas_history_round_trip_does_not_create_sticky_channel_preference() {
    let mut shell = shell();
    let route = canvas(&shell);
    choose(&mut shell, route.clone());
    assert!(selected(&shell, "Ct00000001"));
    let back = step(&shell, false).unwrap();
    assert_eq!(back.route, Route::Resource(ChannelTab::Messages));
    assert!(record(&mut shell, back.route.clone(), Some(&back)));
    shell.channel_tab = back.route.tab();
    let forward = step(&shell, true).unwrap();
    assert_eq!(forward.route, route);
    assert!(record(&mut shell, forward.route.clone(), Some(&forward)));
    shell.channel_tab = forward.route.tab();
    let index = shell
        .channels
        .iter()
        .position(|c| c.id == "C00000001")
        .unwrap();
    shell.select_channel(index, crate::state::ChannelOpen::Global);
    assert_eq!(shell.channel_tab, ChannelTab::Messages);
    assert_eq!(step(&shell, false).unwrap().route, route);
}
#[test]
fn per_channel_and_account_histories_do_not_supply_each_others_destination() {
    let mut shell = shell();
    let route = canvas(&shell);
    choose(&mut shell, route);
    let saved = step(&shell, false).unwrap();
    let original = shell.core.active_channel.clone();
    shell.core.active_channel = Some("COTHER".into());
    ordinary_open(&mut shell);
    assert!(step(&shell, false).is_none());
    assert!(!accepts(&shell, &saved));
    shell.core.active_channel = original;
    ordinary_open(&mut shell);
    assert!(step(&shell, false).is_some());
    shell.core.active_account = Some("another-account".into());
    ordinary_open(&mut shell);
    assert!(step(&shell, false).is_none());
    assert!(!accepts(&shell, &saved));
}
#[test]
fn old_history_callback_cannot_replace_a_new_selection_or_branch() {
    let mut shell = shell();
    let route = canvas(&shell);
    choose(&mut shell, route);
    let old = step(&shell, false).unwrap();
    shell.channel_tab = ChannelTab::Files;
    assert!(!accepts(&shell, &old));
    shell.channel_tab = ChannelTab::Canvas;
    choose(&mut shell, Route::Resource(ChannelTab::Files));
    assert!(!accepts(&shell, &old));
    let back = step(&shell, false).unwrap();
    assert!(record(&mut shell, back.route.clone(), Some(&back)));
    shell.channel_tab = back.route.tab();
    choose(&mut shell, Route::Resource(ChannelTab::Pins));
    assert!(step(&shell, true).is_none());
}
#[test]
fn removed_retyped_and_rebound_file_routes_cannot_reenter_a_canvas() {
    for replacement in [
        json!([]),
        json!([{"id":"Ct00000001","type":"future_resource"}]),
        json!([{"id":"Ct00000001","type":"canvas","data":{"file_id":"F00000009"}}]),
    ] {
        let mut shell = shell();
        let route = canvas(&shell);
        choose(&mut shell, route);
        let key = key(&shell).unwrap();
        let raw = json!({"id":key.channel,"properties":{"tabz":replacement}});
        shell.channel_tabs.entries.get_mut(&key).unwrap().inventory =
            Some(Inventory::from_channel(&raw).unwrap());
        assert!(current(&shell).is_none());
        assert!(target(&shell).is_none());
        assert!(!selected(&shell, "Ct00000001"));
        ordinary_open(&mut shell);
        assert!(step(&shell, false).is_none());
    }
}
#[test]
fn rename_reorder_and_unknown_raw_sibling_do_not_rebind_the_selected_file() {
    let mut shell = shell();
    let route = canvas(&shell);
    choose(&mut shell, route.clone());
    let key = key(&shell).unwrap();
    let raw = json!({"id":key.channel,"properties":{"tabz":[
        {"id":"Ct00000002","type":"future_resource","opaque":{"list":[3,1,2]}},
        {"id":"Ct00000001","type":"canvas","label":"Renamed","data":{"file_id":"F00000001"}}
    ]}});
    let base = Inventory::from_channel(&raw).unwrap();
    shell.channel_tabs.entries.get_mut(&key).unwrap().inventory = Some(base.clone());
    assert_eq!(current(&shell), Some(route));
    assert_eq!(target(&shell).unwrap().title, "Renamed");
    assert_eq!(inventory(&shell).unwrap(), base);
}
#[test]
fn deleted_file_has_no_embedded_target_even_when_tab_metadata_survives() {
    let mut shell = shell();
    let route = canvas(&shell);
    choose(&mut shell, route);
    let team = shell.core.active_team.clone().unwrap();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .deleted_files
        .insert("F00000001".into());
    assert!(target(&shell).is_none());
}
#[test]
fn navigation_metadata_is_bounded_and_does_not_evict_uncertain_operations() {
    let mut shell = shell();
    let key = key(&shell).unwrap();
    let pending = Intent::capture(
        inventory(&shell).unwrap(),
        Action::Remove {
            id: "Ct00000001".into(),
        },
    )
    .unwrap();
    shell.channel_tabs.entries.get_mut(&key).unwrap().pending = Some(pending.clone());
    for i in 0..200 {
        choose(
            &mut shell,
            Route::Resource(if i % 2 == 0 {
                ChannelTab::Pins
            } else {
                ChannelTab::Files
            }),
        );
    }
    assert_eq!(
        shell.channel_tabs.navigation.entries[&key].routes.len(),
        ROUTES
    );
    for i in 0..200 {
        shell.core.active_channel = Some(format!("C{i}"));
        ordinary_open(&mut shell);
    }
    assert_eq!(shell.channel_tabs.navigation.entries.len(), CHANNELS);
    assert_eq!(
        shell.channel_tabs.entries[&key].pending.as_ref(),
        Some(&pending)
    );
}

#[test]
fn automatic_add_selection_requires_new_exact_row_and_original_dispatch_visit() {
    let base = Inventory::from_channel(&json!({"id":"C00000001","properties":{"tabz":[
        {"id":"Ct00000001","type":"canvas","label":"Old label","data":{"file_id":"F00000009"}},
        {"id":null,"type":"future_resource","opaque":[3,1,2]}
    ]}}))
    .unwrap();
    let intent = Intent::capture(
        base.clone(),
        Action::AddCanvas {
            file: "F00000001".into(),
            label: "New label".into(),
            access: super_platinum_core::channel_tabs::Access::Read,
            after: None,
        },
    )
    .unwrap();
    let mut rows = base.rows().to_vec();
    rows.push(json!({"id":"Ct00000003","type":"canvas","label":"New label","data":{"file_id":"F00000001"}}));
    let current =
        Inventory::from_channel(&json!({"id":"C00000001","properties":{"tabz":rows}})).unwrap();
    assert_eq!(
        confirmed_added(&intent, &current, true, true).as_deref(),
        Some("Ct00000003")
    );
    assert!(confirmed_added(&intent, &current, false, true).is_none());
    assert!(confirmed_added(&intent, &current, true, false).is_none());
    assert!(confirmed_added(&intent, &base, true, true).is_none());
    let mut rows = current.rows().to_vec();
    rows[1]["opaque"] = json!([1, 2, 3]);
    let changed =
        Inventory::from_channel(&json!({"id":"C00000001","properties":{"tabz":rows}})).unwrap();
    assert!(confirmed_added(&intent, &changed, true, true).is_none());
}

#[test]
fn selected_remove_and_editor_open_are_mutually_exclusive_at_handoff() {
    let mut shell = shell();
    let route = canvas(&shell);
    choose(&mut shell, route);
    let action = Action::Remove {
        id: "Ct00000001".into(),
    };
    assert!(can_mutate(&shell, &action));
    let generation = shell.shortcut_generation;
    // Real entry point helpers are rechecked after preflight and at editor open.
    assert!(crate::canvas::surface::hold_local(&mut shell, generation));
    assert!(!can_mutate(&shell, &action));
    assert!(can_mutate(
        &shell,
        &Action::Rename {
            id: "Ct00000001".into(),
            label: "Renamed tab".into()
        }
    ));
    crate::canvas::surface::release_local(&mut shell, generation);
    assert!(can_mutate(&shell, &action));
    let intent = Intent::capture(inventory(&shell).unwrap(), action).unwrap();
    let key = key(&shell).unwrap();
    shell.channel_tabs.entries.get_mut(&key).unwrap().pending = Some(intent);
    assert!(removal_pending(&shell));
    assert!(!crate::canvas::surface::hold_local(&mut shell, generation));
}

#[test]
fn home_history_rechecks_bot_eligibility_before_admitting_a_retained_route() {
    let mut shell = shell();
    let home = Route::Resource(ChannelTab::AppHome);
    assert!(!home.valid(&shell));
    assert!(!record(&mut shell, home.clone(), None));
    let team = shell.core.active_team.clone().unwrap();
    let channel = shell.core.active_channel.clone().unwrap();
    {
        let workspace = shell.core.workspaces.get_mut(&team).unwrap();
        workspace.users.insert(
            "U_HOME_HISTORY".into(),
            serde_json::from_value(json!({
                "id":"U_HOME_HISTORY", "is_bot":true, "deleted":false
            }))
            .unwrap(),
        );
        let conversation = workspace.channels.get_mut(&channel).unwrap();
        conversation.is_im = true;
        conversation.user = Some("U_HOME_HISTORY".into());
    }
    assert!(home.valid(&shell));
    choose(&mut shell, home.clone());
    choose(&mut shell, Route::Resource(ChannelTab::Messages));
    let retained = step(&shell, false).unwrap();
    assert_eq!(retained.route, home);
    assert!(accepts(&shell, &retained));
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("U_HOME_HISTORY")
        .unwrap()
        .deleted = true;
    assert!(!home.valid(&shell));
    assert!(step(&shell, false).is_none());
    assert!(!accepts(&shell, &retained));
    assert!(!record(&mut shell, home.clone(), Some(&retained)));
    assert_eq!(current(&shell), Some(Route::Resource(ChannelTab::Messages)));
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("U_HOME_HISTORY")
        .unwrap()
        .deleted = false;
    assert!(accepts(&shell, &retained));
    assert!(record(&mut shell, home, Some(&retained)));
    shell.channel_tab = ChannelTab::AppHome;
    assert_eq!(current(&shell), Some(Route::Resource(ChannelTab::AppHome)));
}
