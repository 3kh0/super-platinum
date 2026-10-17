use super::super::*;
fn case(mode: FieldCreateMode) -> &'static str {
    match mode {
        FieldCreateMode::RatingDefault => "default",
        FieldCreateMode::RatingStandard => "standard",
        FieldCreateMode::RatingCustom => "custom",
        _ => unreachable!(),
    }
}
fn name(mode: FieldCreateMode) -> &'static str {
    if mode == FieldCreateMode::RatingCustom {
        "fixture_quality"
    } else {
        "heart"
    }
}
async fn quantity(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    n: u8,
) -> Result<Value, String> {
    let before = draft(state)?;
    if before["kind"] != "rating" || before["ratingPicker"] != Value::Null {
        return Err("Rating quantity source is not editable".into());
    }
    let event = checked(d, &format!("window.__listRating.quantity({n})")).await?;
    for _ in 0..150 {
        let after = draft(state)?;
        let changed = before["ratingOptions"]["max"] != n;
        if after["ratingOptions"]["max"] == n
            && after["serial"] == before["serial"]
            && after["visit"] == before["visit"]
            && after["selection"] == before["selection"]
            && after["name"] == before["name"]
            && (after["choice"] != before["choice"]) == changed
        {
            let dom = wait_dom(
                d,
                &format!("window.__listRating.quantityReady({n},{})", after["choice"]),
            )
            .await?;
            if draft(state)? != after {
                return Err("Rating quantity changed during DOM proof".into());
            }
            let mut expected = before["ratingOptions"].clone();
            expected["max"] = json!(n);
            if after["ratingOptions"] != expected {
                return Err("Quantity changed emoji/team".into());
            }
            return Ok(json!({"before":before,"event":event,"after":after,"dom":dom}));
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Actual Rating quantity callback did not settle".into())
}
async fn open_picker(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let before = draft(state)?;
    let click = checked(d, "window.__listRating.open()").await?;
    let ready = wait_dom(d, "window.__listRating.pickerReady()").await?;
    let current = draft(state)?;
    if current["ratingPicker"]
        .as_u64()
        .map(|n| n.to_string())
        .as_deref()
        != ready["token"].as_str()
        || current["ratingOptions"] != before["ratingOptions"]
        || current["choice"] != before["choice"]
        || current["serial"] != before["serial"]
    {
        return Err("Emoji picker opened under wrong Rating custody".into());
    }
    let search = checked(
        d,
        &format!("window.__listRating.search({})", json!(name(mode))),
    )
    .await?;
    let result = wait_dom(
        d,
        &format!("window.__listRating.result({})", json!(name(mode))),
    )
    .await?;
    if draft(state)? != current {
        return Err("Picker changed original Rating draft".into());
    }
    Ok(
        json!({"before":before,"click":click,"ready":ready,"search":search,"result":result,"draft":current}),
    )
}
async fn choose(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let before = draft(state)?;
    let event = checked(
        d,
        &format!("window.__listRating.choose({})", json!(name(mode))),
    )
    .await?;
    let returned = wait_dom(d, "window.__listRating.returned()").await?;
    let after = draft(state)?;
    if after["ratingPicker"] != Value::Null
        || after["ratingOptions"] != mode.rating_options()
        || after["choice"] == before["choice"]
        || after["serial"] != before["serial"]
        || after["visit"] != before["visit"]
        || after["selection"] != before["selection"]
        || after["name"] != before["name"]
    {
        return Err("Selected emoji did not update exact current Rating options".into());
    }
    Ok(json!({"event":event,"returned":returned,"before":before,"after":after}))
}
async fn stage(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    proof: &mut Proof,
    name: &str,
    dom: Value,
) -> Result<(), String> {
    if proof.stages.len() >= 2 {
        return Err("Extra Rating prelude capture".into());
    }
    let before = draft(state)?;
    let ledger = rename_fixture::snapshot(&state.peek())?;
    if ledger["requests"] != json!([])
        || ledger["responses"] != json!([])
        || ledger["finished"] != json!([])
    {
        return Err("Rating configuration issued a request".into());
    }
    proof.stable(state, 0)?;
    let capture = harness::capture(d, name).await?;
    proof.stable(state, 0)?;
    if draft(state)? != before || rename_fixture::snapshot(&state.peek())? != ledger {
        return Err("Rating prelude changed during capture".into());
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
    let mode = proof.mode;
    checked(
        d,
        &format!("window.__listRating.init({})", json!(case(mode))),
    )
    .await?;
    let initial = draft(state)?;
    if initial["ratingOptions"] != json!({"emoji":":star:","max":5}) {
        return Err("Rating did not begin at default star5".into());
    }
    let mut quantities = vec![];
    for n in 1..=5 {
        let receipt = quantity(state, d, n).await?;
        if mode == FieldCreateMode::RatingDefault && (n == 1 || n == 5) {
            stage(
                state,
                d,
                proof,
                if n == 1 {
                    "quantity-one"
                } else {
                    "quantity-five"
                },
                receipt["dom"].clone(),
            )
            .await?;
        }
        quantities.push(receipt);
    }
    let final_quantity = quantity(state, d, mode.rating_max()).await?;
    let picker = if mode != FieldCreateMode::RatingDefault {
        let first = open_picker(state, d, mode).await?;
        let dismiss = checked(
            d,
            &format!(
                "window.__listRating.dismiss({})",
                json!(if mode == FieldCreateMode::RatingCustom {
                    "escape"
                } else {
                    "close"
                })
            ),
        )
        .await?;
        let restored = wait_dom(d, "window.__listRating.returned()").await?;
        if draft(state)?["ratingOptions"] != first["before"]["ratingOptions"] {
            return Err("Picker dismissal changed Rating".into());
        }
        let second = open_picker(state, d, mode).await?;
        if first["draft"]["ratingPicker"] == second["draft"]["ratingPicker"] {
            return Err("Reopened picker reused token".into());
        }
        stage(
            state,
            d,
            proof,
            if mode == FieldCreateMode::RatingCustom {
                "custom-picker"
            } else {
                "standard-picker"
            },
            second["result"].clone(),
        )
        .await?;
        let chosen = choose(state, d, mode).await?;
        let verified = verify(state, d, mode).await?;
        stage(
            state,
            d,
            proof,
            if mode == FieldCreateMode::RatingCustom {
                "custom-config"
            } else {
                "standard-config"
            },
            verified,
        )
        .await?;
        json!({"first":first,"dismiss":dismiss,"restored":restored,"second":second,"chosen":chosen})
    } else {
        Value::Null
    };
    Ok(
        json!({"case":case(mode),"quantities":quantities,"finalQuantity":final_quantity,"picker":picker,"after":draft(state)?}),
    )
}
pub(crate) async fn configure(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let before = draft(state)?;
    if before["ratingOptions"] != json!({"emoji":":star:","max":5}) {
        return Err("Independent Rating reopen did not reset defaults".into());
    }
    let quantity = quantity(state, d, mode.rating_max()).await?;
    let picker = if mode == FieldCreateMode::RatingDefault {
        Value::Null
    } else {
        let opened = open_picker(state, d, mode).await?;
        let chosen = choose(state, d, mode).await?;
        json!({"opened":opened,"chosen":chosen})
    };
    let verified = verify(state, d, mode).await?;
    let focus = checked(d, "window.__listRating.focusName()").await?;
    Ok(
        json!({"before":before,"quantity":quantity,"picker":picker,"verified":verified,"focus":focus,"after":draft(state)?}),
    )
}
pub(crate) async fn verify(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let before = draft(state)?;
    let expected = mode.rating_options();
    let editing = rename::presentation(&state.peek()).is_some_and(|p| p.stage == Stage::Editing);
    let dom = wait_dom(
        d,
        &format!(
            "window.__listRating.inspect({},{},{editing})",
            expected["max"], expected["emoji"]
        ),
    )
    .await?;
    if before["ratingOptions"] != expected
        || before["ratingPicker"] != Value::Null
        || dom["choice"].as_str() != before["choice"].as_u64().map(|n| n.to_string()).as_deref()
        || draft(state)? != before
    {
        return Err("Rating DOM/model options custody differs".into());
    }
    Ok(dom)
}
