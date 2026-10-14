//! Actual AppKit label readback in a disconnected, explicitly requested dev fixture.
use super::Owner;
use super::*;
use crate::state::ShellState;
use dioxus::prelude::*;
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_DOCK_BADGE_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("channel")
}

pub(crate) async fn run(state: Signal<ShellState>, desktop: DesktopContext) {
    let projection = std::env::var("SUPER_PLATINUM_DOCK_BADGE_CHECK").as_deref()
        == Ok("projection")
        && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("channel");
    let result = if projection {
        projection_check(state).await
    } else if requested() {
        check(state, desktop).map_err(str::to_owned)
    } else {
        return;
    }
    .unwrap_or_else(|error| json!({"ok":false,"error":error}));
    eprintln!("dock-badge-check: {result}");
}

fn actual_label() -> Result<Option<String>, &'static str> {
    let mtm = MainThreadMarker::new().ok_or("Dock badge check must run on the main thread")?;
    Ok(NSApplication::sharedApplication(mtm)
        .dockTile()
        .badgeLabel()
        .map(|s| s.to_string()))
}

fn check(state: Signal<ShellState>, desktop: DesktopContext) -> Result<Value, &'static str> {
    {
        let shell = state.peek();
        if !crate::fixture::is_fixture()
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
        {
            return Err("Dock badge check requires a disconnected fixture");
        }
    }
    use dioxus::desktop::tao::platform::macos::WindowExtMacOS;
    let writes = Rc::new(RefCell::new(Vec::new()));
    let recorded = writes.clone();
    let mut controller = Controller::with_writer(move |label| {
        desktop.window.set_badge_label(label.clone());
        recorded.borrow_mut().push(label);
    });
    let first = Some(Owner {
        account: "fixture-a".into(),
        epoch: 1,
    });
    let second = Some(Owner {
        account: "fixture-b".into(),
        epoch: 2,
    });
    let mut stages = Vec::new();
    for (name, owner, badge, expected) in [
        ("clear", None, DockBadge::Empty, None),
        ("number", first.clone(), DockBadge::Count(42), Some("42")),
        ("unchanged", first.clone(), DockBadge::Count(42), Some("42")),
        ("dot", first.clone(), DockBadge::Dot, Some("•")),
        ("zero", first.clone(), DockBadge::Count(0), None),
        ("newNumber", first.clone(), DockBadge::Count(7), Some("7")),
        (
            "accountChange",
            second.clone(),
            DockBadge::Count(7),
            Some("7"),
        ),
        ("retired", None, DockBadge::Count(999), None),
        ("beforeDrop", second, DockBadge::Dot, Some("•")),
    ] {
        let before = writes.borrow().len();
        controller.update(owner, badge);
        let actual = actual_label()?;
        // AppKit can expose a cleared label as nil or the empty NSString.
        if actual.as_deref().filter(|s| !s.is_empty()) != expected {
            return Err("AppKit badge label differs from the requested state");
        }
        let changes = writes.borrow().len() - before;
        if name == "unchanged" && changes != 0 {
            return Err("Unchanged badge wrote to AppKit");
        }
        if name == "accountChange" && changes != 2 {
            return Err("Account change did not clear the previous owner first");
        }
        stages.push(json!({"stage":name,"label":actual,"writes":changes}));
    }
    drop(controller);
    if actual_label()?.is_some_and(|s| !s.is_empty()) {
        return Err("Dropping the owner did not clear AppKit");
    }
    let expected = vec![
        None,
        Some("42".into()),
        Some("•".into()),
        None,
        Some("7".into()),
        None,
        Some("7".into()),
        None,
        Some("•".into()),
        None,
    ];
    if *writes.borrow() != expected {
        return Err("Unexpected native label transition sequence");
    }
    Ok(
        json!({"ok":true,"noTransport":true,"noPreferenceWrites":true,"actualAppKitReadback":true,"unchangedNoWrite":true,"ownerClear":true,"dropClear":true,"writes":writes.borrow().len(),"stages":stages,"visualDockPixelsProven":false}),
    )
}

