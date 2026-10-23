use super::*;

pub(super) fn run(
    plan: &Plan,
    matched: &[Matched<'_>],
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    collate: impl Fn(&str, &str) -> Result<std::cmp::Ordering, Error>,
) -> Result<Observation, Error> {
    let state = context.state;
    if !context.now_ms.is_finite() {
        return Err(Error::InvalidClock);
    }
    if state["selfId"].as_str().is_none_or(str::is_empty)
        || !state["metadata"].is_object()
        || !state["navigationHistory"].is_object()
        || !state["experiments"].is_object()
        || ["enterprise", "teamId", "offline"]
            .iter()
            .any(|key| state.get(*key).is_none())
    {
        return Err(Error::MissingObservation(
            "Save channel selector state".into(),
        ));
    }
    let mut state = state.clone();
    let offline = truth(&state["teamId"]) && truth(&state["offline"]);
    if offline
        && (!state["offline"]["channel"].is_object()
            || state["entityCtr"]["channel"]
                .as_f64()
                .is_none_or(|v| !v.is_finite())
            || !state["channelPriority"].is_object())
    {
        return Err(Error::MissingObservation(
            "Save channel offline observations".into(),
        ));
    }
    let mut rows = Vec::new();
    for matched in matched {
        if matched.query != plan.lookup.query {
            return Err(Error::InvalidInput);
        }
        let scored = &matched.scored;
        let id = scored.id;
        let md = &state["metadata"][id];
        if !md.is_object()
            || [
                "latest", "lastRead", "draft", "starred", "unread", "priority",
            ]
            .iter()
            .any(|key| md.get(*key).is_none())
        {
            return Err(Error::MissingObservation(format!("channel metadata {id}")));
        }
        if offline && state["offline"]["channel"].get(id).is_none() {
            // Ejs0's positively observed missing-record fallback, not l24D's
            // different Offline::default_features team CTR.
            state["offline"]["channel"][id] = json!({
                "user_ctr_30dd":state["entityCtr"]["channel"],"team_ctr_90dd":0});
        }
        let item = directory_selectors::item(
            scored.item,
            directory_selectors::Names {
                comparison: &matched.comparison,
                mpim: None,
                custom: None,
                custom_allowed: false,
            },
        )
        .ok_or(Error::InvalidInput)?;
        let mut row = json!({"id":id,"item":item,"score":scored.score});
        if let Some(exact) = scored.is_exact_match {
            row["isExactMatch"] = json!(exact);
        }
        rows.push(row);
    }
    let options = ranking::Options {
        limit: 25,
        source: "texty-autocomplete".into(),
        ..Default::default()
    };
    let shared = Context {
        state: &state,
        history: context.history,
        model: context.model,
        now_ms: context.now_ms,
        normalize: context.normalize,
    };
    let failure = std::cell::RefCell::new(None);
    let ranked = ranking::rank_observed(
        &plan.formatted,
        &json!({"channels":rows}),
        &options,
        &shared,
        cache,
        |a, b| {
            collate(a, b).unwrap_or_else(|error| {
                *failure.borrow_mut() = Some(error);
                std::cmp::Ordering::Equal
            })
        },
    )?;
    if let Some(error) = failure.into_inner() {
        return Err(error);
    }
    let mut exact = Vec::new();
    let mut ranked_ids = Vec::new();
    for row in &ranked.rows {
        let original = matched
            .get(row.original_index)
            .ok_or(Error::InvalidInput)?
            .raw();
        ranked_ids.push(
            original["id"]
                .as_str()
                .ok_or(Error::InvalidInput)?
                .to_owned(),
        );
        // rNCs filters AFTER ranking/capping. Never return the ranker's copy
        // containing synthetic facts in place of the original canonical row.
        if original["name"].as_str() == Some(plan.lookup.query.as_str()) {
            exact.push(original.clone());
        }
    }
    Ok(Observation {
        search: MentionSearch {
            lookup: plan.lookup.clone(),
            rows: exact,
        },
        latest_ids: ranked.channel_latest_ids,
        ranked_ids,
    })
}
