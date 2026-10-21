use super::*;
pub(super) fn combined() -> bool {
    matches!(case().as_str(), "run-named-combined" | "run-rejected")
}
pub(super) fn desired() -> Value {
    desired_for(&case())
}
pub(super) fn desired_for(case: &str) -> Value {
    if case == "run-everyone-external" {
        json!({"permission_type":"named_entities","user_ids":[],"channel_ids":[],"team_ids":["TWORKFLOW","TRUNEXTER"],"org_ids":[]})
    } else if case == "run-named-combined" {
        json!({"permission_type":"named_entities","user_ids":["UWORKFLOW","URUNADA01"],"channel_ids":["CRUNTEAM1"],"team_ids":["TRUNINNER","TRUNEXTER"],"org_ids":[]})
    } else {
        json!({"permission_type":"everyone","user_ids":[],"channel_ids":[],"team_ids":[],"org_ids":[]})
    }
}
pub(super) fn run_ack() -> Value {
    let mut v = desired();
    v["ok"] = json!(true);
    v["opaque_receipt"] = json!({"preserve":[1,"run"]});
    v
}
pub(super) fn command() -> protocol::Command {
    let d = desired();
    let ids = |k: &str| {
        d[k].as_array()
            .filter(|a| !a.is_empty())
            .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
    };
    protocol::Command::RunPermission {
        permission_type: d["permission_type"].as_str().unwrap().into(),
        user_ids: ids("user_ids"),
        channel_ids: ids("channel_ids"),
        team_ids: ids("team_ids"),
        org_ids: ids("org_ids"),
    }
}
