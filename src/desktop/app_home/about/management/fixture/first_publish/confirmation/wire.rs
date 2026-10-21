use super::*;
fn field<'a>(fields: &'a [(String, String)], key: &str) -> Result<&'a str, Error> {
    let values = fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .collect::<Vec<_>>();
    if values.len() == 1 {
        Ok(values[0])
    } else {
        Err(closed())
    }
}
fn admitted(s: &ShellState) -> Result<(), Error> {
    let h = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .ok_or_else(closed)?;
    let p = crate::app_home::about::management::permissions::finish_up::snapshot(s, h.id);
    let a = p["attempts"]
        .as_array()
        .and_then(|v| v.last())
        .ok_or_else(closed)?;
    if a["finished"] != true
        || a["complete"] != true
        || a["closed"] != false
        || !a["confirmationAdd"].is_object()
    {
        return Err(closed());
    }
    Ok(())
}
async fn execute(
    state: Signal<ShellState>,
    request: &PreparedRequest,
    mutation: bool,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        admitted(&s)?;
        let parent = super::super::model(&s)?;
        let m = parent.0.lock().unwrap();
        let model = m.confirmation_add.clone().ok_or_else(closed)?;
        let (method, fields) = crate::app_home::about::management::fixture::decode(request)?;
        let ctx = &s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .ok_or_else(closed)?
            .owner
            .ctx;
        let expected = match (mutation, method) {
            (false, "client.userBoot") => api::user_boot(&ctx.client, &ctx.session),
            (false, "conversations.info") => {
                let id = field(fields, "channel")?;
                if raw::channel(id).is_none() {
                    return Err(closed());
                }
                api::conversations_info(&ctx.client, &ctx.session, id.into())
            }
            (false, "bookmarks.list") => {
                let id = field(fields, "channel")?;
                if !matches!(id, raw::DEST | raw::PRIVATE) {
                    return Err(closed());
                }
                ctx.client.rest_form(
                    &ctx.session,
                    "bookmarks.list",
                    vec![
                        ("channel", id.into()),
                        ("include_folders", "true".into()),
                        ("include_legacy_workflows", "true".into()),
                        ("_x_reason", "bookmarks-list-api".into()),
                    ],
                )
            }
            (true, "bookmarks.add") => {
                if field(fields, "channel")? != raw::DEST {
                    return Err(closed());
                }
                ctx.client.rest_form(
                    &ctx.session,
                    "bookmarks.add",
                    vec![
                        ("channel", raw::DEST.into()),
                        (
                            "link",
                            m.trigger["shortcut_url"]
                                .as_str()
                                .ok_or_else(closed)?
                                .into(),
                        ),
                        ("shortcut_id", "TR_REVIEW".into()),
                        (
                            "title",
                            m.trigger["name"].as_str().unwrap_or("TR_REVIEW").into(),
                        ),
                        ("type", "shortcut".into()),
                        ("_x_reason", "channel-tab-add-bookmark".into()),
                    ],
                )
            }
            _ => return Err(closed()),
        };
        super::super::endpoint::same(request, &expected)?;
        let mut add = model.0.lock().unwrap();
        if mutation && add.requests.iter().any(|r| r["phase"] == "mutation") {
            return Err(closed());
        }
        let result = match method {
            "client.userBoot" => Ok(m
                .seed
                .as_ref()
                .and_then(|v| v.get("boot"))
                .cloned()
                .ok_or_else(closed)?),
            "conversations.info" => {
                let id = field(fields, "channel")?;
                let previous = add
                    .requests
                    .iter()
                    .filter(|r| {
                        r["method"] == "conversations.info"
                            && r["fields"].as_array().is_some_and(|pairs| {
                                pairs.iter().any(|p| p[0] == "channel" && p[1] == id)
                            })
                    })
                    .count();
                // Listing acquires the original raw row first. Only the later
                // original selection validation is offline in this case.
                if case() == "first-add-unavailable" && id == raw::DEST && previous > 0 {
                    Err(Error::Offline("synthetic Add channel unavailable".into()))
                } else {
                    Ok(
                        json!({"ok":true,"channel":raw::channel(field(fields,"channel")?).ok_or_else(closed)?,"opaque_receipt":"original-add-channel"}),
                    )
                }
            }
            "bookmarks.list" => {
                let rows =
                    if case() == "first-add-duplicate" && field(fields, "channel")? == raw::DEST {
                        vec![raw::bookmark(&m.trigger)]
                    } else {
                        add.bookmarks.clone()
                    };
                Ok(json!({"ok":true,"bookmarks":rows,"opaque_receipt":"original-shortcut-list"}))
            }
            "bookmarks.add" => {
                if case() == "first-add-rejected" {
                    Ok(raw::rejection())
                } else {
                    let bookmark = raw::bookmark(&m.trigger);
                    add.bookmarks.push(bookmark.clone());
                    Ok(
                        json!({"ok":true,"bookmark":bookmark,"opaque_receipt":"original-shortcut-added"}),
                    )
                }
            }
            _ => return Err(closed()),
        };
        let n = add.requests.len() + 1;
        add.requests.push(json!({"serial":n,"phase":if mutation{"mutation"}else{"read"},"method":method,
            "fields":fields.iter().filter(|(k,_)|!matches!(k.as_str(),"token"|"enterprise_token")).collect::<Vec<_>>(),
            "retrySafe":request.retry_safe(),"applied":mutation&&case()!="first-add-rejected",
            "owner":{"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team}}));
        drop(add);
        drop(m);
        Ok((
            model,
            n,
            mutation && case() == "first-add-retirement",
            result,
        ))
    })();
    Some(match prepared {
        Ok((model, n, held, result)) => deliver(model, n, held, result).await,
        Err(e) => Err(e),
    })
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    execute(state, request, false).await
}
pub(crate) async fn mutation(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    execute(state, request, true).await
}
