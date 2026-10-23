use super::*;
pub(super) fn person(n: usize) -> Value {
    let mut row = super::super::super::super::picker::person(
        &format!("USEARCH{n:03}"),
        &format!("Fixture Person {n:02}"),
    );
    row["is_app_user"] = json!(false);
    row["is_workflow_bot"] = json!(false);
    row["is_profile_only"] = json!(false);
    row
}
pub(super) fn reply(
    kind: wire::Kind,
    query: &str,
    marker: Option<&str>,
) -> Result<(Value, Option<Vec<String>>), Error> {
    if !matches!(
        query,
        "Dana" | "research" | "oldDana" | "rejected" | "offline"
    ) {
        return Err(closed());
    }
    if marker.is_some_and(|m| m != "search-page-2") {
        return Err(closed());
    }
    if (kind == wire::Kind::Members && query == "research")
        || (kind == wire::Kind::Channels && query != "research")
    {
        if marker.is_some() {
            return Err(closed());
        }
        return Ok((
            json!({"ok":true,"results":[],"next_marker":"","opaque_receipt":{"query":query,"method":kind.method(),"page":1}}),
            None,
        ));
    }
    if kind == wire::Kind::Channels {
        if marker.is_some() {
            return Err(closed());
        }
        let mut original = channel();
        original["is_member"] = json!(false);
        return Ok((
            json!({"ok":true,"results":{"z99":original},"member_channels":[CHANNEL],"next_marker":"","opaque_receipt":{"query":query,"method":kind.method(),"page":1}}),
            Some(vec!["z99".into()]),
        ));
    }
    let (start, end) = if marker.is_none() { (0, 13) } else { (13, 25) };
    let mut rows = Vec::new();
    for n in start..end {
        let row = match kind {
            wire::Kind::Members => {
                if n == 0 && matches!(query, "Dana" | "oldDana") {
                    dm::peer()
                } else {
                    person(100 + n)
                }
            }
            wire::Kind::Channels => {
                if n == 0 && query == "research" {
                    channel()
                } else {
                    let mut c = channel();
                    c["id"] = json!(format!("CFILLER{n:03}"));
                    c["name"] = json!(format!("unrelated-fixture-{n:02}"));
                    c
                }
            }
        };
        rows.push(row);
    }
    // Original object order deliberately differs from lexical key order. Both
    // the raw object and independent ordered keys are retained on every page.
    let mut map = serde_json::Map::new();
    let mut keys = vec![];
    for (n, row) in rows.iter().enumerate() {
        let key = format!("z{:02}", 99 - n);
        keys.push(key.clone());
        map.insert(key, row.clone());
    }
    let mut value = json!({"ok":true,"results":map,"next_marker":if marker.is_none(){"search-page-2"}else{""},"opaque_receipt":{"query":query,"method":kind.method(),"page":if marker.is_none(){1}else{2}}});
    if kind == wire::Kind::Channels {
        value["member_channels"] = json!(rows.iter().map(|r| r["id"].clone()).collect::<Vec<_>>());
    }
    Ok((value, Some(keys)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_order_and_overflow_are_independent() {
        let (a, k) = reply(wire::Kind::Members, "Dana", None).unwrap();
        assert_eq!(k.as_ref().unwrap().first().unwrap(), "z99");
        assert_eq!(a["results"]["z99"]["id"], dm::PEER);
        let (b, kb) = reply(wire::Kind::Members, "Dana", Some("search-page-2")).unwrap();
        let mut pages = wire::Pages::default();
        pages.accept(a, k.as_deref()).unwrap();
        pages.accept(b, kb.as_deref()).unwrap();
        assert!(pages.complete());
        assert_eq!(pages.rows().unwrap().len(), 24);
    }
    #[test]
    fn search_membership_companion_is_not_preapplied_to_raw() {
        let (raw, keys) = reply(wire::Kind::Channels, "research", None).unwrap();
        assert_eq!(raw["results"]["z99"]["is_member"], false);
        assert_eq!(raw["member_channels"], json!([CHANNEL]));
        assert_eq!(keys, Some(vec!["z99".into()]));
        assert_eq!(channel()["is_member"], true);
    }
    #[test]
    fn seed_does_not_invent_remote_sources() {
        assert!((0..51).all(|n| person(n)["id"] != dm::PEER));
        assert_ne!(channel()["id"], raw::DEST);
    }
}
