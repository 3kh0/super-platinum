use super::*;
pub(super) fn row(
    serial: usize,
    phase: &str,
    method: &str,
    fields: &[(String, String)],
    retry_safe: bool,
    owner: Value,
    applied: bool,
) -> Value {
    let safe = fields
        .iter()
        .filter(|(key, _)| key != "token")
        .collect::<Vec<_>>();
    json!({"serial":serial,"phase":phase,"method":method,"fields":safe,
        "args":safe.iter().map(|(k,v)|(k.clone(),json!(v))).collect::<serde_json::Map<_,_>>(),
        "retrySafe":retry_safe,"owner":owner,"applied":applied})
}
pub(super) fn validate_mutation(
    s: &ShellState,
    request: &PreparedRequest,
    method: &str,
    _fields: &[(String, String)],
) -> Result<(), Error> {
    let h = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .ok_or_else(closed)?;
    let d = document(s, h.id).ok_or_else(closed)?;
    let expected = match method {
        "functions.workflows.permissions.copy.set" if raw::combined() => MutationPlan::new(
            &d.editor,
            protocol::Command::CopyPermission("everyone".into()),
        )?
        .slots()[0]
            .plan()
            .request(&h.owner.ctx.client, &h.owner.ctx.session),
        "workflows.triggers.permissions.set" => MutationPlan::new(&d.editor, raw::command())?
            .slots()[0]
            .plan()
            .request(&h.owner.ctx.client, &h.owner.ctx.session),
        "functions.workflows.getIsRestricted" => {
            let mut plan = MutationPlan::new(&d.editor, raw::command())?;
            plan.mark_sent(0)?;
            plan.settle(0, Outcome::Succeeded(raw::run_ack()))?;
            let follow = protocol::FollowUpPlan::new(&plan)?;
            follow.slots()[0]
                .plan()
                .request(&h.owner.ctx.client, &h.owner.ctx.session)
        }
        _ => return Err(closed()),
    };
    same_request(request, &expected)
}
pub(super) fn same_request(
    request: &PreparedRequest,
    expected: &PreparedRequest,
) -> Result<(), Error> {
    if request.retry_safe() != expected.retry_safe()
        || request.method != expected.method
        || request.body != expected.body
        || request.headers != expected.headers
        || normalized(&request.url)
            .zip(normalized(&expected.url))
            .is_none_or(|(a, b)| a != b)
    {
        return Err(closed());
    }
    Ok(())
}
fn normalized(value: &str) -> Option<(String, std::collections::BTreeMap<String, String>)> {
    let parsed = url::Url::parse(value).ok()?;
    if parsed.fragment().is_some() {
        return None;
    }
    let mut fields = std::collections::BTreeMap::new();
    for (key, value) in parsed.query_pairs() {
        let value = if matches!(key.as_ref(), "_x_id" | "_x_csid" | "fp") {
            if value.len() != 32
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || uuid::Uuid::parse_str(&value).ok()?.get_version_num() != 4
            {
                return None;
            }
            "trace".to_owned()
        } else if matches!(key.as_ref(), "_x_b3_spanid" | "_x_b3_traceid") {
            let len = if key == "_x_b3_spanid" { 16 } else { 32 };
            if value.len() != len
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return None;
            }
            "trace".to_owned()
        } else {
            value.into_owned()
        };
        if fields.insert(key.into_owned(), value).is_some() {
            return None;
        }
    }
    Some((value.split('?').next()?.into(), fields))
}
