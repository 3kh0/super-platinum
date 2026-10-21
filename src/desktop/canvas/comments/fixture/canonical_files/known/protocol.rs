use super::*;
pub(super) fn expected(
    client: &SlackClient,
    session: &WorkspaceSession,
    id: &str,
) -> PreparedRequest {
    client.rest_form(
        session,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
            (
                "_x_reason",
                "files-store.ConditionalFetchManager.fetch".into(),
            ),
        ],
    )
}
pub(super) fn validate(
    client: &SlackClient,
    session: &WorkspaceSession,
    actual: &PreparedRequest,
    id: &str,
) -> Result<Value, Error> {
    let expected = expected(client, session, id);
    if actual.method != expected.method
        || actual.headers != expected.headers
        || actual.body != expected.body
        || !same_url(&actual.url, &expected.url)
    {
        return Err(refused());
    }
    let RequestBody::Form(fields) = &actual.body else {
        return Err(refused());
    };
    Ok(json!(
        fields
            .iter()
            .filter(|(k, _)| k != "token")
            .cloned()
            .collect::<std::collections::BTreeMap<_, _>>()
    ))
}
fn same_url(actual: &str, expected: &str) -> bool {
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
        && a.iter().zip(&e).all(|((key, value), (ek, ev))| {
            if key != ek {
                return false;
            }
            let length = match key.as_ref() {
                "_x_id" | "_x_csid" | "fp" | "_x_b3_traceid" => 32,
                "_x_b3_spanid" => 16,
                _ => return value == ev,
            };
            value.len() == length
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}
