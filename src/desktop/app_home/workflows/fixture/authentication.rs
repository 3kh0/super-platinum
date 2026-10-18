//! Closed existing-account mapping fixture; Add/OAuth endpoints are refused.
use super::*;
pub(super) struct State {
    mode: Mode,
    context: Option<Value>,
    mappings: usize,
    selected: bool,
}
impl State {
    pub(super) fn new(mode: Mode) -> Self {
        Self {
            mode,
            context: None,
            mappings: 0,
            selected: false,
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"mappings":self.mappings,"selected":self.selected})
    }
}
fn review() -> Value {
    json!({"ok":false,"error":"pretrip_ack_required","pre_trip_info":{
        "third_party_auths":{"is_shown":true,"is_satisfied":true,"auths":[{
            "app_id":"A_WORKFLOW","function_id":"Fn123","provider_key":"provider","provider_name":"Release provider",
            "valid_tokens":[{"external_token_id":"xt1","external_user_id":"First fixture account","is_default":true},
                {"external_token_id":"xt2","external_user_id":"Second fixture account","is_default":false}]}]},
        "missing_context_parameters":{"is_shown":false,"context_parameters":[]},
        "missing_customizable_input_parameters":{"is_shown":false,"customizable_input_parameters":[]},
        "scopes_acknowledgements":{"is_shown":false,"bot_scopes":[],"user_scopes":[]}}})
}
pub(super) fn begin(
    fixture: &mut Harness,
    path: &str,
    body: &Value,
    once: bool,
) -> Result<Reply, Error> {
    if !once {
        return Err(refused());
    }
    let auth = fixture.authentication.as_mut().ok_or_else(refused)?;
    if path == "/api/apps.auth.external.authMapping.update" {
        if auth.context.is_none()
            || auth.selected
            || body["app_id"] != "A_WORKFLOW"
            || body["provider_key"] != "provider"
            || body["workflow_id"] != "WWORKFLOW"
            || body["external_token_id"] != "xt2"
            || body["mapping_owner_type"] != "END_USER"
            || body.get("function_id").is_some()
        {
            return Err(refused());
        }
        auth.mappings += 1;
        match auth.mode {
            Mode::RateLimited => {
                auth.mode = Mode::Accepted;
                return Err(Error::RateLimited {
                    retry_after_secs: Some(2),
                });
            }
            Mode::Unconfirmed => {
                auth.selected = true;
                return Err(Error::Api("synthetic_mapping_response_lost".into()));
            }
            Mode::Held => {
                auth.selected = true;
                let (tx, rx) = tokio::sync::oneshot::channel();
                fixture.held = Some(tx);
                return Ok(Reply::Held(rx));
            }
            Mode::Accepted => {
                auth.selected = true;
                return ready(json!({"ok":true}));
            }
        }
    }
    if path != "/api/workflows.triggers.trip"
        || body["url"] != "https://slack.com/shortcuts/Ft12345678"
        || body["client_token"].as_str().is_none_or(str::is_empty)
    {
        return Err(refused());
    }
    let context: Value = serde_json::from_str(body["context"].as_str().ok_or_else(refused)?)
        .map_err(|_| refused())?;
    if body["run_precheck"] == "true" {
        if auth.context.is_some()
            || context["location"] != "button"
            || context["action_id"] != "run"
            || context["block_id"] != "workflow"
        {
            return Err(refused());
        }
        auth.context = Some(context);
        return ready(review());
    }
    if body.get("run_precheck").is_some()
        || !auth.selected
        || fixture.started
        || auth.context.as_ref() != Some(&context)
    {
        return Err(refused());
    }
    fixture.started = true;
    ready(json!({"ok":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_retained_token_selection_uses_mapping_without_oauth_or_new_token() {
        let (mut s, origin) = super::super::tests::setup(Mode::Accepted);
        s.app_home.workflow_fixture.as_mut().unwrap().authentication =
            Some(State::new(Mode::Accepted));
        let view = s.app_home.surface.as_ref().unwrap().view.clone().unwrap();
        let ctx = &origin.owner.ctx;
        let req = origin
            .workflow
            .precheck(&ctx.client, &ctx.session, &view, "precheck")
            .unwrap();
        let Reply::Ready(raw) = super::super::begin(&mut s, &origin, req, true).unwrap() else {
            panic!("precheck held")
        };
        let TripResult::Review(mut review) = origin.workflow.response(raw).unwrap() else {
            panic!("review required")
        };
        assert!(review.auth_ready());
        let metadata=origin.workflow.metadata(json!({"ok":true,"trigger":{"id":"Ft12345678","name":"Release","owning_team_id":ctx.session.team_id,"workflow":{"workflow_id":"WWORKFLOW","app_id":"A_WORKFLOW"}}})).unwrap();
        let request = review
            .select_connection_request(
                &ctx.client,
                &ctx.session,
                &view,
                &metadata,
                "Fn123",
                "provider",
                "xt2",
            )
            .unwrap();
        assert!(super::super::begin(&mut s, &origin, request.clone(), false).is_err());
        let Reply::Ready(raw) =
            super::super::begin(&mut s, &origin, request.clone(), true).unwrap()
        else {
            panic!("mapping held")
        };
        review
            .selected_connection("Fn123", "provider", "xt2", &raw)
            .unwrap();
        assert!(
            review.connections()[0]
                .tokens
                .iter()
                .find(|t| t.id == "xt2")
                .unwrap()
                .default
        );
        assert!(super::super::begin(&mut s, &origin, request, true).is_err());
        let start = review
            .prepare_start(&ctx.client, &ctx.session, &view, "start")
            .unwrap();
        assert!(matches!(
            super::super::begin(&mut s, &origin, start, true),
            Ok(Reply::Ready(_))
        ));
        let h = s.app_home.workflow_fixture.as_ref().unwrap();
        assert!(h.started);
        assert_eq!(h.authentication.as_ref().unwrap().mappings, 1);
        assert!(
            !h.ledger
                .iter()
                .any(|r| r["path"] == "/api/apps.auth.external.start")
        );
    }
}