fn require_isolated(state: Signal<ShellState>) -> Result<(), String> {
    let shell = state.peek();
    if !crate::fixture::is_fixture()
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
    {
        Err("Dock projection check requires a disconnected fixture".into())
    } else {
        Ok(())
    }
}

async fn wait_projection(
    state: Signal<ShellState>,
    name: &str,
    expected: Option<&str>,
) -> Result<Value, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        require_isolated(state)?;
        let actual = actual_label().map_err(str::to_owned)?;
        if actual.as_deref().filter(|s| !s.is_empty()) == expected {
            return Ok(json!({"stage":name,"label":actual}));
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Production badge hook timed out at {name}: label={actual:?}, cached={:?}",
                state.peek().dock_badge_value()
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn projection_check(mut state: Signal<ShellState>) -> Result<Value, String> {
    require_isolated(state)?;
    {
        let mut shell = state.write();
        shell.signed_in = true;
        shell.core.active_account = Some("dock-projection-fixture".into());
        shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1);
        shell.core.workspaces.retain(|team, _| team == "T1");
        let w = shell
            .core
            .workspaces
            .get_mut("T1")
            .ok_or("Missing synthetic workspace")?;
        w.notifications = Default::default();
        w.notifications.dock_loaded = true;
        let rules = super_platinum_core::slack::api::parse_badge_experiments(
            &json!({"ok":true,"assignments":{"activity_inbox":{"group":"on"}}}),
        )
        .map_err(|_| "Synthetic badge policy rejected")?;
        let ticket = w.activity_counts.begin_experiments();
        w.activity_counts.apply_experiments(ticket, rules);
        w.apply_activity_counts(
            serde_json::from_value(json!({"at_user":6,"message_reaction":900})).unwrap(),
        );
        shell.refresh_dock_badge();
    }
    let mut stages = vec![wait_projection(state, "activityExcludesReaction", Some("6")).await?];
    {
        let mut shell = state.write();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .apply_activity_counts(serde_json::from_value(json!({})).unwrap());
        shell.refresh_dock_badge_for("T1");
    }
    stages.push(wait_projection(state, "activityZero", None).await?);
    {
        let mut shell = state.write();
        let w = shell.core.workspaces.get_mut("T1").unwrap();
        w.notifications
            .global
            .insert("global_reminders_in_activity".into(), json!(false));
        w.dock_counts.apply_overdue_count(4);
        shell.refresh_dock_badge_for("T1");
    }
    stages.push(wait_projection(state, "eligibleLater", Some("4")).await?);
    {
        let mut shell = state.write();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .notifications
            .user
            .insert("tab_rail_customization".into(), json!(r#"{"later":false}"#));
        shell.refresh_dock_badge_for("T1");
    }
    stages.push(wait_projection(state, "hiddenLater", None).await?);
    {
        let mut shell = state.write();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .notifications
            .user
            .insert("tab_rail_customization".into(), json!(r#"{"later":true}"#));
        shell.refresh_dock_badge_for("T1");
    }
    stages.push(wait_projection(state, "positiveBeforeRetirement", Some("4")).await?);
    {
        let mut shell = state.write();
        shell.core.active_account = None;
        shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1);
        // No refresh here: the cached getter and native owner retirement must
        // clear immediately, before a replacement account loader completes.
    }
    stages.push(wait_projection(state, "accountRetirement", None).await?);
    Ok(
        json!({"ok":true,"mode":"projection","noTransport":true,"noPreferenceWrites":true,
        "productionHook":true,"actualAppKitReadback":true,"filteredActivity":true,
        "authoritativeZero":true,"laterEligibility":true,"ownerClear":true,
        "visualDockPixelsProven":false,"stages":stages}),
    )
}
