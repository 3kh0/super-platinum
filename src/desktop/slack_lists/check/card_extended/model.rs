//! Passive production state and complete modal/DOM authority checks.
use super::*;
use crate::slack_lists::ProjectionLayout;
pub(super) fn projection(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Board card move projection")?;
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.layout_kind != ProjectionLayout::Board
        || p.selected_view.as_deref() != Some("V_LANE")
        || p.view_notice.is_some()
        || !p.incomplete
        || p.loading
        || p.stale
        || p.error.is_some()
        || p.read_only_reason.is_some()
    {
        return Err("Board card move fixture owner/projection differs".into());
    }
    let lanes = p.lanes.iter().map(|lane|json!({"key":lane.key,"label":lane.label,"count":lane.count,"rows":lane.rows.iter().map(|r|r.id.clone()).collect::<Vec<_>>() })).collect::<Vec<_>>();
    let v = shell.slack_lists.view.as_ref().unwrap();
    Ok(
        json!({"lanes":lanes,"columns":p.columns.iter().map(|c|c.id.clone()).collect::<Vec<_>>(),"visit":p.generation,"selection":v.layout.selection_generation,"layout":p.layout_generation,"findRevision":p.find_revision,"matched":p.matched_rows,"incomplete":p.incomplete,"account":shell.core.active_account,"epoch":shell.core.account_epoch,"expanded":v.expanded,"selectedView":p.selected_view,"viewOrder":p.view_choices.iter().map(|v|json!([v.id,v.name])).collect::<Vec<_>>(),"loading":p.loading,"stale":p.stale,"localOrder":v.layout.board_order.as_ref().map(|d|d.typed_order())}),
    )
}
pub(super) struct Proof {
    pub docs: [Value; 2],
    pub population: [Value; 2],
    pub find: Value,
    pub expanded: Value,
    pub serial: u64,
    pub stages: Vec<Value>,
    pub captures: serde_json::Map<String, Value>,
}
impl Proof {
    pub fn stable(&self, state: Signal<ShellState>, mode: usize) -> Result<(), String> {
        let p = projection(state)?;
        let expected = lanes(mode);
        let matched = if adapter::case() == Some(adapter::Case::Cross) {
            9
        } else {
            70
        };
        if document(state)? != self.docs[0]
            || population(state)? != self.population[mode]
            || find(state)? != self.find
            || p["lanes"] != expected
            || p["columns"] != json!(["C_NAME", "C_NUMBER"])
            || p["expanded"] != self.expanded
            || p["matched"] != matched
            || p["incomplete"] != true
            || p["viewOrder"] != oracle::views()
            || p["localOrder"] != Value::Null
        {
            return Err("Board card move raw document/page/Find/projection drift".into());
        }
        Ok(())
    }
    pub async fn stage(
        &mut self,
        state: Signal<ShellState>,
        d: &dioxus::desktop::DesktopContext,
        name: &str,
    ) -> Result<(), String> {
        let ordinal = self.stages.len();
        if oracle::STAGES.get(ordinal) != Some(&name) {
            return Err("Wrong Board card move stage order".into());
        }
        let mode = usize::from(ordinal >= 5);
        self.stable(state, mode)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::stage(&ledger, ordinal, &self.docs, &self.population, self.serial)?;
        let surface = matches!(name, "baseline" | "board");
        let draft_before = if surface { Value::Null } else { draft(state)? };
        let bound = if surface {
            Value::Null
        } else {
            wait_dom(
                d,
                &format!("window.__listCardExtended.sameDraft({draft_before})"),
            )
            .await?
        };
        let geometry = wait_dom(
            d,
            &format!("window.__listCardExtended.geometry({})", json!(name)),
        )
        .await?;
        let capture = harness::capture(d, name).await?;
        self.stable(state, mode)?;
        if rename_fixture::snapshot(&state.peek())? != ledger
            || !surface && draft(state)? != draft_before
        {
            return Err("Board card move authority changed during native capture".into());
        }
        let after_bound = if surface {
            Value::Null
        } else {
            checked(
                d,
                &format!("window.__listCardExtended.sameDraft({draft_before})"),
            )
            .await?
        };
        // The final await cannot silently move the canonical/ledger fence.
        self.stable(state, mode)?;
        if rename_fixture::snapshot(&state.peek())? != ledger
            || !surface && draft(state)? != draft_before
        {
            return Err("Board card move changed after final DOM witness".into());
        }
        self.captures.insert(name.into(), capture);
        self.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"bound":bound,"afterBound":after_bound,"draft":draft_before,"document":document(state)?,"population":population(state)?,"find":find(state)?,"projection":projection(state)?,"ledger":ledger,"custody":if surface{Value::Null}else{adapter::custody(&state.peek())?},"noSession":true,"noTransport":true}));
        Ok(())
    }
}

fn lanes(mode: usize) -> Value {
    let cross = adapter::case() == Some(adapter::Case::Cross);
    let mut progress = if cross {
        vec![0, 1, 2, 3, 6, 7, 8]
    } else {
        (0..73)
            .filter(|i| !matches!(i, 11 | 12 | 20))
            .collect::<Vec<_>>()
    };
    let mut done = if cross { vec![4, 5] } else { vec![] };
    if mode == 1 {
        if cross {
            progress.retain(|i| *i != 2);
            done.insert(1, 2);
        } else {
            progress.retain(|i| *i != 70);
            progress.push(70);
        }
    }
    let rows = |ids: Vec<usize>| {
        ids.into_iter()
            .map(|i| format!("R_LIST_{i:03}"))
            .collect::<Vec<_>>()
    };
    json!([{"key":"OPT_PROGRESS","label":"In progress","count":progress.len(),"rows":rows(progress)},{"key":"OPT_DONE","label":"Done","count":done.len(),"rows":rows(done)}])
}
