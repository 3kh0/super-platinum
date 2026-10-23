use super::*;
struct Settled {
    model: Model,
    returned: bool,
}
impl Drop for Settled {
    fn drop(&mut self) {
        if let Some(d) = self.model.0.lock().unwrap().multiple.as_mut() {
            d.settlements
                .push(json!({"serial":1,"returned":self.returned}));
        }
    }
}
pub(crate) async fn open(
    state: Signal<ShellState>,
    request: &wire::Request,
    current: impl Fn() -> bool,
    mut observe: impl FnMut(wire::Attempt),
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        if !active(&s) || !current() || case() == "reuse" {
            return Err(closed());
        }
        let model = capture(&s).map_err(|_| closed())?;
        let (client, session) = context(&s)?;
        let peers = PEERS.iter().map(|id| peer(id).unwrap()).collect::<Vec<_>>();
        let expected =
            wire::Open::new(&peers, "TWORKFLOW", "UWORKFLOW", None)?.request(&client, &session)?;
        super::super::super::super::picker::same_request(request.prepared(), expected.prepared())?;
        let raw = response(&case());
        let (tx, rx) = oneshot::channel();
        {
            let mut m = model.0.lock().unwrap();
            let d = m.multiple.as_mut().ok_or_else(closed)?;
            if !d.requests.is_empty() {
                return Err(closed());
            }
            d.requests
                .push(json!({"serial":1,"request":request.inspect(),"owner":owner(&s,&session)}));
            d.responses
                .push(json!({"serial":1,"raw":raw,"mayHaveOpened":case()!="rejected"}));
            d.held = Some(tx);
        }
        Ok((model, rx, raw))
    })();
    let (model, rx, raw) = match prepared {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let mut settled = Settled {
        model: model.clone(),
        returned: false,
    };
    if !matches!(
        tokio::time::timeout(Duration::from_secs(45), rx).await,
        Ok(Ok(()))
    ) {
        return Some(Err(closed()));
    }
    let live = current();
    let rejected = raw["ok"] == false;
    let attempt = wire::Attempt {
        number: 1,
        status: Some(200),
        outcome: if !live {
            wire::Outcome::Retired
        } else if rejected {
            wire::Outcome::Rejected
        } else if request.open().decode(raw.clone()).is_ok() {
            wire::Outcome::Acknowledged
        } else {
            wire::Outcome::Unknown
        },
        raw: Some(raw.clone()),
        response_text: Some(raw.to_string()),
        retry_after_millis: None,
        may_have_applied: !rejected,
    };
    model
        .0
        .lock()
        .unwrap()
        .multiple
        .as_mut()
        .unwrap()
        .attempts
        .push(attempt.clone());
    observe(attempt);
    settled.returned = true;
    Some(if !live {
        Err(Error::Transport(
            "Original group selection retired; response retained".into(),
        ))
    } else if rejected {
        Err(Error::Api("not_allowed".into()))
    } else {
        Ok(raw)
    })
}
