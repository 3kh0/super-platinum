use super::*;
/// This tuple preserves object result order from the closed original response.
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    if let Some(result) = ranking::read(state, request).await {
        return Some(result);
    }
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        if !active(&s) {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let now = Instant::now();
        let discovery = crate::app_home::about::management::fixture::decode(request)
            .ok()
            .is_some_and(|v| v.0 == "api.getFlannelHttpUrl");
        let (method, query, raw, keys, expected) = if discovery {
            (
                "api.getFlannelHttpUrl",
                "".to_owned(),
                dm::discovery(),
                None,
                DirectoryEndpoint::discovery(&client, &session)?,
            )
        } else {
            let kind = if request
                .url
                .split('?')
                .next()
                .is_some_and(|u| u.ends_with("/users/search"))
            {
                wire::Kind::Members
            } else if request
                .url
                .split('?')
                .next()
                .is_some_and(|u| u.ends_with("/channels/search"))
            {
                wire::Kind::Channels
            } else {
                return Err(closed());
            };
            let RequestBody::Json(body) = &request.body else {
                return Err(closed());
            };
            let query = body["query"].as_str().ok_or_else(closed)?;
            let marker = body
                .get("marker")
                .map(|v| v.as_str().ok_or_else(closed))
                .transpose()?;
            let plan = wire::Query::new(query)?;
            let endpoint = DirectoryEndpoint::decode(&session, &dm::discovery(), now)?;
            let expected = wire::Read::new(&session, kind, &plan, marker)?
                .request(&client, &session, &endpoint, now)?;
            let (raw, keys) = if empty::requested() {
                empty::reply(kind, query, marker)?
            } else if ranking::requested() {
                ranking::search_reply(kind, query, marker)?
            } else {
                pages::reply(kind, query, marker)?
            };
            (kind.method(), query.into(), raw, keys, expected)
        };
        super::super::super::super::picker::same_request(request, &expected)?;
        let mut m = model.0.lock().unwrap();
        let d = m.search.as_mut().ok_or_else(closed)?;
        if d.requests.len() >= 32 {
            return Err(closed());
        }
        let serial = d.requests.len() + 1;
        let hold = !discovery && !d.queries.contains(&query);
        if hold {
            d.queries.push(query.clone());
        }
        let rejected = query == "rejected";
        let offline = query == "offline";
        let response = if rejected {
            json!({"ok":false,"error":"not_allowed","opaque_receipt":"original-search-rejected"})
        } else {
            raw
        };
        d.requests.push(json!({"serial":serial,"method":method,"query":query,"body":safe_body(&request.body),"endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),"owner":owner(&s,&session)}));
        d.responses.push(if offline{json!({"serial":serial,"error":"offline"})}else{json!({"serial":serial,"raw":response,"orderedKeys":if rejected{None}else{keys.clone()}})});
        let rx = if hold {
            let (tx, rx) = oneshot::channel();
            d.held.push((serial, tx));
            Some(rx)
        } else {
            None
        };
        drop(m);
        Ok((model, serial, rx, response, keys, rejected, offline))
    })();
    let (model, serial, rx, raw, keys, rejected, offline) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settlement = Settlement {
        model,
        serial,
        returned: false,
    };
    if let Some(rx) = rx {
        if !matches!(
            tokio::time::timeout(Duration::from_secs(45), rx).await,
            Ok(Ok(()))
        ) {
            return Some(Err(closed()));
        }
    }
    settlement.returned = true;
    Some(if offline {
        Err(Error::Offline(
            "Synthetic destination search unavailable".into(),
        ))
    } else if rejected {
        Err(Error::Api("not_allowed".into()))
    } else {
        Ok((raw, keys))
    })
}
/// Fresh posting acquisition is distinct from search publication.
pub(crate) async fn ordinary(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if let Some(result) = ranking::ordinary(state, request).await {
        return Some(result);
    }
    if !requested() {
        return None;
    }
    let (method, fields) = crate::app_home::about::management::fixture::decode(request).ok()?;
    if method != "conversations.info" || !fields.iter().any(|(k, v)| k == "channel" && v == CHANNEL)
    {
        return None;
    }
    Some((|| {
        let s = state.peek();
        if !active(&s) {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let expected = api::conversations_info(&client, &session, CHANNEL.into());
        super::super::super::super::picker::same_request(request, &expected)?;
        let raw = json!({"ok":true,"channel":channel(),"opaque_receipt":"original-search-channel-posting"});
        // Passive source evidence BEFORE this fresh info response is returned.
        // The fixture never publishes or corrects membership itself.
        let source = crate::app_home::channel_fetches::canonical::observed(&s, "TWORKFLOW").ok();
        let canonical = source.as_ref().and_then(|v| v.row(CHANNEL)).cloned();
        let current = source.as_ref().is_some_and(|v| v.current(&s));
        let mut m = model.0.lock().unwrap();
        let serial = m.reads.len() + 1;
        m.search
            .as_mut()
            .ok_or_else(closed)?
            .posting_sources
            .push(json!({"readSerial":serial,"canonical":canonical,"current":current}));
        m.reads.push(json!({"serial":serial,"method":method,"body":safe_body(&request.body),"endpoint":request.url.split('?').next(),"retrySafe":request.retry_safe(),"owner":owner(&s,&session),"result":{"raw":raw}}));
        Ok(raw)
    })())
}
