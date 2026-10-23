use super::*;
use super_platinum_core::block_actions::{
    home::workflow::directory_observations as observed, name_policy, shortcuts::grid,
};

fn source_self(s: &ShellState) -> Result<Value, Error> {
    let model = crate::app_home::about::management::fixture::first_publish::capture(s)
        .map_err(|_| closed())?;
    let m = model.0.lock().unwrap();
    let raw = m.seed.as_ref().ok_or_else(closed)?["boot"]["self"].clone();
    if raw["id"] != "UWORKFLOW" || raw["team_id"] != "TWORKFLOW" {
        return Err(closed());
    }
    Ok(raw)
}
pub(super) fn reply(method: &str, self_raw: &Value, scenario: &str) -> Result<Value, Error> {
    Ok(match method {
        "client.gantryBoot" => json!({"ok":true,"self":self_raw,"team":{"id":"TWORKFLOW"},
            "workspaces":[{"id":"TWORKFLOW","allow_external_users":false,"prefs":{"display_real_names":true}}],
            "opaque_receipt":"original-ranking-grid"}),
        // These are complete responses. Missing frecency is an observed empty
        // history in the actual decoder, not a fixture-set ranking feature.
        "users.prefs.get" => {
            json!({"ok":true,"prefs":{"display_real_names_override":1},"opaque_receipt":"original-ranking-user-prefs"})
        }
        "team.prefs.get" => {
            json!({"ok":true,"prefs":{"display_real_names":true},"opaque_receipt":"original-ranking-team-prefs"})
        }
        "experiments.getByUser" => json!({"ok":true,"assignments":{
            "autocomplete_affinity_score":{"group":"off"},
            "autocomplete_affinity_ml_model":{"group":"off"},
            "autocomplete_flannel_fuzzy_match_passthrough":{"group":"control"}},
            "opaque_receipt":"original-ranking-experiments"}),
        "search.autocomplete.model" => {
            let model = if scenario == "cached-write" {
                // Intentionally opposite READ/WRITE direction. A subsequent
                // query must use this actual cached reply, not inject weights.
                json!({"CHANNEL_IS_MEMBER":0,"WRITE_CHANNEL_IS_MEMBER":8,
                    "READ_CHANNEL_IS_MEMBER":-8,"USER_IS_ACTIVE":1})
            } else {
                Value::Null
            };
            json!({"ok":true,"model":model,"opaque_receipt":"original-ranking-model"})
        }
        "search.autocomplete.offlineFeatures" => json!({"ok":true,
            "top_channels":{},"top_users":{},"users_has_dmed":{},"users_has_mpdmed":{},
            "opaque_receipt":"original-ranking-offline"}),
        _ => return Err(closed()),
    })
}

/// This closed response has no nonempty priority map. Absence is not the same
/// observation as Object.keys({}); never invent order for a future nonempty map.
pub(super) fn offline_keys(raw: &Value) -> Result<Option<Vec<String>>, Error> {
    match raw.get("channel_priority_90d") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(values)) if values.is_empty() => Ok(Some(Vec::new())),
        _ => Err(closed()),
    }
}

