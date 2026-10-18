//! Closed mentions fixture. The held transport is constructed without HTTP.
use super::*;
use serde_json::json;
use super_platinum_core::slack::Transport;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_MENTIONS_CHECK";
pub(crate) const CHANNEL: &str = "CMENTION01";
pub(crate) const LABEL: &str = "project-harbor";
pub(crate) const USER: &str = "UMENTION01";
pub(crate) const GROUP: &str = "SMENTION01";
pub(crate) const GROUP_HANDLE: &str = "@@ Cafe\u{0301} crew  ";
pub(crate) fn group_mode() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROFILE_GROUP_MENTIONS_CHECK").as_deref() == Ok("1")
}
pub(crate) const USER_LABEL: &str = "  Cafe\u{0301} Harbor  ";
pub(crate) fn user_mode() -> bool {
    named() && std::env::var("SUPER_PLATINUM_PROFILE_USER_MENTIONS_CHECK").as_deref() == Ok("1")
}
pub(crate) fn source_label() -> &'static str {
    if group_mode() {
        GROUP_HANDLE
    } else if user_mode() {
        USER_LABEL
    } else {
        LABEL
    }
}
pub(crate) fn mounted_label() -> String {
    if group_mode() {
        return GROUP_HANDLE.into();
    }
    if user_mode() {
        format!("@{USER_LABEL}")
    } else {
        format!("#{LABEL}")
    }
}
pub(crate) fn renamed_label() -> &'static str {
    if group_mode() {
        return "renamed-group";
    }
    if user_mode() {
        "renamed-member"
    } else {
        "renamed-harbor"
    }
}

#[derive(Clone)]
pub(super) struct Identity {
    token: Token,
    transport: Arc<Transport>,
}

pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-sections-mentions" | "profile-sections-mentions-narrow")
        )
}

pub(super) fn enabled(shell: &ShellState) -> bool {
    if !cfg!(any(test, feature = "protected-native-fixture"))
        || !((named() && std::env::var(FLAG).as_deref() == Ok("1"))
            || super::member_recovery::enabled()
            || super::group_recovery::enabled()
            || (super::recovery::named()
                && std::env::var(super::recovery::FLAG).as_deref() == Ok("1")))
        || shell.profile_edit.fixture.is_none()
    {
        return false;
    }
    shell
        .profile_edit
        .sections
        .fixture
        .as_ref()
        .and_then(|f| f.mentions.as_ref())
        .is_some_and(|identity| {
            identity.token.current(shell)
                && shell
                    .core
                    .transport
                    .as_ref()
                    .is_some_and(|t| Arc::ptr_eq(t, &identity.transport))
        })
}

/// Duplicate channel occurrences carry distinct unknown fields. The editor and
/// section wire must retain both even when the source Delta length merges them.
pub(crate) fn blocks(text: &str) -> Vec<Value> {
    let token = |occurrence| {
        if group_mode() {
            json!({"type":"usergroup","usergroup_id":GROUP,"fixture_occurrence":occurrence,"tab_id":"opaque-tab","opaque":{"keep":true}})
        } else if user_mode() {
            json!({"type":"user","user_id":USER,"fixture_occurrence":occurrence})
        } else {
            json!({"type":"channel","channel_id":CHANNEL,"fixture_occurrence":occurrence})
        }
    };
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":text},token(1),token(2)
        ]}]}),
    ]
}

pub(crate) fn install(shell: &mut ShellState) {
    crate::app_home::channel_fetches::fixture::install(shell);
    // The accepted raw directory belongs to this account. The older profile
    // fixture installs its own account; restore custody before binding drafts.
    let account = shell.core.active_account.clone();
    let (Some(session), Some(transport)) =
        (shell.core.session.clone(), shell.core.transport.clone())
    else {
        return;
    };
    install_interaction(shell);
    shell.core.active_account = account;
    shell.core.session = Some(session);
    shell.core.transport = Some(transport.clone());
    let token = Token::capture(shell).expect("closed mentions profile identity");
    shell.profile_edit.draft.as_mut().unwrap().token = token.clone();
    shell.profile_edit.sections.draft.as_mut().unwrap().token = token.clone();
    shell
        .profile_edit
        .sections
        .fixture
        .as_mut()
        .unwrap()
        .mentions = Some(Identity { token, transport });
    set_blocks(shell, blocks("Working with "));
}

