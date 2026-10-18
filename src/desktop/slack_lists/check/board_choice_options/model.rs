//! Passive production state and complete modal/DOM authority checks.
use super::*;
use crate::slack_lists::ProjectionLayout;
pub(super) fn projection(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing Board choice projection")?;
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
        return Err("Board choice fixture owner/projection differs".into());
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
    pub fn stable(
        &self,
        state: Signal<ShellState>,
        mode: usize,
        page_mode: usize,
    ) -> Result<(), String> {
        let p = projection(state)?;
        let rows = (0..9).map(|i| format!("R_LIST_{i:03}")).collect::<Vec<_>>();
        if document(state)? != self.docs[mode]
            || population(state)? != self.population[page_mode]
            || find(state)? != self.find
            || p["lanes"] != lanes(mode, rows, &self.docs)
            || p["columns"] != json!(["C_NAME", "C_NUMBER"])
            || p["expanded"] != self.expanded
            || p["matched"] != 9
            || p["incomplete"] != true
            || p["viewOrder"] != oracle::views()
            || p["localOrder"] != Value::Null
        {
            return Err("Board choice raw document/page/Find/projection drift".into());
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
            return Err("Wrong Board choice stage order".into());
        }
        let mode = usize::from(ordinal >= 5);
        let page_mode = 0;
        self.stable(state, mode, page_mode)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::stage(&ledger, ordinal, &self.docs, &self.population, self.serial)?;
        let surface = name == "board";
        let draft_before = if surface { Value::Null } else { draft(state)? };
        let bound = if surface {
            Value::Null
        } else {
            wait_dom(
                d,
                &format!("window.__listBoardOptions.sameDraft({draft_before})"),
            )
            .await?
        };
        let geometry = wait_dom(
            d,
            &format!("window.__listBoardOptions.geometry({})", json!(name)),
        )
        .await?;
        let capture = harness::capture(d, name).await?;
        self.stable(state, mode, page_mode)?;
        if rename_fixture::snapshot(&state.peek())? != ledger
            || !surface && draft(state)? != draft_before
        {
            return Err("Board choice authority changed during native capture".into());
        }
        let after_bound = if surface {
            Value::Null
        } else {
            checked(
                d,
                &format!("window.__listBoardOptions.sameDraft({draft_before})"),
            )
            .await?
        };
        // The final await cannot silently move the canonical/ledger fence.
        self.stable(state, mode, page_mode)?;
        if rename_fixture::snapshot(&state.peek())? != ledger
            || !surface && draft(state)? != draft_before
        {
            return Err("Board choice changed after final DOM witness".into());
        }
        self.captures.insert(name.into(), capture);
        self.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"bound":bound,"afterBound":after_bound,"draft":draft_before,"document":document(state)?,"population":population(state)?,"find":find(state)?,"projection":projection(state)?,"ledger":ledger,"custody":if surface{Value::Null}else{adapter::custody(&state.peek())?},"noSession":true,"noTransport":true}));
        Ok(())
    }
}

fn lanes(mode: usize, rows: Vec<String>, docs: &[Value; 2]) -> Value {
    let mut lanes = vec![
        json!({"key":"OPT_PROGRESS","label":"In progress","count":rows.len(),"rows":rows}),
        json!({"key":"OPT_DONE","label":"Done","count":0,"rows":[]}),
    ];
    if mode == 1 && adapter::case() == Some(adapter::Case::Add) {
        let choices = docs[1]["list_metadata"]["schema"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "C_SELECT")
            .unwrap()["options"]["choices"]
            .as_array()
            .unwrap();
        let added = choices.last().unwrap();
        lanes.push(json!({"key":added["value"],"label":"Later","count":0,"rows":[]}));
    }
    json!(lanes)
}
