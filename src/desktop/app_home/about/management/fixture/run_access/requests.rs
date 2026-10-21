use super::*;
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
        let witness = owner(&s);
        // This reason identifies the source-required post-set restriction request,
        // independently from preflight and explicit EditorLoad readback.
        let follow = method == "functions.workflows.getIsRestricted"
            && fields
                .iter()
                .any(|(k, v)| k == "_x_reason" && v == "set-trigger-run-permission");
        if follow {
            return Err(closed());
        } // Follow-ups enter the separately validated adapter below.
        let mut result = if method == "api.getFlannelHttpUrl" {
            picker::discovery(&mut s, request)
        } else {
            super::super::response(&mut s, request)
        };
        let serial = {
            let mut m = model.0.lock().unwrap();
            if let Ok(v) = &mut result {
                match method {
                    "functions.workflows.permissions.copy.list" => *v = m.copy.clone(),
                    "workflows.triggers.permissions.list" => *v = m.run.clone(),
                    "workflows.triggers.prefs.get" => *v = m.external.clone(),
                    "functions.workflows.getIsRestricted" => *v = m.restriction.clone(),
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
        Ok((model, serial, result)) => deliver(model, serial, false, result).await,
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
        wire::validate_mutation(&s, request, method, fields)?;
        let witness = owner(&s);
        let follow = method == "functions.workflows.getIsRestricted";
        let (serial, hold, result) = {
            let mut m = model.0.lock().unwrap();
            if !m.armed
                || m.requests.iter().any(|r| {
                    r["phase"] == (if follow { "followup" } else { "mutation" })
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
            let copy = method == "functions.workflows.permissions.copy.set";
            let rejected = !follow && !copy && case() == "run-rejected";
            if !follow && !rejected {
                if copy {
                    m.copy["permission_type"] = json!("everyone");
                } else {
                    m.run = raw::desired();
                    m.run["ok"] = json!(true);
                }
            }
            let result = if follow {
                if case() == "run-followup-failure" {
                    Err(Error::Offline(
                        "synthetic restriction follow-up offline".into(),
                    ))
                } else {
                    Ok(m.restriction.clone())
                }
            } else if rejected {
                Ok(json!({"ok":false,"error":"not_allowed"}))
            } else if !copy && case() == "run-unknown" {
                Err(Error::Transport(
                    "synthetic run permission reply lost after dispatch".into(),
                ))
            } else if copy {
                Ok(
                    json!({"ok":true,"permission_type":"everyone","opaque_receipt":{"preserve":[1,"copy"]}}),
                )
            } else {
                Ok(raw::run_ack())
            };
            let hold = !follow && !copy && !m.held_once && case() == "run-retirement";
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
            (serial, hold, result)
        };
        let parent = s.app_home.about.management.fixture.as_mut().unwrap();
        parent.requests.push(request.clone());
        if !follow {
            parent.writes += 1;
        }
        Ok((model, serial, hold, result))
    })();
    Some(match prepared {
        Ok((model, serial, hold, result)) => deliver(model, serial, hold, result).await,
        Err(e) => Err(e),
    })
}
