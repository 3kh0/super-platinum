use super::*;

fn safe_fields(fields: &[(String, String)]) -> Value {
    json!(
        fields
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "token" | "enterprise_token"))
            .collect::<Vec<_>>()
    )
}
fn latest(workflow: &Value) -> Value {
    json!({"ok":true,"decorated_workflow":workflow,"available_data":{},"expression_data":{},"opaque_receipt":"first-publish"})
}
fn row(
    s: &ShellState,
    serial: usize,
    phase: &str,
    method: &str,
    fields: &[(String, String)],
    applied: bool,
) -> Value {
    json!({"serial":serial,"phase":phase,"method":method,"fields":safe_fields(fields),"applied":applied,
        "owner":{"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team}})
}
pub(crate) async fn read(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        let (method, fields) = super::super::decode(request)?;
        let result = {
            let m = model.0.lock().unwrap();
            match method {
                "functions.workflows.getLatest" => {
                    if !fields
                        .iter()
                        .any(|(k, v)| k == "workflow_id" && v == "WfREVIEW01")
                    {
                        return Err(closed());
                    }
                    let mut workflow = m.workflow.clone();
                    if case() == "first-stale-readback" && workflow["is_published"] == true {
                        workflow["is_published"] = json!(false);
                    }
                    Ok(latest(&workflow))
                }
                "workflows.triggers.list" => Ok(json!({"ok":true,"triggers":[m.trigger]})),
                "workflows.triggers.info" => Ok(json!({"ok":true,"trigger":m.trigger})),
                "functions.workflows.permissions.copy.list" => Ok(m.copy.clone()),
                "workflows.triggers.permissions.list" => Ok(m.run.clone()),
                "apps.certified.list" => Ok(
                    json!({"ok":true,"apps":[],"is_aaa_enabled":false,"can_user_manage_app":true}),
                ),
                "workflows.triggers.types.list" => Ok(
                    json!({"ok":true,"trigger_types":[{"trigger_type":{"id":"TT_SHORTCUT","type":"shortcut"},"is_available_to_user":true,"private_channel_access":false,"private_channel_team_ids":[]}]}),
                ),
                "api.getFlannelHttpUrl" => {
                    super::picker::validate_discovery(&s, request)?;
                    Ok(super::picker::discovery_raw())
                }
                _ => super::super::response(&mut s, request),
            }
        };
        if matches!(
            method,
            "functions.workflows.getLatest"
                | "workflows.triggers.list"
                | "workflows.triggers.info"
                | "functions.workflows.permissions.copy.list"
                | "workflows.triggers.permissions.list"
                | "apps.certified.list"
                | "workflows.triggers.types.list"
                | "api.getFlannelHttpUrl"
        ) {
            s.app_home
                .about
                .management
                .fixture
                .as_mut()
                .unwrap()
                .requests
                .push(request.clone());
        }
        let n = {
            let mut m = model.0.lock().unwrap();
            let n = m.requests.len() + 1;
            m.requests.push(row(&s, n, "read", method, fields, false));
            n
        };
        Ok((model, n, result))
    })();
    Some(match prepared {
        Ok((m, n, r)) => deliver(m, n, false, r).await,
        Err(e) => Err(e),
    })
}
pub(super) fn same(request: &PreparedRequest, expected: &PreparedRequest) -> Result<(), Error> {
    let normalized = |s: &str| -> Option<(String, Vec<(String, String)>)> {
        let u = url::Url::parse(s).ok()?;
        let mut pairs = vec![];
        for (k, v) in u.query_pairs() {
            let value = if matches!(k.as_ref(), "_x_id" | "_x_csid" | "fp") {
                if uuid::Uuid::parse_str(&v).ok()?.get_version_num() != 4 {
                    return None;
                }
                "trace".into()
            } else {
                v.into_owned()
            };
            pairs.push((k.into_owned(), value));
        }
        pairs.sort();
        Some((s.split('?').next()?.into(), pairs))
    };
    if request.method != expected.method
        || request.body != expected.body
        || request.headers != expected.headers
        || request.retry_safe() != expected.retry_safe()
        || normalized(&request.url)
            .zip(normalized(&expected.url))
            .is_none_or(|(a, b)| a != b)
    {
        return Err(closed());
    }
    Ok(())
}
fn expected(
    s: &ShellState,
    m: &Data,
    request: &PreparedRequest,
    method: &str,
    fields: &[(String, String)],
) -> Result<(), Error> {
    let h = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .ok_or_else(closed)?;
    let d = document(s, h.id).ok_or_else(closed)?;
    let field = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .ok_or_else(closed)
    };
    let metadata = method == "functions.workflows.update"
        || method == "workflows.triggers.update"
            && field("_x_reason")? == "update-hermes-workflow-metadata";
    let command = if metadata {
        let name = if method == "functions.workflows.update" {
            field("title")?
        } else {
            field("name")?
        };
        let icon = fields
            .iter()
            .find(|(k, _)| k == "icon_url")
            .map(|(_, v)| v.clone());
        Some(
            d.editor
                .metadata_command(&name, &field("description")?, icon.as_deref())?,
        )
    } else {
        match method {
            "functions.workflows.publish" => Some(protocol::Command::Published(true)),
            "functions.workflows.permissions.copy.set" => {
                Some(protocol::Command::CopyPermission("collaborators".into()))
            }
            "functions.workflows.collaborators.add" => {
                Some(protocol::Command::AddCollaborators(vec![
                    "UMANNEW01".into(),
                ]))
            }
            "functions.workflows.collaborators.remove" => {
                Some(protocol::Command::RemoveCollaborators(vec![
                    "UMANOLD01".into(),
                ]))
            }
            "workflows.triggers.permissions.set" | "functions.workflows.getIsRestricted" => {
                Some(protocol::Command::RunPermission {
                    permission_type: "everyone".into(),
                    user_ids: None,
                    channel_ids: None,
                    team_ids: None,
                    org_ids: None,
                })
            }
            "workflows.triggers.update" => None,
            _ => return Err(closed()),
        }
    };
    let expected = if let Some(command) = command {
        let mut plan = MutationPlan::new(&d.editor, command)?;
        if method == "functions.workflows.getIsRestricted" {
            plan.mark_sent(0)?;
            plan.settle(0, Outcome::Succeeded(json!({"ok":true})))?;
            protocol::FollowUpPlan::new(&plan)?.slots()[0]
                .plan()
                .request(&h.owner.ctx.client, &h.owner.ctx.session)
        } else {
            let slot = usize::from(metadata && method == "workflows.triggers.update");
            plan.slots()[slot]
                .plan()
                .request(&h.owner.ctx.client, &h.owner.ctx.session)
        }
    } else {
        protocol::TriggerUpdate::synchronize(&m.workflow, &m.trigger)?
            .ok_or_else(closed)?
            .request(&h.owner.ctx.client, &h.owner.ctx.session)
    };
    same(request, &expected)
}
pub(crate) async fn mutation(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        let (method, fields) = super::super::decode(request)?;
        let field = |key: &str| {
            fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .ok_or_else(closed)
        };
        let (n, hold, result, workflow, trigger) = {
            let mut m = model.0.lock().unwrap();
            if !m.armed {
                return Err(closed());
            }
            expected(&s, &m, request, method, fields)?;
            let reason = field("_x_reason")?;
            if m.requests
                .iter()
                .any(|r| r["phase"] == "mutation" && r["method"] == method && r["reason"] == reason)
            {
                return Err(closed());
            }
            let reject = case() == "first-metadata-rejected"
                && method == "functions.workflows.update"
                || case() == "first-settings-rejected-held"
                    && method == "functions.workflows.permissions.copy.set";
            let result = if reject {
                Ok(
                    json!({"ok":false,"error":"synthetic_rejected","opaque_receipt":"rejected-original"}),
                )
            } else {
                match method {
                    "functions.workflows.update" => {
                        m.workflow["title"] = json!(field("title")?);
                        m.workflow["description"] = json!(field("description")?);
                        if let Ok(icon) = field("icon_url") {
                            m.workflow["icons"] = json!({"image_192":icon});
                        }
                        Ok(latest(&m.workflow))
                    }
                    "workflows.triggers.update" => {
                        for key in ["name", "description", "inputs"] {
                            if let Ok(v) = field(key) {
                                m.trigger[key] = if key == "inputs" {
                                    serde_json::from_str(v).map_err(|_| closed())?
                                } else {
                                    json!(v)
                                };
                            }
                        }
                        Ok(
                            json!({"ok":true,"trigger":m.trigger,"opaque_receipt":"trigger-original"}),
                        )
                    }
                    "functions.workflows.permissions.copy.set" => {
                        m.copy = json!({"ok":true,"permission_type":field("permission_type")?});
                        Ok(m.copy.clone())
                    }
                    "workflows.triggers.permissions.set" => {
                        m.run = json!({"ok":true,"permission_type":field("permission_type")?,"user_ids":[],"channel_ids":[],"team_ids":[],"org_ids":[]});
                        Ok(m.run.clone())
                    }
                    "functions.workflows.getIsRestricted" => Ok(
                        json!({"ok":true,"is_restricted":false,"restricted_by_connector_steps_only":false}),
                    ),
                    "functions.workflows.collaborators.add"
                    | "functions.workflows.collaborators.remove" => {
                        let ids = field("collaborator_ids")?.split(',').collect::<Vec<_>>();
                        let selected = m.workflow["collaborators"]
                            .as_array_mut()
                            .ok_or_else(closed)?;
                        if method.ends_with(".add") {
                            for id in ids {
                                if !selected.iter().any(|v| v == id) {
                                    selected.push(json!(id));
                                }
                            }
                        } else {
                            selected.retain(|v| !ids.iter().any(|id| v == id));
                        }
                        Ok(latest(&m.workflow))
                    }
                    "functions.workflows.publish" => {
                        m.workflow["is_published"] = json!(true);
                        m.workflow["last_published_version_id"] = json!("VFIRST");
                        m.workflow["unpublished_change_count"] = json!(0);
                        m.workflow["date_updated"] = json!(2);
                        if case() == "first-primary-unknown" {
                            Err(Error::Transport(
                                "synthetic first publication reply lost".into(),
                            ))
                        } else {
                            Ok(latest(&m.workflow))
                        }
                    }
                    _ => return Err(closed()),
                }
            };
            let hold = case() == "first-retirement" && method == "functions.workflows.publish"
                || case() == "first-settings-rejected-held"
                    && method == "workflows.triggers.permissions.set";
            let n = m.requests.len() + 1;
            let mut r = row(&s, n, "mutation", method, fields, !reject);
            r["reason"] = json!(reason);
            m.requests.push(r);
            (n, hold, result, m.workflow.clone(), m.trigger.clone())
        };
        let f = s.app_home.about.management.fixture.as_mut().unwrap();
        f.workflow = workflow;
        f.trigger = trigger;
        f.writes += 1;
        f.requests.push(request.clone());
        Ok((model, n, hold, result))
    })();
    Some(match prepared {
        Ok((m, n, h, r)) => deliver(m, n, h, r).await,
        Err(e) => Err(e),
    })
}
