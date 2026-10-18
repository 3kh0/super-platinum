use super::*;

pub(super) async fn observe(fixture: &State) -> Result<Vec<Value>, String> {
    let mut events: Vec<Value> = vec![];
    for ordinal in 0..2 {
        // Read identity from the real mounted visit, not a generated stand-in.
        let mut read = dioxus::document::eval(
            "const r=document.getElementById('forward-dialog');dioxus.send({owner:r?.dataset.scheduledForwardOwner,generation:r?.dataset.editorGeneration});",
        );
        let identity = tokio::time::timeout(Duration::from_secs(2), read.recv::<Value>())
            .await
            .map_err(|_| "Keyboard owner timeout")?
            .map_err(|_| "Keyboard owner missing")?;
        if identity["owner"].as_str().is_none_or(str::is_empty)
            || identity["generation"].as_str().is_none_or(str::is_empty)
        {
            return Err("Invalid mounted keyboard owner".into());
        }
        if let Some(first) = events.first() {
            if first.get("owner") != identity.get("owner")
                || first.get("generation") != identity.get("generation")
            {
                return Err("Keyboard visit changed".into());
            }
        }
        let config = json!({"ordinal":ordinal,"owner":identity["owner"],"generation":identity["generation"]});
        let script = format!(
            "{};try{{await scheduledForwardKeyboard({config},dioxus);}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}",
            include_str!("keyboard.js")
        );
        let mut eval = dioxus::document::eval(&script);
        let ready = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>())
            .await
            .map_err(|_| "Keyboard readiness timeout")?
            .map_err(|_| "Keyboard readiness failed")?;
        if ready["kind"] != "ready"
            || ready["ordinal"] != ordinal
            || ready["owner"] != identity["owner"]
            || ready["generation"] != identity["generation"]
        {
            return Err(format!("Invalid keyboard readiness: {ready}"));
        }
        eprintln!(
            "scheduled-forward-keyboard: {}",
            json!({"version":1,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"scenario":fixture.scenario,"phase":fixture.phase,"nonce":fixture.nonce,"pid":std::process::id(),"request":ready})
        );
        let result = tokio::time::timeout(Duration::from_secs(9), eval.recv::<Value>())
            .await
            .map_err(|_| "Trusted keyboard timeout")?
            .map_err(|_| "Trusted keyboard bridge failed")?;
        if result["ok"] != true
            || result["ordinal"] != ordinal
            || result["cleaned"] != true
            || result["trusted"] != true
            || result["owner"] != identity["owner"]
            || result["generation"] != identity["generation"]
        {
            return Err(format!("Trusted keyboard proof refused: {result}"));
        }
        events.push(result);
    }
    Ok(events)
}