pub(crate) fn set_blocks(shell: &mut ShellState, blocks: Vec<Value>) {
    let pane = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (token, visit, serial) = (pane.token.clone(), pane.visit, pane.serial);
    let mut raw = shell
        .profile_edit
        .sections
        .fixture
        .as_ref()
        .unwrap()
        .snapshot
        .raw()
        .clone();
    let fields = raw["result"]["data"]["user"]["profileSections"][0]["profileElements"]
        .as_array_mut()
        .unwrap();
    fields
        .iter_mut()
        .find(|f| f["elementId"] == "long")
        .unwrap()["blocks"] = json!(serde_json::to_string(&blocks).unwrap());
    let snapshot = api::OwnProfileSections::from_response(
        &super::super::super::fixture::session(&token.owner),
        &raw,
    )
    .unwrap();
    shell
        .profile_edit
        .sections
        .fixture
        .as_mut()
        .unwrap()
        .snapshot = snapshot.clone();
    network::finish_load(shell, &token, visit, serial, Ok(snapshot.clone()));
    shell.profile_edit.sections.draft.as_mut().unwrap().form =
        Some(protocol::SectionDraft::open(&snapshot, "S1").unwrap());
}

pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Some(fixture) = shell.profile_edit.sections.fixture.as_ref() else {
        return json!({"enabled":false});
    };
    let view = presentation(shell);
    let draft = view
        .as_ref()
        .and_then(|v| v.form.as_ref())
        .and_then(|f| f.value("long"));
    let blocks = match draft {
        Some(SectionValue::LongText(blocks)) => json!(blocks),
        _ => Value::Null,
    };
    let managed =
        fixture.snapshot.raw()["result"]["data"]["user"]["profileSections"][0]["profileElements"]
            .as_array()
            .and_then(|fields| fields.iter().find(|f| f["elementId"] == "locked"))
            .map(|f| f["text"].clone());
    let form = view.as_ref().and_then(|v| v.form.as_ref());
    let choice = match form.and_then(|f| f.value("choice")) {
        Some(SectionValue::Text(value)) => Some(value),
        _ => None,
    };
    let country = match form.and_then(|f| f.value("location")) {
        Some(SectionValue::Location { country, .. }) => Some(country),
        _ => None,
    };
    json!({"enabled":enabled(shell),"attempts":fixture.attempts,"reads":fixture.reads,
        "blocks":blocks,"dirty":view.as_ref().and_then(|v|v.form.as_ref()).is_some_and(|f|f.dirty()),
        "busy":view.as_ref().is_some_and(|v|v.busy),"uncertain":view.as_ref().is_some_and(|v|v.uncertain),
        "canSave":view.as_ref().is_some_and(|v|v.can_save),"canCheck":view.as_ref().is_some_and(|v|v.can_check),
        "notice":view.as_ref().and_then(|v|v.message.as_ref()),"managedValue":managed,
        "visit":view.as_ref().map(|v|v.visit),"serial":view.as_ref().map(|v|v.serial),
        "choiceValue":choice,"countryValue":country,
        "account":shell.core.active_account,"epoch":shell.core.account_epoch})
}

pub(crate) async fn seed(mut state: Signal<ShellState>, label: &str) -> Result<(), String> {
    if group_mode() && group_row(&state.peek()).is_some() {
        group_event(&mut state.write(), group_raw(label))?;
        return Ok(());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing mentions owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"is_admin":true,"profile":{}});
    let mut users = vec![user.clone()];
    if user_mode() {
        users.push(json!({"id":USER,"name":"username-must-not-win","team_id":owner.team,
            "real_name":"Real fallback", "profile":{"display_name":label,"real_name":"Profile fallback"}}));
    }
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Profile mentions"},"self":user,"users":users,
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true},
                    {"id":CHANNEL,"name":label,"is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Profile mentions"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    if group_mode() {
        let page: super_platinum_core::slack::models::UserGroupsPage =
            serde_json::from_value(json!({"results":[group_raw(label)]}))
                .map_err(|e| e.to_string())?;
        let mut shell = state.write();
        let team = shell.core.active_team.clone().ok_or("Missing group team")?;
        for group in page.results {
            shell
                .core
                .workspaces
                .get_mut(&team)
                .unwrap()
                .usergroups
                .insert(group.id.clone(), group);
        }
        shell.refresh_from_core();
    }
    Ok(())
}

