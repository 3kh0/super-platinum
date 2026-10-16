//! Passive actual-state proofs; fixture setup is explicitly separated from readback.
use super::*;
use crate::slack_lists::{ProjectionLayout, layout};
pub(super) fn projection(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let p = crate::slack_lists::presentation(&shell).ok_or("Missing projection")?;
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.selected_view.as_deref() != Some("V_GROUP")
        || p.view_notice.is_some()
        || !p.incomplete
    {
        return Err("Grouping isolation/projection failed".into());
    }
    let board = p.layout_kind == ProjectionLayout::Board;
    let mut groups = Vec::<Value>::new();
    if board {
        for lane in p.lanes.iter() {
            groups.push(json!({"key":lane.key,"label":lane.label,"count":lane.count,"rows":lane.rows.iter().map(|r|r.id.clone()).collect::<Vec<_>>()}));
        }
    } else {
        for e in p.entries.iter() {
            match e {
                layout::TableEntry::Group(g) => {
                    groups.push(json!({"key":g.key,"label":g.label,"count":g.count,"rows":[]}))
                }
                layout::TableEntry::Row(r) => {
                    if groups.is_empty() {
                        groups.push(json!({"key":"","label":"","count":0,"rows":[]}));
                    }
                    let g = groups.last_mut().unwrap();
                    g["rows"].as_array_mut().unwrap().push(json!(r.id));
                    if g["label"] == "" {
                        g["count"] = json!(g["rows"].as_array().unwrap().len());
                    }
                }
            }
        }
    }
    let v = shell.slack_lists.view.as_ref().unwrap();
    Ok(
        json!({"board":board,"groups":groups,"columns":p.columns.iter().map(|c|c.id.clone()).collect::<Vec<_>>(),"layout":p.layout_generation,"groupingRevision":p.grouping_revision,"findRevision":p.find_revision,"collapsed":v.layout.collapsed,"expanded":v.expanded,"visit":p.generation,"selection":v.layout.selection_generation,"account":shell.core.active_account,"epoch":shell.core.account_epoch,"matched":p.matched_rows,"incomplete":p.incomplete}),
    )
}
pub(super) fn setup_retirement(mut state: Signal<ShellState>) -> Result<Value, String> {
    let before = projection(state)?;
    {
        let mut shell = state.write();
        let v = shell.slack_lists.view.as_mut().ok_or("Missing List")?;
        v.layout
            .collapsed
            .insert("V_GROUP".into(), ["true".into()].into());
        v.layout
            .collapsed
            .insert("V_OTHER".into(), ["true".into()].into());
        crate::slack_lists::rebuild_rows(&mut shell);
    }
    let setup = projection(state)?;
    let shell = state.peek();
    let v = shell.slack_lists.view.as_ref().unwrap();
    let r = v.rows.first().ok_or("No retained row")?;
    if layout::instance_record(
        &shell,
        &r.instance_id,
        "C_NAME",
        v.generation,
        v.layout.generation,
    )
    .as_deref()
        != Some(r.id.as_str())
    {
        return Err("Initial layout authority missing".into());
    }
    Ok(
        json!({"before":before,"setup":setup,"token":{"visit":v.generation,"layout":v.layout.generation,"instance":r.instance_id,"row":r.id,"column":"C_NAME"}}),
    )
}
pub(super) fn retired_layout(state: Signal<ShellState>, old: &Value) -> Result<Value, String> {
    let after = projection(state)?;
    let setup = &old["setup"];
    let token = &old["token"];
    let shell = state.peek();
    let rejected = layout::instance_record(
        &shell,
        token["instance"].as_str().ok_or("Missing instance")?,
        "C_NAME",
        token["visit"].as_u64().ok_or("Missing visit")?,
        token["layout"].as_u64().ok_or("Missing layout")?,
    )
    .is_none();
    if !rejected
        || after["layout"] == setup["layout"]
        || after["groupingRevision"] == setup["groupingRevision"]
        || after["findRevision"] != setup["findRevision"]
        || after["expanded"] != setup["expanded"]
        || after["collapsed"].get("V_GROUP").is_some()
        || after["collapsed"]["V_OTHER"] != setup["collapsed"]["V_OTHER"]
        || after["groups"] != old["before"]["groups"]
    {
        return Err("Conflict failed exact grouping identity retirement".into());
    }
    Ok(json!({"before":old,"after":after,"oldLayoutRejected":rejected,"sameBucketMembers":true}))
}
pub(super) struct Proof {
    pub board: bool,
    pub docs: [Value; 4],
    pub population: Value,
    pub find: Value,
    pub expanded: Value,
    pub ids: [u64; 3],
    pub stages: Vec<Value>,
    pub captures: serde_json::Map<String, Value>,
}
impl Proof {
    pub fn stable(&self, state: Signal<ShellState>, mode: usize) -> Result<(), String> {
        let p = projection(state)?;
        if p["board"] != self.board
            || p["groups"] != oracle::groups(mode, self.board)
            || p["columns"] != oracle::columns(mode, self.board)
            || document(state)? != self.docs[mode]
            || population(state)? != self.population
            || find(state)? != self.find
            || p["expanded"] != self.expanded
        {
            return Err(format!(
                "Grouping raw/projection/Find drift in mode {mode}: {}",
                p["groups"]
            ));
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
            return Err("Wrong Group by stage order".into());
        }
        let mode = oracle::MODES[ordinal];
        self.stable(state, mode)?;
        let ledger = rename_fixture::snapshot(&state.peek())?;
        oracle::stage(&ledger, ordinal, &self.docs, self.ids)?;
        let surface = matches!(ordinal, 5 | 6 | 8);
        let samples = if surface {
            checked(
                d,
                &format!("window.__listViewGrouping.surfaceSamples({})", json!(name)),
            )
            .await?
        } else {
            Value::Null
        };
        let geometry = wait_dom(
            d,
            &format!("window.__listViewGrouping.geometry({})", json!(name)),
        )
        .await?;
        let model = if surface { Value::Null } else { draft(state)? };
        let owner = |v: &Value, n: Option<u64>| v.as_str() == n.map(|n| n.to_string()).as_deref();
        if !owner(
            &geometry["owner"]["visit"],
            crate::slack_lists::generation(&state.peek()),
        ) || !owner(
            &geometry["owner"]["selection"],
            rename::selection(&state.peek()),
        ) || !surface
            && (!owner(&geometry["owner"]["serial"], model["serial"].as_u64())
                || !owner(&geometry["draft"]["choice"], model["choice"].as_u64())
                || geometry["owner"]["view"] != "V_GROUP")
        {
            return Err("Rendered Group by owner differs".into());
        }
        self.captures
            .insert(name.into(), harness::capture(d, name).await?);
        self.stable(state, mode)?;
        if rename_fixture::snapshot(&state.peek())? != ledger {
            return Err("Grouping ledger changed during capture".into());
        }
        self.stages.push(json!({"ordinal":ordinal,"stage":name,"geometry":geometry,"samples":samples,"ledger":ledger,"document":document(state)?,"population":population(state)?,"find":find(state)?,"projection":projection(state)?,"draft":model,"custody":if surface{Value::Null}else{adapter::custody(&state.peek())?},"noSession":true,"noTransport":true}));
        Ok(())
    }
}
