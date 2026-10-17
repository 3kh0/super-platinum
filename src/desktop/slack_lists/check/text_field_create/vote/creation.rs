//! Real Vote Type and emoji callbacks; default/reset and custom creation stay distinct.
use super::super::*;
fn case(mode: FieldCreateMode) -> &'static str {
    match mode {
        FieldCreateMode::VoteDefault => "default",
        FieldCreateMode::VoteCustom => "custom",
        _ => unreachable!(),
    }
}
async fn open_picker(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let before = draft(state)?;
    let click = checked(d, "window.__listVote.open()").await?;
    let ready = wait_dom(d, "window.__listVote.pickerReady()").await?;
    let current = draft(state)?;
    if current["votePicker"]
        .as_u64()
        .map(|n| n.to_string())
        .as_deref()
        != ready["token"].as_str()
        || current["voteOptions"] != before["voteOptions"]
        || current["choice"] != before["choice"]
        || current["serial"] != before["serial"]
        || current["visit"] != before["visit"]
        || current["selection"] != before["selection"]
    {
        return Err("Vote picker opened under wrong custody".into());
    }
    let search = checked(d, &format!("window.__listVote.search({})", json!(name))).await?;
    let result = wait_dom(d, &format!("window.__listVote.result({})", json!(name))).await?;
    if draft(state)? != current {
        return Err("Vote picker changed original draft".into());
    }
    Ok(
        json!({"before":before,"click":click,"ready":ready,"search":search,"result":result,"draft":current}),
    )
}
async fn choose(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    options: Value,
) -> Result<Value, String> {
    let before = draft(state)?;
    let event = checked(d, &format!("window.__listVote.choose({})", json!(name))).await?;
    let returned = wait_dom(d, "window.__listVote.returned()").await?;
    let after = draft(state)?;
    if after["votePicker"] != Value::Null
        || after["voteOptions"] != options
        || after["choice"] == before["choice"]
        || after["serial"] != before["serial"]
        || after["visit"] != before["visit"]
        || after["selection"] != before["selection"]
        || after["name"] != before["name"]
    {
        return Err("Vote choice did not update exact options/custody".into());
    }
    Ok(json!({"event":event,"returned":returned,"before":before,"after":after}))
}
async fn kind(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    want: &str,
    changed: bool,
) -> Result<Value, String> {
    let before = draft(state)?;
    let event = checked(
        d,
        &format!("window.__listTextCreate.selectKind({})", json!(want)),
    )
    .await?;
    for _ in 0..150 {
        let after = draft(state)?;
        if after["kind"] == want && (after["choice"] != before["choice"]) == changed {
            let dom = wait_dom(
                d,
                &format!(
                    "window.__listTextCreate.choiceReady({},{})",
                    json!(want),
                    json!(
                        after["choice"]
                            .as_u64()
                            .ok_or("Missing type epoch")?
                            .to_string()
                    )
                ),
            )
            .await?;
            if draft(state)? != after
                || after["serial"] != before["serial"]
                || after["name"] != before["name"]
                || after["visit"] != before["visit"]
                || after["selection"] != before["selection"]
            {
                return Err("Vote type callback custody changed".into());
            }
            return Ok(json!({"before":before,"event":event,"after":after,"dom":dom}));
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Vote type callback did not settle".into())
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    name: &str,
    dom: Value,
) -> Result<(), String> {
    if proof.stages.len() >= 2 {
        return Err("Extra Vote prelude capture".into());
    }
    let before = draft(state)?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    if ledger["requests"] != json!([])
        || ledger["responses"] != json!([])
        || ledger["finished"] != json!([])
    {
        return Err("Vote prelude dispatched".into());
    }
    proof.stable(state, 0)?;
    let capture = harness::capture(d, name).await?;
    proof.stable(state, 0)?;
    if draft(state)? != before || rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Vote prelude changed during capture".into());
    }
    proof.captures.insert(name.into(), capture);
    proof.stages.push(json!({"ordinal":proof.stages.len(),"stage":name,"geometry":dom,"draft":before,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"noSession":true,"noTransport":true}));
    Ok(())
}
pub(crate) async fn prelude(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listVote.init({})", json!(case(proof.mode))),
    )
    .await?;
    let initial = draft(state)?;
    let default = json!({"emoji":":sparkling_heart:"});
    if initial["voteOptions"] != default || initial["votePicker"] != Value::Null {
        return Err("Vote default differs".into());
    }
    let dom = wait_dom(d, "window.__listVote.inspect(':sparkling_heart:',true)").await?;
    stage(state, d, proof, "vote-default", dom).await?;
    let same = kind(state, d, "vote", false).await?;
    if same["after"]["voteOptions"] != default {
        return Err("Same Vote type reset draft".into());
    }
    let first = open_picker(state, d, "heart").await?;
    let dismiss = checked(d, "window.__listVote.dismiss('close')").await?;
    let restored = wait_dom(d, "window.__listVote.returned()").await?;
    if draft(state)?["voteOptions"] != default
        || draft(state)?["choice"] != first["before"]["choice"]
    {
        return Err("Vote picker Cancel mutated options".into());
    }
    let second = open_picker(state, d, "heart").await?;
    if first["draft"]["votePicker"] == second["draft"]["votePicker"] {
        return Err("Reused Vote picker token".into());
    }
    let standard = choose(
        state,
        d,
        "heart",
        json!({"emoji":":heart:","emoji_team_id":"T1"}),
    )
    .await?;
    let retained = kind(state, d, "vote", false).await?;
    if retained["after"]["voteOptions"] != standard["after"]["voteOptions"] {
        return Err("Same kind lost configured Vote".into());
    }
    let away = kind(state, d, "text", true).await?;
    let back = kind(state, d, "vote", true).await?;
    if back["after"]["voteOptions"] != default {
        return Err("Vote type reset failed".into());
    }
    let configured = configure(state, d, proof.mode).await?;
    stage(
        state,
        d,
        proof,
        "vote-configured",
        configured["verified"].clone(),
    )
    .await?;
    Ok(
        json!({"case":case(proof.mode),"initial":initial,"same":same,"first":first,"dismiss":dismiss,"restored":restored,"second":second,"standard":standard,"retained":retained,"away":away,"back":back,"configured":configured,"after":draft(state)?}),
    )
}
pub(crate) async fn configure(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let before = draft(state)?;
    if before["voteOptions"] != json!({"emoji":":sparkling_heart:"}) {
        return Err("Vote reopen did not reset default".into());
    }
    let picker = if mode == FieldCreateMode::VoteCustom {
        let opened = open_picker(state, d, "fixture_vote").await?;
        let chosen = choose(state, d, "fixture_vote", mode.vote_options()).await?;
        json!({"opened":opened,"chosen":chosen})
    } else {
        Value::Null
    };
    let verified = verify(state, d, mode).await?;
    let focus = checked(d, "window.__listVote.focusName()").await?;
    Ok(
        json!({"before":before,"picker":picker,"verified":verified,"focus":focus,"after":draft(state)?}),
    )
}
pub(crate) async fn verify(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let before = draft(state)?;
    let expected = mode.vote_options();
    let editing = rename::presentation(&state.peek()).is_some_and(|p| p.stage == Stage::Editing);
    let dom = wait_dom(
        d,
        &format!("window.__listVote.inspect({},{editing})", expected["emoji"]),
    )
    .await?;
    if before["voteOptions"] != expected
        || before["votePicker"] != Value::Null
        || dom["choice"].as_str() != before["choice"].as_u64().map(|n| n.to_string()).as_deref()
        || draft(state)? != before
    {
        return Err("Vote options DOM/model custody differs".into());
    }
    Ok(dom)
}
