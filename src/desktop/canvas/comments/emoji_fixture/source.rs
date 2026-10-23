use super::*;
pub(in crate::canvas::comments) async fn assignments(
    state: Signal<ShellState>,
) -> Result<Value, String> {
    if !context_enabled(&state.peek()) {
        return Err("Original fixture context unavailable".into());
    }
    let witness=channel_fetches::fixture_emoji_assignments(state,TEAM,move|request|async move{
        let shell=state.peek();if !context_enabled(&shell){return Err(Error::Offline("Original fixture owner retired".into()));}
        let original=api::list_experiments(&shell.core.client,&shell.core.session.as_ref().unwrap().workspaces[TEAM]);
        if request.method!=original.method||request.headers!=original.headers||request_url(&request.url).zip(request_url(&original.url)).is_none_or(|(a,b)|a!=b){return Err(Error::Offline("Assignment factory changed".into()));}
        let (RequestBody::Form(actual),RequestBody::Form(expected))=(&request.body,&original.body)else{return Err(Error::Offline("Original assignment form absent".into()));};
        if actual!=expected{return Err(Error::Offline("Original assignment fields changed".into()));}
        let args:std::collections::BTreeMap<_,_>=actual.iter().filter(|(k,_)|k!="token").cloned().collect();
        let raw=json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}});
        let socket=shell.core.workspaces[TEAM].rt_generation;
        let mut lock=closed().lock().unwrap();let c=lock.as_mut().unwrap();
        if c.requests.len()>=2||socket!=7+c.requests.len()as u64{return Err(Error::Offline("Unexpected original assignment read".into()));}
        c.requests.push(json!({"serial":c.requests.len()+1,"socket":socket,"method":"POST","path":"/api/experiments.getByUser","args":args,"tokenPresent":actual.iter().filter(|(k,v)|k=="token"&&v==TOKEN).count()==1,"factoryMatched":true,"response":raw}));Ok(raw)
    }).await?;
    if !witness.current(&state.peek()) {
        return Err("Accepted assignment retired".into());
    }
    let receipt = witness.snapshot(&state.peek());
    closed()
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .witnesses
        .push(witness);
    Ok(receipt)
}
pub(in crate::canvas::comments) fn retire(shell: &mut ShellState) -> Result<Value, String> {
    if !context_enabled(shell) || shell.core.workspaces[TEAM].rt_generation != 7 {
        return Err("Original socket unavailable".into());
    }
    let original = closed()
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .witnesses
        .first()
        .cloned()
        .ok_or("Original assignment unavailable")?;
    let before = original.snapshot(shell);
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 8;
    closed().lock().unwrap().as_mut().unwrap().socket = 8;
    let _ = channel_fetches::connected(shell, TEAM, 8);
    let after = original.snapshot(shell);
    let available = channel_fetches::emoji_standard(shell, TEAM).is_some();
    if before["current"] != true || after["current"] != false || available {
        return Err("Original socket map did not retire".into());
    }
    Ok(
        json!({"kind":"synthetic-socket-connected","before":before,"after":after,"socket":8,"accessorAvailable":available,"networkDispatched":false,"workResumed":false}),
    )
}
pub(in crate::canvas::comments) fn media(shell: &ShellState) -> Result<Value, String> {
    if !context_enabled(shell) {
        return Err("Original fixture context missing".into());
    }
    let id = shell
        .media
        .register_image(MediaAssetKind::Emoji, URL, "image/png");
    if shell.media.is_ready(&id) {
        return Err("Original image already ready".into());
    }
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icons/png/icon_16.png"
    ));
    let before = id.webview_uri_at(shell.media_epoch);
    shell
        .media
        .insert(id.clone(), "image/png", Arc::<[u8]>::from(bytes.as_slice()));
    Ok(
        json!({"url":URL,"id":id.uri(),"uriBefore":before,"epochBefore":shell.media_epoch,"bytes":bytes.len(),"mime":"image/png","readyBefore":false,"readyAfter":shell.media.is_ready(&id)}),
    )
}

fn request_url(raw: &str) -> Option<(url::Url, std::collections::BTreeMap<String, String>)> {
    let mut url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut query = std::collections::BTreeMap::new();
    for (key, value) in url.query_pairs() {
        let value = match key.as_ref() {
            "_x_id" | "_x_csid" | "fp" => {
                let bytes = value.as_bytes();
                if bytes.len() != 32
                    || !bytes
                        .iter()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                    || bytes[12] != b'4'
                    || !matches!(bytes[16], b'8' | b'9' | b'a' | b'b')
                {
                    return None;
                }
                String::new()
            }
            "_x_desktop_ia"
            | "_x_foreground"
            | "_x_frontend_build_type"
            | "_x_gantry"
            | "_x_num_retries"
            | "_x_version_ts"
            | "slack_route" => value.into_owned(),
            _ => return None,
        };
        if query.insert(key.into_owned(), value).is_some() {
            return None;
        }
    }
    if query.len() != 10 {
        return None;
    }
    url.set_query(None);
    Some((url, query))
}
