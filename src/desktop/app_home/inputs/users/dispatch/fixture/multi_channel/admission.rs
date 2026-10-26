//! Multi-channel occurrences against unseen Date custody and real Reset retirement.
use super::super::lanes::admission::{acknowledged, date_buffer};
use super::*;

pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    let day = before["nativeMounts"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|e| e["block"] == "day" && e["action"] == "value")
        })
        .ok_or("Current Date registry entry missing")?
        .clone();
    let unseen = date_buffer(d, &day, None).await?;
    let choice = toggle(s, d, "pick", BETA, &[ALPHA, BETA]).await?;
    refused(s, 1).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused_model = snapshot(&s.peek());
    if refused_model["formState"]["values"]["day"] != before["formState"]["values"]["day"]
        || refused_model["refusals"][0]["reason"] != "sibling-custody"
    {
        return Err("Unseen Date buffer did not refuse channel sibling custody".into());
    }
    let correction = date_buffer(d, &day, Some(&unseen["before"])).await?;
    let ack = acknowledged(d, &correction).await?;
    if correction["after"]["seq"].as_u64() != correction["before"]["seq"].as_u64().map(|v| v + 1) {
        return Err("Date correction did not publish exactly one input observation".into());
    }
    shot(s,d,n,"unseen-date-refused",json!({"before":before,"unseen":unseen,"choice":choice,"refused":refused_model,"correction":correction,"ack":ack}),st,cap).await?;

    let fresh = toggle(s, d, "pick", ALPHA, &[BETA]).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "fresh-choice", fresh, st, cap).await?;

    let quiet = toggle(s, d, "quiet", BETA, &[ALPHA, BETA]).await?;
    let single = choose(s, d, "single", BETA).await?;
    let conversation = choose(s, d, "conversation", BETA).await?;
    let conversations = choose(s, d, "conversations", BETA).await?;
    let notes = notes("Admission latest sibling").await?;
    let mut notes_received = false;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["notes"]["value"]["value"]
            == "Admission latest sibling"
        {
            notes_received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    if !notes_received {
        return Err("Quiet notes input was not acknowledged".into());
    }
    let latest = snapshot(&s.peek());
    let values = &latest["formState"]["values"];
    if values["pick"]["value"]["selected_channels"] != json!([BETA])
        || values["quiet"]["value"]["selected_channels"] != json!([ALPHA, BETA])
        || values["single"]["value"]["selected_channel"] != BETA
        || values["conversation"]["value"]["selected_conversation"] != BETA
        || values["conversations"]["value"]["selected_conversations"] != json!([ALPHA, BETA])
    {
        return Err(
            "Button siblings did not commit their ordered channel/conversation values".into(),
        );
    }
    let clicked = click(s, d).await?;
    counts(s, 1, 1, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Admission Button reply was not held".into());
    }
    counts(s, 1, 1, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "button-latest",
        json!({"quiet":quiet,"single":single,"conversation":conversation,"conversations":conversations,"notes":notes,"click":clicked}),
        st,
        cap,
    )
    .await?;

    let held = hold_directory(s, d).await?;
    shot(s, d, n, "directory-held", held, st, cap).await?;
    let before_reset = snapshot(&s.peek());
    let reset = reset(s, d).await?;
    let mut retired = false;
    for _ in 0..250 {
        if directory::pending_closed(&s.peek()) {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !retired {
        return Err("Reset did not cancel the owned directory receiver".into());
    }
    let release = directory::release(&mut s.write());
    settle(s, d).await?;
    let after = snapshot(&s.peek());
    let values = &after["formState"]["values"];
    if release["delivered"] != false
        || !after["pickerBlock"].is_null()
        || values["pick"]["value"]["selected_channels"] != json!([ALPHA])
        || values["quiet"]["value"]["selected_channels"] != json!([ALPHA])
        || values["required"]["value"] != json!({"type":"multi_channels_select"})
        || values["single"]["value"]["selected_channel"] != ALPHA
        || values["conversation"]["value"]["selected_conversation"] != ALPHA
        || values["conversations"]["value"]["selected_conversations"] != json!([ALPHA])
        || values["notes"]["value"]["value"] != "Sibling"
        || values["day"]["value"]["selected_date"] != Value::Null
        || !after["nativeMounts"].as_array().is_some_and(|rows| {
            rows.iter().any(|r| {
                r["block"] == "day"
                    && r["value"] == ""
                    && r["pending"] == false
                    && r["invalid"] == false
            })
        })
        || after["requests"] != before_reset["requests"]
        || after["buttonSync"]["requests"] != before_reset["buttonSync"]["requests"]
        || after["directory"]["requests"].as_array().map(Vec::len) != Some(1)
        || after["refusals"].as_array().map(Vec::len) != Some(1)
    {
        return Err(
            "Reset did not reseed channel/conversation state or admitted the retired response"
                .into(),
        );
    }
    for (collection, block, visible) in [
        ("multiChannelWidgets", "pick", json!([ALPHA])),
        ("multiChannelWidgets", "quiet", json!([ALPHA])),
        ("multiChannelWidgets", "required", json!([])),
        ("channelWidgets", "single", json!(ALPHA)),
        ("sharedLookupWidgets", "single", json!(ALPHA)),
        ("sharedLookupWidgets", "conversation", json!(ALPHA)),
        ("sharedLookupMultiples", "pick", json!([ALPHA])),
        ("sharedLookupMultiples", "quiet", json!([ALPHA])),
        ("sharedLookupMultiples", "required", json!([])),
        ("sharedLookupMultiples", "conversations", json!([ALPHA])),
    ] {
        let widget = |model: &Value| {
            model[collection]
                .as_array()
                .and_then(|rows| rows.iter().find(|w| w["block"] == block))
                .cloned()
        };
        let old = widget(&before_reset).ok_or("Reset prior lookup widget missing")?;
        let new = widget(&after).ok_or("Reset replacement lookup widget missing")?;
        if new["visible"] != visible
            || new["current"] != true
            || new["revision"].as_u64().is_none()
            || new["revision"] == old["revision"]
            || (matches!(collection, "channelWidgets" | "multiChannelWidgets")
                && (new["instance"].as_u64().is_none() || new["instance"] == old["instance"]))
        {
            return Err(format!(
                "Reset reused stale lookup widget {collection}/{block}"
            ));
        }
    }
    counts(s, 1, 1, 1, 1).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-retired",
        json!({"before":before_reset,"reset":reset,"release":release,"silence":quiet}),
        st,
        cap,
    )
    .await
}

async fn hold_directory(mut s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    directory::configure(&mut s.write(), directory::Reply::Channels, true);
    let before = snapshot(&s.peek());
    let count = before["directory"]["requests"]
        .as_array()
        .ok_or("Directory ledger missing")?
        .len();
    let opened = open(d, "pick").await?;
    let selector = "#app-home [data-modal-users='pick'] input[type=search]";
    let input = capture::dom(&format!(r#"(()=>{{{}const n=document.querySelector({});n?.scrollIntoView({{block:'center'}});if(!owned(n))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,'held-reset');n.dispatchEvent(new Event('input',{{bubbles:true}}));return{{ok:true,value:n.value,trusted:false,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD,json!(selector))).await?;
    for _ in 0..250 {
        let x = snapshot(&s.peek());
        if x["directory"]["requests"].as_array().map(Vec::len) == Some(count + 1)
            && x["directory"]["pending"] == true
            && x["directory"]["query"]["loading"] == true
            && x["directory"]["query"]["current"] == true
        {
            return Ok(
                json!({"open":opened,"input":input,"beforeRequests":count,"held":x["directory"]}),
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Channel Reset directory request was not held".into())
}