/// The mounted editor intentionally keeps its admitted labels. This probes the
/// distinct unadmitted begin/finish boundary against a real Store invalidation.
pub(crate) fn refuse_stale_opening(shell: &mut ShellState) -> Result<Value, String> {
    let view = presentation(shell).ok_or("Missing section")?;
    let before = snapshot(shell);
    let opening = super::super::rich::begin(shell, view.visit, view.serial, "long", 9001)
        .map_err(|e| e.to_string())?;
    let team = shell.core.active_team.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    if group_mode() {
        let event = serde_json::from_value(
            json!({"type":"subteam_updated","subteam":group_raw("stale-group")}),
        )
        .map_err(|e| e.to_string())?;
        super_platinum_core::slack::models::apply_group_event(
            &mut workspace.usergroups,
            &workspace.self_user_id,
            &event,
        );
    } else if user_mode() {
        workspace.directory_members.invalidate(USER);
    } else {
        workspace.directory_channels.invalidate(CHANNEL);
    }
    let refused = !super::super::rich::finish(shell, opening);
    if group_mode() {
        // Represent a legacy cached row explicitly; never rebuild raw evidence
        // from this typed projection. Retry needs a newly observed row.
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .usergroups
            .get_mut(GROUP)
            .unwrap()
            .label_source = None;
    }
    let after = snapshot(shell);
    Ok(
        json!({"staleOpeningRefused":refused,"draftUnchanged":before["blocks"]==after["blocks"],
        "noWrite":before["attempts"]==after["attempts"]}),
    )
}

pub(crate) fn replace_account(mut state: Signal<ShellState>) -> Result<Value, String> {
    let (opening, before, epoch, media) = {
        let mut shell = state.write();
        let view = presentation(&shell).ok_or("Missing original section")?;
        let opening = super::super::rich::begin(&mut shell, view.visit, view.serial, "long", 9002)
            .map_err(|e| e.to_string())?;
        (
            opening,
            snapshot(&shell),
            shell.core.account_epoch,
            shell.media.clone(),
        )
    };
    let mut loaded = ShellState::fixture(media);
    crate::app_home::channel_fetches::fixture::install(&mut loaded);
    loaded.core.account_epoch = epoch + 1;
    loaded.core.active_account = Some("00000000-0000-4000-8000-000000000082".into());
    crate::bootstrap::install_loaded(state, loaded);
    let refused = !super::super::rich::finish(&mut state.write(), opening);
    let shell = state.peek();
    Ok(
        json!({"oldOpeningRefused":refused,"previousAccount":before["account"],"account":shell.core.active_account,
        "previousEpoch":epoch,"epoch":shell.core.account_epoch,"oldEditorHidden":presentation(&shell).is_none(),
        "previousAttempts":before["attempts"],"previousBlocks":before["blocks"]}),
    )
}

/// This initial row retains fields which labels must neither filter nor erase.
pub(crate) fn group_raw(handle: &str) -> Value {
    json!({"id":GROUP,"team_id":"TINIT0001","handle":handle,"name":"Fixture crew","date_delete":0,"is_visible":true,"fixture_metadata":{"keep":true}})
}
pub(crate) fn group_row(
    shell: &ShellState,
) -> Option<&super_platinum_core::slack::models::UserGroup> {
    shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))
        .and_then(|w| w.usergroups.get(GROUP))
}
pub(crate) fn group_event(shell: &mut ShellState, raw: Value) -> Result<(), String> {
    if !group_mode() || !enabled(shell) {
        return Err("Closed group fixture required".into());
    }
    let event = serde_json::from_value(json!({"type":"subteam_updated","subteam":raw}))
        .map_err(|e| e.to_string())?;
    let team = shell.core.active_team.clone().ok_or("Missing group team")?;
    let workspace = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing group workspace")?;
    super_platinum_core::slack::models::apply_group_event(
        &mut workspace.usergroups,
        &workspace.self_user_id,
        &event,
    );
    shell.refresh_from_core();
    Ok(())
}
