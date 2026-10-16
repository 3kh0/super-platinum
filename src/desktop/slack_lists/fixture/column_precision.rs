//! Named disconnected Number format seed. Static review or explicit owned checker.
use super::*;

pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-number-precision" | "slack-list-number-precision-narrow"
    ));
    let server = seed(shell, name);
    if super::super::rename::fixture::precision_check_enabled() {
        super::super::rename::fixture::seed_precision(shell, server);
    } else {
        let visit = generation(shell).unwrap();
        assert!(super::super::rename::precision::open_shell(
            shell, "C_NUMBER", visit
        ));
    }
}

/// Shared disconnected Number data; callers choose which existing editor to open.
pub(super) fn seed(shell: &mut ShellState, name: &str) -> Value {
    assert!(shell.core.session.is_none() && shell.core.transport.is_none());
    super::install(shell, name);
    // Separate synthetic owners make repeated pure fixture construction independent.
    shell.core.active_account = Some(format!("number-precision-fixture-{}", next_id()));
    // Reopen binds the normal owner scope to this synthetic identity.
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Number format review");
    f.document["created"] = json!(1700000000);
    f.document["user"] = json!("U1");
    f.document["list_metadata"]["schema"][2]["options"] = json!({"precision":0});
    f.document["list_metadata"]["schema"][2]["opaque"] = json!({"keep":"number-column"});
    f.document["list_metadata"]["opaque"] = json!({"keep":"metadata"});
    f.document["list_metadata"]["views"] = json!([
        {"id":"V_PRECISION","name":"Estimates","type":"table","columns":[
            {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true},
            {"id":"C_CHECK","key":"done","visible":true}],"grouping":{},"filters":[],"sorts":[],"opaque":{"keep":true}},
        {"id":"V_PRECISION_BOARD","name":"Estimate board","type":"board","columns":[
            {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":true}],
            "grouping":{"group_by":"done","group_by_column_id":"C_CHECK"},"filters":[],"sorts":[]}
    ]);
    f.document["list_metadata"]["default_view"] = json!("V_PRECISION");
    f.records["response_metadata"] = json!({"next_cursor":"precision-opaque+/=","keep":true});
    let server = f.document.clone();
    let file = serde_json::from_value(server.clone()).unwrap();
    assert!(super::super::open_shell(shell, file));
    refresh(shell);
    server
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_dialog_retains_disconnected_raw_numbers_hierarchy_cursor_and_metadata() {
        for name in [
            "slack-list-number-precision",
            "slack-list-number-precision-narrow",
        ] {
            let mut shell =
                ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(enabled(&shell));
            let f = shell.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(
                f.records["records"][5]["fields"][2]["number"],
                json!([12.5])
            );
            assert_eq!(f.records["records"][11]["parent_record_id"], "R_LIST_010");
            assert_eq!(
                f.records["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            let p = super::super::super::rename::presentation(&shell).unwrap();
            assert_eq!(p.stage, super::super::super::rename::Stage::Editing);
            assert_eq!(p.precision.unwrap().desired, 0);
            assert_eq!(f.attempts, 0);
            // Drop the static review through its normal retained registry owner.
            super::super::super::rename::precision::discard_fixture(&mut shell);
        }
    }
}
