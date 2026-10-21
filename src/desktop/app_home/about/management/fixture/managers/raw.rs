//! Immutable original producer vectors, separate from observed ACK and readback.
use super::*;
pub(super) const OLD: &str = "UMANOLD01";
pub(super) const NEW: &str = "UMANNEW01";
pub(super) const COLD: &str = "UMANCOLD1";
pub(super) fn original_ids() -> Vec<&'static str> {
    vec![
        "UWORKFLOW",
        if case() == "managers-cold" { COLD } else { OLD },
    ]
}
pub(super) fn self_remove() -> bool {
    matches!(
        case().as_str(),
        "managers-self-success" | "managers-self-rejected-held"
    )
}
pub(super) fn copy_changed() -> bool {
    matches!(
        case().as_str(),
        "managers-combined"
            | "managers-add-rejected"
            | "managers-add-unknown"
            | "managers-self-rejected-held"
    )
}
pub(super) fn run_changed() -> bool {
    matches!(
        case().as_str(),
        "managers-combined" | "managers-self-rejected-held" | "managers-agents"
    )
}
pub(super) fn original_run() -> Value {
    if case() == "managers-no-trigger" {
        Value::Null
    } else if matches!(case().as_str(), "managers-combined" | "managers-agents") {
        json!({"ok":true,"permission_type":"named_entities","user_ids":["UWORKFLOW",OLD],"channel_ids":[],"team_ids":[],"org_ids":[]})
    } else {
        json!({"ok":true,"permission_type":"app_collaborators"})
    }
}
pub(super) fn run_command() -> protocol::Command {
    protocol::Command::RunPermission {
        permission_type: if matches!(case().as_str(), "managers-combined" | "managers-agents") {
            "named_entities"
        } else {
            "everyone"
        }
        .into(),
        user_ids: if matches!(case().as_str(), "managers-combined" | "managers-agents") {
            Some(desired_run_ids().into_iter().map(str::to_owned).collect())
        } else {
            None
        },
        channel_ids: None,
        team_ids: None,
        org_ids: None,
    }
}
pub(super) fn desired_run() -> Value {
    if matches!(case().as_str(), "managers-combined" | "managers-agents") {
        json!({"ok":true,"permission_type":"named_entities","user_ids":desired_run_ids(),"channel_ids":[],"team_ids":[],"org_ids":[]})
    } else {
        json!({"ok":true,"permission_type":"everyone","user_ids":[],"channel_ids":[],"team_ids":[],"org_ids":[]})
    }
}
pub(super) fn run_ack() -> Value {
    let mut v = desired_run();
    v["opaque_receipt"] = json!({"preserve":[1,"run"]});
    v
}
pub(super) fn available() -> Value {
    json!({"manager_receipt":{"preserve":[1,"available"]}})
}
pub(super) fn expression() -> Value {
    json!({"manager_receipt":{"preserve":[2,"expression"]}})
}
pub(super) fn latest(workflow: &Value) -> Value {
    json!({"ok":true,"decorated_workflow":workflow,"available_data":available(),"expression_data":expression()})
}
pub(super) fn ack(workflow: &Value) -> Value {
    let mut v = latest(workflow);
    v["opaque_receipt"] = json!({"preserve":[3,"manager"]});
    v
}
pub(super) fn person(id: &str, label: &str) -> Value {
    json!({"id":id,"name":label.to_lowercase().replace(' ',"_"),"real_name":label,"team_id":"TWORKFLOW","updated":101,"deleted":false,"is_bot":false,"is_restricted":false,"is_ultra_restricted":false,"is_stranger":false,"profile":{"real_name":label,"real_name_normalized":label,"display_name":label,"display_name_normalized":label}})
}
pub(super) fn old() -> Value {
    person(OLD, "Original Manager")
}
pub(super) fn new() -> Value {
    person(NEW, "New Manager")
}
pub(super) fn cold() -> Value {
    person(COLD, "Cold Manager")
}

pub(super) fn query_rows() -> Vec<Value> {
    let mut rows = vec![new()];
    let mut enterprise = person("UMANENTER", "Enterprise Manager");
    enterprise["team_id"] = json!("TOTHER");
    enterprise["enterprise_user"] = json!({"teams":["TWORKFLOW"]});
    enterprise["is_stranger"] = json!(true);
    rows.push(enterprise);
    let mut local = person("UMANLOCAL", "Local Manager");
    local["is_stranger"] = json!(true);
    rows.push(local);
    for (id, label, key) in [
        ("UMANBOT01", "Bot Manager", "is_bot"),
        ("UMANDEAD1", "Deleted Manager", "deleted"),
        ("UMANGUEST", "Guest Manager", "is_restricted"),
        ("UMANPROF1", "Profile Manager", "is_profile_only_user"),
        ("UMANFLOW1", "Workflow Manager", "is_workflow_bot"),
    ] {
        let mut row = person(id, label);
        row[key] = json!(true);
        rows.push(row);
    }
    let mut remote = person("UMANAWAY1", "Remote Manager");
    remote["team_id"] = json!("TOTHER");
    rows.push(remote);
    rows.push(person("USLACKBOT", "Slackbot Manager"));
    rows
}

pub(super) fn added_ids() -> Vec<&'static str> {
    if case() == "managers-agents" {
        vec![NEW, "UMANAFYES", "UMANAFDEF"]
    } else {
        vec![NEW]
    }
}
fn desired_run_ids() -> Vec<&'static str> {
    let mut ids = vec!["UWORKFLOW"];
    ids.extend(added_ids());
    ids.push(OLD);
    ids
}
