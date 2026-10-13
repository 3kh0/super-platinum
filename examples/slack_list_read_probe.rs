//! Read-only scoped Lists compatibility probe. Prints structural metadata, never auth or cell text.
use serde_json::{Value, json};
use super_platinum_core::{
    config,
    slack::{SlackClient, Transport, api},
    slack_lists::{Document, RecordsPage},
};
fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}
fn shape(value: &Value) -> Value {
    match value {
        Value::Array(values) => json!({"array":values.len(),"first":values.first().map(shape)}),
        Value::Object(_) => json!({"object_keys":keys(value)}),
        Value::Null => json!("null"),
        Value::Bool(_) => json!("bool"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
    }
}
async fn run() -> Result<(), String> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let verification_fields = args
        .last()
        .is_some_and(|arg| arg == "--verification-fields");
    let first_row_rich = args.last().is_some_and(|arg| arg == "--first-row-rich");
    let views = args.last().is_some_and(|arg| arg == "--views");
    let dates = args.last().is_some_and(|arg| arg == "--dates");
    if first_row_rich || views || dates || verification_fields {
        args.pop();
    }
    if args.len() != 4 {
        return Err(
            "Usage: slack_list_read_probe FILE_ID WORKSPACE_ID PRIVATE_CHANNEL_ID CHANNEL_NAME [--first-row-rich|--views|--dates|--verification-fields]"
                .into(),
        );
    }
    let [id, workspace_id, channel_id, channel_name] = args.as_slice() else {
        unreachable!()
    };
    if (first_row_rich || views || dates || verification_fields) && id != "F0C8MDD6B8D" {
        return Err("Detailed output is restricted to the newly created verification List.".into());
    }
    if !id.starts_with('F')
        || ![id, workspace_id, channel_id]
            .iter()
            .all(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return Err("Malformed observed IDs".into());
    }
    let session = config::load_session()
        .map_err(|_| "Could not load existing session")?
        .ok_or("No existing session")?;
    let workspace = session
        .workspaces
        .get(workspace_id)
        .ok_or("Requested workspace missing from current account")?;
    let transport =
        Transport::new(session.d_cookie.clone()).map_err(|_| "Transport setup failed")?;
    let client = SlackClient::default();
    let channel = transport
        .execute(client.rest_form(
            workspace,
            "conversations.info",
            vec![("channel", channel_id.clone())],
        ))
        .await
        .map_err(|_| "Channel verification failed")?;
    let channel = &channel["channel"];
    if channel["id"] != *channel_id
        || channel["name"] != *channel_name
        || channel["is_private"] != true
        || channel["is_member"] != true
    {
        return Err("Private test channel guard failed".into());
    }
    let experiments = api::fetch_list_experiments(&transport, &client, workspace).await;
    println!(
        "{}",
        match experiments {
            Ok(flags) => json!({"stage":"list_experiments","known":true,
            "grid_todos":flags.grid_todos,"todos":flags.todos,"todos_rollback":flags.todos_rollback,
            "default_view_dual_write":flags.default_view_dual_write}),
            Err(_) => json!({"stage":"list_experiments","known":false}),
        }
    );
    let response = transport
        .execute(api::file_info(&client, workspace, &id))
        .await
        .map_err(|_| "File read failed")?;
    let file = &response["file"];
    if file["id"] != *id {
        return Err("File identity mismatch".into());
    }
    if verification_fields {
        println!(
            "{}",
            json!({"stage":"verification_limits","file_id":id,
            "list_limits":file["list_limits"]})
        );
    }
    if views || dates {
        println!(
            "{}",
            json!({"stage":"saved_views", "file_id":id,
            "views":file["list_metadata"]["views"],
            "default_view":file["list_metadata"]["default_view"],
            "todo_mode":file["list_metadata"]["todo_mode"],
            "schema":file["list_metadata"]["schema"]})
        );
    }
    let tabs = channel
        .get("properties")
        .and_then(|p| p.get("tabs"))
        .cloned()
        .unwrap_or(Value::Null);
    println!(
        "{}",
        json!({"stage":"file","workspace":workspace.team_id,"channel_verified":true,"file_id":id,"file_keys":keys(file),"filetype":file["filetype"],"mode":file["mode"],"access":file["access"],"metadata_keys":keys(&file["list_metadata"]),"tabs_shape":shape(&tabs),"schema":file["list_metadata"]["schema"].as_array().map(|columns|columns.iter().map(|c|json!({"id":c["id"],"key":c["key"],"name":c["name"],"type":c["type"],"keys":keys(c)})).collect::<Vec<_>>()),"parse":Document::parse(file.clone()).map(|d|json!({"columns":d.columns.len()})).unwrap_or_else(|e|json!({"error":e.to_string()}))})
    );
    let records = transport
        .execute(api::lists_records_list(&client, workspace, &id))
        .await
        .map_err(|_| "Records read failed")?;
    if dates {
        let page = RecordsPage::parse(records.clone()).map_err(|_| "Records parse failed")?;
        println!(
            "{}",
            json!({"stage":"verification_dates","file_id":id,
            "rows":page.records.iter().map(|record|json!({"id":record.id,
                "due_date":record.fields.get("todo_due_date")})).collect::<Vec<_>>()})
        );
    }
    if verification_fields {
        let page = RecordsPage::parse(records.clone()).map_err(|_| "Records parse failed")?;
        println!(
            "{}",
            json!({"stage":"verification_fields","file_id":id,"rows":page.records.iter().map(|record|json!({"id":record.id,"fields":record.fields})).collect::<Vec<_>>()})
        );
    }
    if first_row_rich {
        let page = RecordsPage::parse(records.clone()).map_err(|_| "Records parse failed")?;
        let ordered = page.ordered(None);
        let first = ordered.first().ok_or("Verification List has no rows")?;
        println!(
            "{}",
            json!({"stage":"first_row_rich", "file_id":id,
            "record_id":first.id, "field":first.fields.get("name")})
        );
    }
    println!(
        "{}",
        json!({"stage":"records","keys":keys(&records),"records_shape":shape(&records["records"]),"first_record":records["records"].as_array().and_then(|r|r.first()).map(|r|json!({"keys":keys(r),"fields":r["fields"].as_array().map(|fs|fs.iter().map(|f|json!({"key":f["key"],"keys":keys(f),"shapes":f.as_object().map(|o|o.iter().filter(|(k,_)|*k!="key").map(|(k,v)|(k.clone(),shape(v))).collect::<serde_json::Map<_,_>>())})).collect::<Vec<_>>())})),"parse":RecordsPage::parse(records).map(|p|json!({"rows":p.records.len(),"complete":p.complete()})).unwrap_or_else(|e|json!({"error":e.to_string()}))})
    );
    let permission = api::fetch_list_allow_lists(&transport, &client, workspace).await;
    let error = match &permission {
        Err(super_platinum_core::slack::Error::Api(code))
            if code.len() < 96 && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
        {
            Some(code.clone())
        }
        Err(_) => Some("permission_lookup_failed".into()),
        _ => None,
    };
    println!(
        "{}",
        json!({"stage":"permission","ok":permission.is_ok(),"allow_lists":permission.as_ref().ok().cloned().flatten(),"error":error})
    );
    let task_policy = api::fetch_list_personal_task_lists(&transport, &client, workspace).await;
    println!(
        "{}",
        json!({"stage":"personal_task_policy","ok":task_policy.is_ok(),
            "requested_list_is_personal":task_policy.as_ref().ok().map(|policy|policy.contains(id))})
    );
    Ok(())
}
fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    if let Err(reason) = runtime.block_on(run()) {
        eprintln!("read probe: {reason}");
        std::process::exit(1);
    }
}
