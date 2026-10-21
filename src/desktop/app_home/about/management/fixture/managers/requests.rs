use super::*;
pub(crate) async fn read(
    mut state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    if let Some(result) = agents::read(state, request).await {
        return Some(result);
    }
    let prepared = (|| {
        let mut s = state.write();
        let model = model(&s)?;
        let (method, fields) = super::super::decode(request)?;
        if method == "functions.workflows.getIsRestricted"
            && fields
                .iter()
                .any(|(k, v)| k == "_x_reason" && v == "set-trigger-run-permission")
        {
            return Err(closed());
        }
        let witness = owner(&s);
        let mut result = if method == "api.getFlannelHttpUrl" {
            picker::discovery(&mut s, request)
        } else {
            super::super::response(&mut s, request)
        };
        let serial = {
            let mut m = model.0.lock().unwrap();
            if let Ok(v) = &mut result {
                match method {
                    "functions.workflows.getLatest" => *v = raw::latest(&m.workflow),
                    "functions.workflows.permissions.copy.list" => *v = m.copy.clone(),
                    "workflows.triggers.permissions.list" => *v = m.run.clone(),
                    "workflows.triggers.prefs.get" => *v = m.external.clone(),
                    "functions.workflows.getIsRestricted" => *v = m.restriction.clone(),
                    "workflows.triggers.list" if case() == "managers-no-trigger" => {
                        *v = json!({"ok":true,"triggers":[]})
                    }
                    _ => {}
                }
            }
            let serial = m.requests.len() + 1;
            m.requests.push(wire::row(
                serial,
                "read",
                method,
                fields,
                request.retry_safe(),
                witness,
                false,
            ));
            serial
        };
        Ok((model, serial, result))
    })();
    Some(match prepared {
        Ok((m, n, r)) => deliver(m, n, false, r).await,
        Err(e) => Err(e),
    })
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
        let follow = method == "functions.workflows.getIsRestricted";
        let (serial, hold, result, workflow) = {
            let mut m = model.0.lock().unwrap();
            let witness = wire::validate_mutation(&s, &m, request, method)?;
            if !m.armed
                || m.requests.iter().any(|r| {
                    r["phase"] == if follow { "followup" } else { "mutation" }
                        && r["method"] == method
                })
            {
                return Err(closed());
            }
            if follow
                && !m.responses.iter().any(|v| {
                    v["result"]["raw"]["opaque_receipt"]["preserve"][1] == "run"
                        && v["result"]["raw"]["ok"] == true
                })
            {
                return Err(closed());
            }
            let add = method == "functions.workflows.collaborators.add";
            let remove = method == "functions.workflows.collaborators.remove";
            if remove
                && !m.responses.iter().any(|v| {
                    v["result"]["raw"]["decorated_workflow"]["collaborators"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|v| v == raw::NEW))
                        && v["result"]["raw"]["ok"] == true
                })
            {
                return Err(closed());
            }
            let rejected = add
                && matches!(
                    case().as_str(),
                    "managers-add-rejected" | "managers-self-rejected-held"
                );
            if !follow && !rejected {
                match method {
                    "functions.workflows.collaborators.add" => m.workflow["collaborators"]
                        .as_array_mut()
                        .ok_or_else(closed)?
                        .extend(raw::added_ids().into_iter().map(|id| json!(id))),
                    "functions.workflows.collaborators.remove" => m.workflow["collaborators"]
                        .as_array_mut()
                        .ok_or_else(closed)?
                        .retain(|v| {
                            v != if raw::self_remove() {
                                "UWORKFLOW"
                            } else {
                                raw::OLD
                            }
                        }),
                    "functions.workflows.permissions.copy.set" => {
                        m.copy["permission_type"] = json!("everyone")
                    }
                    "workflows.triggers.permissions.set" => m.run = raw::desired_run(),
                    _ => return Err(closed()),
                }
            }
            let result = if follow {
                Ok(m.restriction.clone())
            } else if rejected {
                Ok(json!({"ok":false,"error":"not_allowed"}))
            } else if add && case() == "managers-add-unknown" {
                Err(Error::Transport(
                    "synthetic manager add reply lost after application".into(),
                ))
            } else if add || remove {
                Ok(raw::ack(&m.workflow))
            } else if method == "functions.workflows.permissions.copy.set" {
                Ok(
                    json!({"ok":true,"permission_type":"everyone","opaque_receipt":{"preserve":[1,"copy"]}}),
                )
            } else {
                Ok(raw::run_ack())
            };
            let hold = !m.held_once
                && (add && case() == "managers-retirement"
                    || method == "workflows.triggers.permissions.set"
                        && case() == "managers-self-rejected-held");
            let serial = m.requests.len() + 1;
            m.requests.push(wire::row(
                serial,
                if follow { "followup" } else { "mutation" },
                method,
                fields,
                request.retry_safe(),
                witness,
                !follow && !rejected,
            ));
            (serial, hold, result, m.workflow.clone())
        };
        let parent = s.app_home.about.management.fixture.as_mut().unwrap();
        parent.requests.push(request.clone());
        if !follow {
            parent.writes += 1;
        }
        parent.workflow = workflow;
        Ok((model, serial, hold, result))
    })();
    Some(match prepared {
        Ok((m, n, h, r)) => deliver(m, n, h, r).await,
        Err(e) => Err(e),
    })
}
