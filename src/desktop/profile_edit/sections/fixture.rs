//! Named disconnected layout fixture. Synthetic sections only; no credentials,
//! Transport or production write adapter is installed by this fixture.
use super::*;
pub(crate) mod check;
pub(crate) mod group_recovery;
pub(crate) mod group_recovery_check;
mod interaction;
pub(crate) mod member_freshness;
pub(crate) mod member_freshness_check;
pub(crate) mod member_known;
pub(crate) mod member_known_check;
pub(crate) mod member_recovery;
pub(crate) mod member_recovery_check;
pub(crate) mod mentions;
pub(crate) mod mentions_check;
pub(crate) mod recovery;
pub(crate) mod recovery_check;
pub(super) use interaction::{Interaction, check, load, save};
pub(crate) use interaction::{
    Outcome, complete, enabled as interaction_enabled, install_interaction,
};
pub(crate) fn install_pronunciation(shell: &mut ShellState) {
    install(shell);
    let d = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (token, visit, serial) = (d.token.clone(), d.visit, d.serial);
    let snapshot = pronunciation_snapshot(&token.owner, "Zoe (pronounced zo-ee)");
    network::finish_load(shell, &token, visit, serial, Ok(snapshot.clone()));
    shell.profile_edit.sections.draft.as_mut().unwrap().form =
        Some(protocol::SectionDraft::open_pronunciation(&snapshot).unwrap());
}
pub(super) fn pronunciation_snapshot(owner: &Owner, text: &str) -> api::OwnProfileSections {
    let raw = serde_json::json!({"ok":true,"result":{"data":{"user":{"id":owner.user,"isSelf":true,"profileSections":[{
    "sectionId":"H1","type":"HEADER","profileElements":[
        {"elementId":"pronunciation","elementKey":"Name Pronunciation","__typename":"ProfileTextElement","text":text,"permissions":{"ui":true}},
        {"elementId":"title","elementKey":"title","__typename":"ProfileTextElement","text":"Product engineer","permissions":{"ui":false}},
        {"elementId":"recording","elementKey":"Name Recording","__typename":"ProfileFileElement","fileId":"F_FIXTURE","permissions":{"ui":false}}
    ]}]}}}});
    api::OwnProfileSections::from_response(&super::super::fixture::session(owner), &raw).unwrap()
}
pub(crate) fn install(shell: &mut ShellState) {
    super::super::fixture::install(shell);
    let visit = shell.profile_edit.draft.as_ref().unwrap().visit;
    let (token, serial) = open_shell(shell, visit).unwrap();
    let mut fields = vec![
        serde_json::json!({"elementId":"text","__typename":"ProfileTextElement","label":"Focus","text":"Product engineering","hint":"What you work on"}),
        serde_json::json!({"elementId":"choice","__typename":"ProfileTextElement","label":"Working hours","text":"Morning","options":["Morning","Afternoon","Evening"]}),
        serde_json::json!({"elementId":"date","__typename":"ProfileDateElement","label":"Start date","date":"2026-06-15"}),
        serde_json::json!({"elementId":"link","__typename":"ProfileLinkElement","label":"Portfolio","displayText":"My work","uri":"https://example.com"}),
        serde_json::json!({"elementId":"tags","__typename":"ProfileTagsElement","label":"Skills","tags":["Rust","Design systems"]}),
        serde_json::json!({"elementId":"persons","__typename":"ProfilePersonElement","label":"Collaborators","persons":[],"isScimManaged":false}),
        serde_json::json!({"elementId":"location","__typename":"ProfileCurrentLocationElement","label":"Current location","city":"New York","state":"NY","country":"US","description":"Home office"}),
        serde_json::json!({"elementId":"long","__typename":"ProfileLongTextElement","label":"About me","blocks":"[{\"type\":\"rich_text\",\"elements\":[{\"type\":\"rich_text_section\",\"elements\":[{\"type\":\"text\",\"text\":\"Building useful tools\",\"style\":{\"bold\":true}}]}]}]"}),
        serde_json::json!({"elementId":"locked","__typename":"ProfileTextElement","label":"Managed field","text":"Preserved organization value","permissions":{"ui":false}}),
    ];
    for field in &mut fields {
        field["id"] = serde_json::json!(format!(
            "{}:section:S1:{}",
            token.owner.user,
            field["elementId"].as_str().unwrap()
        ));
        if field.get("permissions").is_none() {
            field["permissions"] = serde_json::json!({"ui":true});
        }
    }
    let raw = serde_json::json!({"ok":true,"result":{"data":{"user":{"id":token.owner.user,"profileSections":[
        {"sectionId":"S1","id":format!("{}:section:S1",token.owner.user),"type":"CUSTOM","label":"About you","canEdit":true,"profileElements":fields}]}}}});
    let snapshot =
        api::OwnProfileSections::from_response(&super::super::fixture::session(&token.owner), &raw)
            .unwrap();
    network::finish_load(shell, &token, visit, serial, Ok(snapshot.clone()));
    shell.profile_edit.sections.draft.as_mut().unwrap().form =
        Some(protocol::SectionDraft::open(&snapshot, "S1").unwrap());
}
