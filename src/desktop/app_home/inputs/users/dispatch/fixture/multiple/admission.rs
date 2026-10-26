//! Multi-user occurrences against unseen Date custody and real Reset retirement.
use super::super::lanes::admission::{acknowledged, date_buffer, hold_directory};
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
    let choice = toggle(s, d, "pick", BLAIR, &[ADA, BLAIR]).await?;
    refused(s, 1).await?;
    counts(s, 0, 0, 0, 0).await?;
    let refused_model = snapshot(&s.peek());
    if refused_model["formState"]["values"]["day"] != before["formState"]["values"]["day"]
        || refused_model["refusals"][0]["reason"] != "sibling-custody"
    {
        return Err("Unseen Date buffer did not refuse user sibling custody".into());
    }
    let correction = date_buffer(d, &day, Some(&unseen["before"])).await?;
    let ack = acknowledged(d, &correction).await?;
    if correction["after"]["seq"].as_u64() != correction["before"]["seq"].as_u64().map(|v| v + 1) {
        return Err("Date correction did not publish exactly one input observation".into());
    }
    shot(s,d,n,"unseen-date-refused",json!({"before":before,"unseen":unseen,"choice":choice,"refused":refused_model,"correction":correction,"ack":ack}),st,cap).await?;

    let fresh = toggle(s, d, "pick", ADA, &[BLAIR]).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "fresh-choice", fresh, st, cap).await?;

    let quiet = toggle(s, d, "quiet", BLAIR, &[ADA, BLAIR]).await?;
    let single = choose(s, d, "single", CASEY).await?;
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
        json!({"quiet":quiet,"single":single,"notes":notes,"click":clicked}),
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
        if people::pending_closed(&s.peek()) {
            retired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !retired {
        return Err("Reset did not cancel the owned directory receiver".into());
    }
    let release = people::release(&mut s.write());
    settle(s, d).await?;
    let after = snapshot(&s.peek());
    if release["delivered"] != false
        || !after["pickerBlock"].is_null()
        || after["formState"]["values"]["pick"]["value"]["selected_users"] != json!([ADA])
        || after["formState"]["values"]["quiet"]["value"]["selected_users"] != json!([ADA])
        || after["formState"]["values"]["required"]["value"] != json!({"type":"multi_users_select"})
        || after["formState"]["values"]["single"]["value"]["selected_user"] != ADA
        || !after["multiVisible"].is_null()
        || after["formState"]["values"]["notes"]["value"]["value"] != "Sibling"
        || after["requests"] != before_reset["requests"]
        || after["buttonSync"]["requests"] != before_reset["buttonSync"]["requests"]
    {
        return Err(
            "Reset did not reseed multi/single state or admitted the retired response".into(),
        );
    }
    for (collection, block, visible) in [
        ("multiUserWidgets", "pick", json!([ADA])),
        ("multiUserWidgets", "quiet", json!([ADA])),
        ("multiUserWidgets", "required", json!([])),
        ("userWidgets", "single", json!(ADA)),
    ] {
        let widget = |model: &Value| {
            model[collection]
                .as_array()
                .and_then(|rows| rows.iter().find(|w| w["block"] == block))
                .cloned()
        };
        let old = widget(&before_reset).ok_or("Reset prior user widget missing")?;
        let new = widget(&after).ok_or("Reset replacement user widget missing")?;
        if new["visible"] != visible || new["current"] != true || new["instance"] == old["instance"]
        {
            return Err(format!("Reset reused stale user widget {block}"));
        }
    }
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
