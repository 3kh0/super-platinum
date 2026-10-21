//! Isolated channel-reference cells with exact predecessor restoration.
use super::*;
use super_platinum_core::slack::models::Channel;

pub(crate) struct ChannelEditSnapshot {
    document: Value,
    records: Value,
    channels: std::collections::BTreeMap<String, Channel>,
}
pub(crate) fn add_channel_edits(shell: &mut ShellState) -> Option<ChannelEditSnapshot> {
    if !enabled(shell) {
        return None;
    }
    let team = shell.core.active_team.clone()?;
    let workspace = shell.core.workspaces.get_mut(&team)?;
    let channels = workspace.channels.clone();
    for raw in [
        json!({"id":"C00000001","name":"alpha-public","is_channel":true,"is_member":false}),
        json!({"id":"G00000002","name":"beta-private","is_group":true,"is_private":true,"is_member":false}),
        json!({"id":"C00000003","name":"gamma-external","is_channel":true,"is_ext_shared":true,"shared_team_ids":["T_OTHER"]}),
        json!({"id":"C00000004","name":"old-archived","is_channel":true,"is_archived":true}),
        json!({"id":"D00000005","name":"direct-message","is_im":true}),
        json!({"id":"G00000006","name":"group-message","is_group":true,"is_mpim":true}),
        json!({"id":"C00000007","name":"unknown-type"}),
    ] {
        let channel: Channel = serde_json::from_value(raw).ok()?;
        workspace.channels.insert(channel.id.clone(), channel);
    }
    let fixture = shell.slack_lists.fixture.as_mut()?;
    let snapshot = ChannelEditSnapshot {
        document: fixture.document.clone(),
        records: fixture.records.clone(),
        channels,
    };
    fixture.document["list_metadata"]["schema"].as_array_mut()?.extend([
  json!({"id":"C_EDIT_CHANNEL","key":"edit_channel","name":"Channel","type":"channel","options":{"format":"single_entity"}}),
  json!({"id":"C_EDIT_CHANNELS","key":"edit_channels","name":"Related channels","type":"channel","options":{"format":"multi_entity"}}),
 ]);
    fixture.document["list_metadata"]["views"].as_array_mut()?.push(json!({"id":"V_EDIT_CHANNELS","name":"Channel editing","type":"table","position":"502","columns":[
  {"id":"C_NAME","key":"NAME","visible":true},{"id":"C_USER","key":"assignee","visible":true},{"id":"C_EDIT_CHANNEL","key":"edit_channel","visible":true},{"id":"C_EDIT_CHANNELS","key":"edit_channels","visible":true}
 ],"filters":[{"key":"estimate","column_id":"C_NUMBER","operator":"less_than_or_equal","values":[2]}],"sorts":[]}));
    for record in fixture.records["records"].as_array_mut()? {
        if record["id"] == "R_LIST_000" {
            let fields = record["fields"].as_array_mut()?;
            for field in fields.iter_mut().filter(|field| field["key"] == "estimate") {
                field["number"] = json!([0]);
            }
            fields.extend([
    json!({"column_id":"C_EDIT_CHANNEL","key":"edit_channel","channel":["C00000001"]}),
    json!({"column_id":"C_EDIT_CHANNELS","key":"edit_channels","channel":["C00000001","Z99999999"]}),
   ]);
        }
    }
    refresh(shell);
    Some(snapshot)
}
pub(crate) fn channel_edit_value(shell: &ShellState, key: &str) -> Option<Value> {
    if !enabled(shell) {
        return None;
    }
    shell.slack_lists.fixture.as_ref()?.records["records"]
        .as_array()?
        .iter()
        .find(|record| record["id"] == "R_LIST_000")?["fields"]
        .as_array()?
        .iter()
        .find(|field| field["key"] == key)?
        .get("channel")
        .cloned()
}
pub(crate) fn restore_channel_edits(shell: &mut ShellState, snapshot: ChannelEditSnapshot) {
    if !enabled(shell) {
        return;
    }
    if let Some(workspace) = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get_mut(team))
    {
        workspace.channels = snapshot.channels;
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document = snapshot.document;
    fixture.records = snapshot.records;
    refresh(shell);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_keeps_target_visible_after_prior_edits_and_restores_every_record_and_channel() {
        let mut shell =
            ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
        let fixture = shell.slack_lists.fixture.as_mut().unwrap();
        let record = fixture.records["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["id"] == "R_LIST_000")
            .unwrap();
        record["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|field| field["key"] == "estimate")
            .unwrap()["number"] = json!([5]);
        let records = fixture.records.clone();
        let channels = serde_json::to_value(
            &shell.core.workspaces[&shell.core.active_team.clone().unwrap()].channels,
        )
        .unwrap();
        let snapshot = add_channel_edits(&mut shell).unwrap();
        let view = shell.slack_lists.view.as_ref().unwrap();
        let (visit, layout) = (view.generation, view.layout.generation);
        assert!(super::super::super::layout::select_shell(
            &mut shell,
            Some("V_EDIT_CHANNELS".into()),
            visit,
            layout
        ));
        assert!(
            shell
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .any(|row| row.id == "R_LIST_000"
                    && row
                        .cells
                        .iter()
                        .any(|cell| cell.column_id == "C_EDIT_CHANNEL" && cell.can_edit))
        );
        restore_channel_edits(&mut shell, snapshot);
        assert_eq!(shell.slack_lists.fixture.as_ref().unwrap().records, records);
        assert_eq!(
            serde_json::to_value(
                &shell.core.workspaces[&shell.core.active_team.clone().unwrap()].channels
            )
            .unwrap(),
            channels
        );
    }
}