async fn route(
    state: Signal<ShellState>,
    request: &PreparedRequest,
    cache: bool,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    if !super::super::super::requested() {
        return None;
    }
    let (method, fields) = crate::app_home::about::management::fixture::decode(request).ok()?;
    let supported = if cache {
        matches!(
            method,
            "search.autocomplete.model" | "search.autocomplete.offlineFeatures"
        )
    } else {
        matches!(
            method,
            "client.gantryBoot" | "users.prefs.get" | "team.prefs.get" | "experiments.getByUser"
        )
    };
    if !supported {
        return None;
    }
    Some((|| {
        let s = state.peek();
        if !enabled(&s) || (!cache && !active(&s)) {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let expected = match method {
            "client.gantryBoot" => grid::discovery(&client, &session),
            "users.prefs.get" => name_policy::request::user(&client, &session),
            "team.prefs.get" => {
                let target = fields
                    .iter()
                    .find(|(k, _)| k == "target_team")
                    .map(|(_, v)| v.as_str())
                    .ok_or_else(closed)?;
                if target != session.team_id {
                    return Err(closed());
                }
                name_policy::request::team_for(&client, &session, target).map_err(|_| closed())?
            }
            "experiments.getByUser" => api::list_experiments(&client, &session),
            "search.autocomplete.model" => observed::model_request(&client, &session),
            "search.autocomplete.offlineFeatures" => observed::offline_request(&client, &session),
            _ => return Err(closed()),
        };
        super::super::super::super::super::picker::same_request(request, &expected)?;
        let raw = reply(&method, &source_self(&s)?, &case())?;
        let empty_case = super::super::empty::requested().then(super::super::empty::case);
        let raw = super::super::empty::observation(empty_case.as_deref(), method, raw);
        let keys = if method == "search.autocomplete.offlineFeatures" {
            offline_keys(&raw)?
        } else {
            None
        };
        let offline = method == "users.prefs.get"
            && ((requested() && case() == "read-failure")
                || empty_case.as_deref() == Some("cold-failure"));
        let mut m = model.0.lock().unwrap();
        let d = data_mut(&mut m).ok_or_else(closed)?;
        if d.requests.len() >= 256 {
            return Err(closed());
        }
        let serial = d.requests.len() + 1;
        d.requests.push(json!({"serial":serial,"kind":if cache{"cache"}else{"query"},
            "method":method,"body":safe_body(&request.body),"endpoint":request.url.split('?').next(),
            "retrySafe":request.retry_safe(),"owner":owner(&s,&session)}));
        d.responses.push(if offline {
            json!({"serial":serial,"error":"offline"})
        } else {
            json!({"serial":serial,"raw":raw,"orderedKeys":keys})
        });
        d.settlements.push(json!({"serial":serial,"returned":true}));
        if offline {
            Err(Error::Offline(
                "Synthetic ranking preferences unavailable".into(),
            ))
        } else {
            Ok((raw, keys))
        }
    })())
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    route(state, request, false).await
}
pub(crate) async fn cache_read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    route(state, request, true).await
}

/// Fresh posting read for the original source-oracle winner. Search result
/// membership is never itself a grant to send.
pub(crate) async fn ordinary(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let (method, fields) = crate::app_home::about::management::fixture::decode(request).ok()?;
    let target = fields.iter().find(|(k, _)| k == "channel")?.1.as_str();
    let ordinal = target.strip_prefix("CRANKR")?.parse::<usize>().ok()?;
    if method != "conversations.info" || ordinal >= 24 || target != format!("CRANKR{ordinal:03}") {
        return None;
    }
    Some((|| {
        let s = state.peek();
        if !enabled(&s) || !active(&s) {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let expected = api::conversations_info(&client, &session, target.into());
        super::super::super::super::super::picker::same_request(request, &expected)?;
        let canonical = crate::app_home::channel_fetches::canonical::observed(&s, "TWORKFLOW")?;
        let current = canonical.current(&s);
        let row = canonical.row(target).cloned().ok_or_else(closed)?;
        if !current || row["is_member"] != true {
            return Err(closed());
        }
        let raw = json!({"ok":true,"channel":rows::posting_channel(ordinal),"opaque_receipt":"original-ranking-destination-posting"});
        let mut m = model.0.lock().unwrap();
        if m.reads.len() >= 128 {
            return Err(closed());
        }
        let serial = m.reads.len() + 1;
        m.search
            .as_mut()
            .ok_or_else(closed)?
            .posting_sources
            .push(json!({"readSerial":serial,"canonical":row,"current":current}));
        m.reads.push(
            json!({"serial":serial,"method":method,"body":safe_body(&request.body),
            "endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),
            "owner":owner(&s,&session),"result":{"raw":raw}}),
        );
        Ok(raw)
    })())
}
