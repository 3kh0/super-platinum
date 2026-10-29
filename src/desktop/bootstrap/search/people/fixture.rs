use super::*;
use serde_json::json;
use super_platinum_core::config::Session;
use super_platinum_core::slack::api::people_pages::AccountType;

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    #[cfg(feature = "protected-native-fixture")]
    let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
    #[cfg(not(feature = "protected-native-fixture"))]
    let transport = Arc::new(Transport::new("synthetic").unwrap());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    shell.core.active_account = Some("people-search-fixture".into());
    shell.core.transport = Some(transport);
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), session)].into(),
    });
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("alex".into());
    shell.set_search_kind(SearchKind::People);
    let request = super::super::begin(shell, "T1", "alex", false).unwrap();
    if name.contains("-loading") {
        return;
    }
    if name.contains("search-people-error") {
        shell.search_pager.fail(&request);
        return;
    }
    let users: Vec<User> = if name.contains("search-people-empty") {
        Vec::new()
    } else {
        serde_json::from_value(json!([
            {"id":"U_REMOTE","profile":{"display_name":"Alex Morgan","title":"Desktop engineer","email":"alex@example.test","pronouns":"they/them"}},
            {"id":"U2","profile":{"display_name":"Jules","title":"Release reviewer","email":"jules@example.test"}}
        ])).unwrap()
    };
    shell.search_pager.complete_lookup(&request);
    shell.search_people_results = Some(Results {
        owner: Identity::capture_account(shell).unwrap(),
        cursor: Cursor::capture(shell),
        users: users.into_iter().map(Arc::new).collect(),
    });
    if name.starts_with("search-people-paging") {
        // Presentation fixture only: the real parameter/request gates stay shut.
        shell.search_people_pager_fixture = true;
    }
    shell.search_people_account_fixture = if name.starts_with("search-people-account-type") {
        Some(if name.contains("-deactivated") {
            AccountType::Deactivated
        } else {
            AccountType::RegularMembers
        })
    } else {
        None
    };
    shell.search_people_save_fixture = if name.starts_with("search-people-saving") {
        Some(numbered::saved_sort::Fixture::Saving)
    } else if name.starts_with("search-people-save-unconfirmed") {
        Some(numbered::saved_sort::Fixture::Unconfirmed)
    } else {
        None
    };
    if name.starts_with("profile-people-read-only") {
        let source = Source::capture(shell, "U_REMOTE").unwrap();
        let Some(Open::ReadOnly(_)) = source.prepare(shell) else {
            panic!("read-only fixture selection");
        };
        let pane = shell.core.profile_pane.as_mut().unwrap();
        pane.loading = false;
        if name.contains("-error") {
            pane.error = Some("Could not refresh this profile. Please try again.".into());
        }
    }
}
