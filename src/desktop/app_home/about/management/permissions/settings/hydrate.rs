//! Missing original grants are read by identity, never resolved by a text search.
use super::*;
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{channel_info, member_info};

fn required(draft: Option<&Draft>, raw: Option<&Value>) -> Vec<String> {
    let mut ids = draft.map(|d| d.ids().to_vec()).unwrap_or_default();
    if let Some(raw) = raw.filter(|_| draft.is_none()) {
        for key in ["user_ids", "channel_ids", "team_ids", "org_ids"] {
            if let Some(a) = raw[key].as_array() {
                ids.extend(a.iter().filter_map(Value::as_str).map(str::to_owned));
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}
fn missing(v: &Settings, o: &Original) -> Vec<String> {
    missing_ids(v.draft.as_ref(), o.editor.run_permissions(), |id| {
        v.rows.contains_key(id)
    })
}
fn missing_ids(
    draft: Option<&Draft>,
    raw: Option<&Value>,
    contains: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut ids = required(draft, raw);
    ids.retain(|id| !contains(id));
    ids
}
pub(super) fn has_missing(v: &Settings, o: &Original) -> bool {
    !missing(v, o).is_empty()
}
pub(in crate::app_home::about::management) fn start(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    retry: bool,
) {
    let captured = {
        let s = state.peek();
        let Some(c) = owned(&s, visit, id) else {
            return;
        };
        let Some(v) = c.settings.as_ref() else { return };
        if v.pending || v.hydration_started && !retry {
            return;
        }
        let ids = missing(v, &c.original);
        if ids.is_empty() {
            return;
        }
        (c.original.clone(), v.generation, ids)
    };
    dioxus::core::spawn_forever(async move {
        let (o, generation, ids) = captured;
        if acquire::authorize(state, &o, generation).await.is_err() {
            return;
        }
        {
            let mut s = state.write();
            if !acquire::current(&s, &o, generation) {
                return;
            }
            let v = s
                .app_home
                .about
                .management
                .permissions
                .confirmation
                .as_mut()
                .unwrap()
                .settings
                .as_mut()
                .unwrap();
            if v.pending || v.hydration_started && !retry {
                return;
            }
            v.hydration_started = true;
            v.pending = true;
            v.notice = None;
        }
        let result = load(state, &o, generation, &ids).await;
        if acquire::authorize(state, &o, generation).await.is_err() {
            return;
        }
        let mut s = state.write();
        if let Err(Error::RateLimited { retry_after_secs }) = &result {
            navigation::retain_limit(&mut s, &o.owner.ctx, *retry_after_secs);
        }
        if !acquire::current(&s, &o, generation) {
            return;
        }
        let v = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap()
            .settings
            .as_mut()
            .unwrap();
        v.pending = false;
        match result{
   Ok((rows,receipts))=>{
    for raw in rows{if let Some((e,row))=source::row(&raw,&v.context){v.context.entities.retain(|x|x.id!=e.id);v.context.entities.push(e);v.rows.insert(row.id.clone(),row);}}
    v.receipts.extend(receipts);
    if v.draft.is_none(){v.draft=o.editor.run_permissions().and_then(|raw|Draft::from_acquired(raw,&v.context).ok());}
    v.notice=(!missing(v,&o).is_empty()).then(||"Some selected references are unavailable. Retry to load their details.".into());
   },
   Err(_)=>v.notice=Some("Selected reference details could not be loaded. Your settings are retained. Retry to load their details.".into()),
  }
        v.epoch = next();
        drop(s);
        actions::bind_settings(state, visit, id);
    });
}
async fn load(
    state: Signal<ShellState>,
    o: &Original,
    generation: u64,
    ids: &[String],
) -> Result<(Vec<Value>, Vec<Value>), Error> {
    let mut rows = Vec::new();
    let mut receipts = Vec::new();
    let edge_needed = ids.iter().any(|id| {
        matches!(
            id.as_bytes().first(),
            Some(b'U' | b'W' | b'C' | b'G' | b'D' | b'Z')
        )
    });
    let endpoint = if edge_needed {
        let (e, r) = acquire::discovery(state, o, generation).await?;
        receipts.push(r);
        Some(e)
    } else {
        None
    };
    for members in [true, false] {
        let selected: Vec<_> = ids
            .iter()
            .filter(|id| {
                if members {
                    matches!(id.as_bytes().first(), Some(b'U' | b'W'))
                } else {
                    matches!(id.as_bytes().first(), Some(b'C' | b'G' | b'D' | b'Z'))
                }
            })
            .cloned()
            .collect();
        for chunk in selected.chunks(100) {
            let mut marker = None;
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..20 {
                acquire::authorize(state, o, generation).await?;
                let ctx = &o.owner.ctx;
                let endpoint = endpoint.as_ref().ok_or_else(unavailable)?;
                let member = member_info::Read::capture(chunk.to_vec());
                let channel = channel_info::Read::capture(chunk.to_vec(), None);
                let mr = if members {
                    let r = member?;
                    Some(if let Some(m) = marker.as_deref() {
                        r.next_page(m)?
                    } else {
                        r
                    })
                } else {
                    None
                };
                let cr = if !members {
                    let r = channel?;
                    Some(if let Some(m) = marker.as_deref() {
                        r.next_page(m)?
                    } else {
                        r
                    })
                } else {
                    None
                };
                let request = if let Some(r) = &mr {
                    endpoint.member_info(&ctx.client, &ctx.session, Instant::now(), r)?
                } else {
                    endpoint.channel_info(
                        &ctx.client,
                        &ctx.session,
                        Instant::now(),
                        cr.as_ref().unwrap(),
                    )?
                };
                let result = if fixture::enabled(&state.peek()) {
                    tokio::select! {r=crate::app_home::about::management::permissions::picker_request(state,&request,false)=>r.ok_or_else(unavailable)?,_=acquire::retired(state,o,generation)=>return Err(unavailable())}
                } else if crate::fixture::is_fixture() {
                    return Err(unavailable());
                } else if members {
                    tokio::select! {r=ctx.transport.execute_directory_member_info_current(request,||acquire::current(&state.peek(),o,generation))=>r.map(|v|v.into_parts()),_=acquire::retired(state,o,generation)=>return Err(unavailable())}
                } else {
                    tokio::select! {r=ctx.transport.execute_directory_channel_info_current(request,||acquire::current(&state.peek(),o,generation))=>r.map(|v|v.into_parts()),_=acquire::retired(state,o,generation)=>return Err(unavailable())}
                };
                if !acquire::current(&state.peek(), o, generation) {
                    return Err(unavailable());
                }
                let (raw, keys) = result?;
                if let Some(r) = mr {
                    let page = member_info::Page::decode(&r, raw, keys)?;
                    rows.extend_from_slice(page.rows());
                    receipts.push(page.raw().clone());
                    marker = page.next_marker().map(str::to_owned);
                } else {
                    let page = channel_info::Page::decode(cr.as_ref().unwrap(), raw, keys)?;
                    for raw in page.rows() {
                        let mut row = raw.clone();
                        row["is_member"] =
                            json!(page.membership_ids().iter().any(|id| row["id"] == *id));
                        rows.push(row);
                    }
                    receipts.push(page.raw().clone());
                    marker = page.next_marker().map(str::to_owned);
                }
                let Some(next) = marker.as_ref() else { break };
                if !seen.insert(next.clone()) {
                    return Err(unavailable());
                }
            }
            if marker.is_some() {
                return Err(unavailable());
            }
        }
    }
    let teams: Vec<_> = ids
        .iter()
        .filter(|id| matches!(id.as_bytes().first(), Some(b'T' | b'E')))
        .cloned()
        .collect();
    for chunk in teams.chunks(250) {
        let read =
            protocol::settings_acquisition::Read::teams_by_ids(&o.owner.ctx.session, chunk, None)?;
        let (raw, keys) = acquire::request(state, o, generation, &read, None).await?;
        let page = read.decode_ordered(raw, keys.as_deref())?;
        rows.extend_from_slice(page.rows());
        receipts.push(page.raw().clone());
    }
    Ok((rows, receipts))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hydration_retry_requires_an_actual_missing_selected_identity() {
        let raw = json!({"user_ids":["UWORKFLOW"],"team_ids":["TRUNEXTER"]});
        assert!(missing_ids(None, Some(&raw), |_| true).is_empty());
        assert_eq!(
            missing_ids(None, Some(&raw), |id| id == "UWORKFLOW"),
            vec!["TRUNEXTER"]
        );
        assert!(
            missing_ids(None, Some(&json!({"permission_type":"everyone"})), |_| {
                false
            })
            .is_empty()
        );
    }
    #[test]
    fn actual_named_transition_hydrates_missing_collaborators_without_copy_only_reads() {
        let c = Context {
            current_team_id: "TWORKFLOW".into(),
            scope_id: "TWORKFLOW".into(),
            unified_user: false,
            entities: vec![Entity {
                id: "TWORKFLOW".into(),
                kind: Kind::Team,
                enterprise_id: None,
                eligible: true,
            }],
            collaborators: vec!["UMISSING01".into()],
            available_external_ids: None,
        };
        let raw = json!({"ok":true,"permission_type":"app_collaborators"});
        let mut d = Draft::from_acquired(&raw, &c).unwrap();
        assert!(required(Some(&d), Some(&raw)).is_empty());
        d.select_mode(Mode::NamedEntities, &c).unwrap();
        assert_eq!(required(Some(&d), Some(&raw)), vec!["UMISSING01"]);
        d.replace_internal(vec![], &c).unwrap();
        assert!(required(Some(&d), Some(&raw)).is_empty());
    }
    #[test]
    fn unresolved_original_team_and_user_ids_are_identity_reads_not_query_matches() {
        let raw = json!({"user_ids":["UMISSING01","UMISSING01"],"channel_ids":["CMISSING01"],"team_ids":["TMISSING01"],"org_ids":["EMISSING01"]});
        assert_eq!(
            required(None, Some(&raw)),
            vec!["CMISSING01", "EMISSING01", "TMISSING01", "UMISSING01"]
        );
    }
}
