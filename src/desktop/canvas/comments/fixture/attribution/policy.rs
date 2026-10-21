use super::*;
use super_platinum_core::block_actions::name_policy::request as factory;
pub(in crate::canvas) async fn request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let (expected, raw) = {
        let shell = state.peek();
        let c = carrier(&shell)?;
        let d = data().lock().unwrap();
        if request
            .url
            .split('?')
            .next()
            .is_some_and(|v| v.ends_with("/users.prefs.get"))
        {
            (
                factory::user(&shell.core.client, &c.session),
                json!({"ok":true,"prefs":{"display_real_names_override":if d.real {1}else{0},"hidden_users":if d.hidden {EDITOR}else{""}}}),
            )
        } else {
            (
                factory::team_for(&shell.core.client, &c.session, &c.session.team_id)
                    .map_err(|_| refused())?,
                json!({"ok":true,"prefs":{"display_real_names":false}}),
            )
        }
    };
    let exact_body = matches!((&request.body,&expected.body),(RequestBody::Form(a),RequestBody::Form(b)) if a==b);
    if !same_url(&request.url, &expected.url)
        || request.method != expected.method
        || request.headers != expected.headers
        || !exact_body
    {
        return Err(refused());
    }
    let receiver = {
        let mut d = data().lock().unwrap();
        d.policy_requests.push(wire(request)?);
        d.policy_responses.push(raw.clone());
        if std::mem::take(&mut d.hold_next) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            d.held = Some((tx, raw.clone()));
            Some(rx)
        } else {
            None
        }
    };
    match receiver {
        Some(rx) => rx.await.map_err(|_| refused()),
        None => Ok(raw),
    }
}
pub(super) fn same_url(actual: &str, expected: &str) -> bool {
    let (Some((a_base, a_query)), Some((e_base, e_query))) =
        (actual.split_once('?'), expected.split_once('?'))
    else {
        return false;
    };
    if a_base != e_base || actual.contains('#') {
        return false;
    }
    let a = url::form_urlencoded::parse(a_query.as_bytes()).collect::<Vec<_>>();
    let e = url::form_urlencoded::parse(e_query.as_bytes()).collect::<Vec<_>>();
    a.len() == e.len()
        && a.iter()
            .zip(&e)
            .all(|((key, value), (expected_key, expected_value))| {
                if key != expected_key {
                    return false;
                }
                match key.as_ref() {
                    "_x_id" | "_x_csid" | "fp" => uuid::Uuid::parse_str(value).is_ok_and(|id| {
                        id.get_version_num() == 4 && id.simple().to_string() == value.as_ref()
                    }),
                    "_x_b3_traceid" | "_x_b3_spanid" => {
                        value.len() == if key == "_x_b3_traceid" { 32 } else { 16 }
                            && value
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    }
                    _ => value == expected_value,
                }
            })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> PreparedRequest {
        factory::user(
            &super_platinum_core::slack::SlackClient::default(),
            &WorkspaceSession {
                team_id: "T1".into(),
                user_id: SELF.into(),
                enterprise_id: None,
                name: "Synthetic".into(),
                url: "https://canvas-attribution-fixture.slack.com".into(),
                token: TOKEN.into(),
            },
        )
    }
    #[test]
    fn independently_generated_factory_correlations_are_admitted() {
        let a = request();
        let b = request();
        assert_ne!(a.url, b.url);
        assert!(same_url(&a.url, &b.url));
    }
    #[test]
    fn fixed_endpoint_query_and_duplicate_fields_are_not_normalized() {
        let a = request();
        assert!(!same_url(&format!("{}&target_team=TOTHER", a.url), &a.url));
        assert!(!same_url(
            &a.url.replace("users.prefs.get", "team.prefs.get"),
            &a.url
        ));
        assert!(!same_url(&format!("{}#fragment", a.url), &a.url));
    }
    #[test]
    fn malformed_correlation_is_not_a_valid_new_request() {
        let a = request();
        let mut parsed = url::Url::parse(&a.url).unwrap();
        let pairs = parsed
            .query_pairs()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    if k == "_x_id" {
                        "not-a-uuid".into()
                    } else {
                        v.to_string()
                    },
                )
            })
            .collect::<Vec<_>>();
        parsed.query_pairs_mut().clear().extend_pairs(pairs);
        assert!(!same_url(parsed.as_str(), &a.url));
    }
}
pub(in crate::canvas) fn hold(shell: &ShellState) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    data().lock().unwrap().hold_next = true;
    Ok(())
}
pub(in crate::canvas) fn release(shell: &ShellState) -> Result<Value, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let mut d = data().lock().unwrap();
    let (tx, raw) = d.held.take().ok_or_else(refused)?;
    let value = json!({"delivered":tx.send(raw).is_ok()});
    d.releases.push(value.clone());
    Ok(value)
}
pub(in crate::canvas) fn held_closed(shell: &ShellState) -> bool {
    enabled(shell)
        && data()
            .lock()
            .unwrap()
            .held
            .as_ref()
            .is_some_and(|(tx, _)| tx.is_closed())
}
