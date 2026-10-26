use super::super::check::helpers as h;
use super::*;
async fn held(mut s: Signal<ShellState>, d: &DesktopContext, text: &str) -> Result<Value, String> {
    h::edit(s, d, "enter", text).await?;
    let index = arm(&mut s.write());
    let press = h::enter(s, d, "enter", json!({})).await?;
    let queued = snapshot(&s.peek())["queue"].clone();
    if queued.as_array().is_none_or(|v| v.len() != 1) {
        return Err("Admission occurrence was not captured before its debounce".into());
    }
    let held = waiting(s).await?;
    Ok(json!({"attempt":index,"press":press,"queued":queued,"held":held}))
}
fn refused(model: &Value, index: usize) -> Result<(), String> {
    if model["admission"]["attempts"][index]["accepted"] != false {
        return Err("Original calendar attempt was not refused".into());
    }
    Ok(())
}
pub(crate) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut h::Captures,
) -> Result<(), String> {
    let start = held(s, d, "calendar race").await?;
    let index = start["attempt"].as_u64().unwrap() as usize;
    let observation = gestures::calendar(s, "2026-10-26", true).await?;
    release(&mut s.write())?;
    h::counts(s, 1, 1).await?;
    let silence = h::quiet(s, 1).await?;
    h::settle(s, d).await?;
    let after = snapshot(&s.peek());
    refused(&after, index)?;
    let attempt = &after["admission"]["attempts"][index];
    if attempt["held"] != true
        || attempt["sourceCurrent"] != true
        || attempt["timedOut"] != false
        || attempt["pausedMs"].as_u64().is_none_or(|v| v >= 2000)
        || !after["admission"]["busy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["attempt"] == json!(index) && b["busy"] == false)
        || after["requests"][0]["action"]["value"] != "calendar race"
        || after["formState"]["values"]["day"]["value"]["selected_date"] != "2026-10-25"
        || after["nativeMounts"][0]["value"] != json!(["2026-10-26", "2026-10-25"]).to_string()
    {
        return Err(
            "Trusted calendar progress did not preserve original text/parent and current draft"
                .into(),
        );
    }
    h::shot(
        s,
        d,
        n,
        "trusted-progress",
        json!({"start":start,"observation":observation,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let start = held(s, d, "unseen").await?;
    let index = start["attempt"].as_u64().unwrap() as usize;
    let unseen = gestures::calendar(s, "2026-10-27", false).await?;
    release(&mut s.write())?;
    let silence = h::quiet(s, 1).await?;
    let refused_model = snapshot(&s.peek());
    refused(&refused_model, index)?;
    let restore = gestures::calendar(s, "2026-10-26", false).await?;
    let no_replay = h::quiet(s, 1).await?;
    h::shot(s,d,n,"unseen-refused",json!({"start":start,"unseen":unseen,"refused":refused_model,"silence":silence,"restore":restore,"noReplay":no_replay}),st,cap).await?;

    let start = held(s, d, "old source").await?;
    let index = start["attempt"].as_u64().unwrap() as usize;
    let changed = h::refresh(&mut s.write());
    let dom_retired = gestures::retired(&start).await?;
    release(&mut s.write())?;
    let silence = h::quiet(s, 1).await?;
    h::settle(s, d).await?;
    let after = snapshot(&s.peek());
    refused(&after, index)?;
    if after["admission"]["attempts"][index]["sourceCurrent"] != false {
        return Err("Refreshed source did not retire captured attempt".into());
    }
    h::shot(
        s,
        d,
        n,
        "source-retired",
        json!({"start":start,"refresh":changed,"domRetired":dom_retired,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;
    h::edit(s, d, "enter", "fresh source").await?;
    let press = h::enter(s, d, "enter", json!({})).await?;
    h::counts(s, 2, 2).await?;
    h::shot(
        s,
        d,
        n,
        "fresh-after-source",
        json!({"press":press}),
        st,
        cap,
    )
    .await?;

    let start = held(s, d, "old owner").await?;
    let index = start["attempt"].as_u64().unwrap() as usize;
    s.write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .rt_generation += 1;
    let dom_retired = gestures::retired(&start).await?;
    release(&mut s.write())?;
    let silence = h::quiet(s, 2).await?;
    h::settle(s, d).await?;
    let after = snapshot(&s.peek());
    refused(&after, index)?;
    if after["admission"]["attempts"][index]["sourceCurrent"] != false {
        return Err("Socket owner did not retire captured attempt".into());
    }
    h::shot(
        s,
        d,
        n,
        "owner-retired",
        json!({"start":start,"domRetired":dom_retired,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;
    {
        let mut w = s.write();
        let owner = Owner::capture(&w).ok_or("Fixture Home rebind unavailable")?;
        let home = w.app_home.surface.as_mut().unwrap();
        home.id = owner.home_visit;
        home.owner = owner;
        home.revision = next();
        inputs::hydrate(&mut w);
    }
    h::settle(s, d).await?;
    h::edit(s, d, "enter", "fresh owner").await?;
    let press = h::enter(s, d, "enter", json!({})).await?;
    h::counts(s, 3, 3).await?;
    let silence = h::quiet(s, 3).await?;
    h::shot(
        s,
        d,
        n,
        "fresh-after-owner",
        json!({"fixtureOwnerRebindAfterSocket":true,"press":press,"silence":silence}),
        st,
        cap,
    )
    .await
}
