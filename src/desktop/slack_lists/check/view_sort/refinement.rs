//! Current toolbar contract: local Sort Apply, then actual shared Save action.
use super::*;
use rename_fixture::sort_refinement as adapter;
mod flow;
mod oracle;
pub(super) const LEGACY: &[&str] = &[
    "asc-edit",
    "asc-applied",
    "asc-saving",
    "uncertain",
    "stale",
    "retired",
    "ascending",
    "desc-edit",
    "desc-applied",
    "desc-saving",
    "descending",
    "clear-edit",
    "clear-applied",
    "clear-saving",
    "cleared",
];
pub(super) const ORDERED: &[&str] = &[
    "structure-added",
    "structure-removed",
    "multi-edit",
    "multi-applied",
    "multi-saving",
    "uncertain",
    "stale",
    "retired",
    "multi",
    "reorder-edit",
    "reorder-applied",
    "reorder-saving",
    "reordered",
    "remove-edit",
    "remove-applied",
    "remove-saving",
    "single",
    "clear-edit",
    "clear-applied",
    "clear-saving",
    "cleared",
];
pub(super) fn names() -> &'static [&'static str] {
    if rename_fixture::ordered_sort_check_enabled() {
        ORDERED
    } else {
        LEGACY
    }
}
struct CurrentProof {
    ordered: bool,
    docs: Vec<Value>,
    population: Value,
    expanded: Value,
    find: Value,
    ids: Vec<u64>,
    stages: Vec<Value>,
    captures: serde_json::Map<String, Value>,
}
impl CurrentProof {
    fn model(
        &self,
        state: Signal<ShellState>,
        canonical: usize,
        projection: usize,
    ) -> Result<Value, String> {
        let shell = state.peek();
        let raw = adapter::state(&shell)?;
        let p = crate::slack_lists::presentation(&shell).ok_or("Missing Sort projection")?;
        let rows = p
            .entries
            .iter()
            .filter_map(|e| match e {
                crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let want = if self.ordered {
            super::ordered::order(projection)
        } else {
            super::oracle::order(projection)
        };
        if raw["document"] != self.docs[canonical]
            || raw["expanded"] != self.expanded
            || raw["population"] != self.population
            || raw["find"] != self.find
            || raw["selected"] != "V_SORT"
            || rows != want
            || !p.incomplete
            || p.view_notice.is_some()
            || p.layout_kind != crate::slack_lists::ProjectionLayout::Table
            || shell.core.session.is_some()
            || shell.core.transport.is_some()
        {
            return Err("Sort canonical/local projection, raw population or owner drift".into());
        }
        Ok(json!({"state":raw,"rows":rows,"partial":true,"noSession":true,"noTransport":true}))
    }
    fn exact(
        &self,
        state: Signal<ShellState>,
        count: usize,
        finished: usize,
        server: usize,
        write: bool,
        read: bool,
        child: bool,
    ) -> Result<Value, String> {
        let l = adapter::snapshot(&state.peek())?;
        oracle::exact(
            &l,
            &self.docs,
            self.ordered,
            &self.ids,
            count,
            finished,
            server,
            write,
            read,
            child,
        )?;
        if l["population"] != self.population {
            return Err("Fixture raw population drift".into());
        }
        Ok(l)
    }
    async fn stage(
        &mut self,
        state: Signal<ShellState>,
        d: &dioxus::desktop::DesktopContext,
        name: &str,
        canonical: usize,
        projection: usize,
        count: usize,
        finished: usize,
        server: usize,
        write: bool,
    ) -> Result<(), String> {
        if names().get(self.stages.len()) != Some(&name) {
            return Err("Wrong current Sort stage order".into());
        }
        let model = self.model(state, canonical, projection)?;
        let l = self.exact(state, count, finished, server, write, false, false)?;
        let editing = model["state"]["local"] == true;
        let typed = if editing { draft(state)? } else { Value::Null };
        if editing {
            checked(d, &format!("window.__listViewSort.sameDraft({typed})")).await?;
        }
        // Reveal is separately recorded; the geometry witness itself never repairs focus.
        checked(d, "window.__listSortRefinement.revealCapture()").await?;
        let geometry = wait_dom(
            d,
            &format!(
                "window.__listSortRefinement.geometry({},{},{})",
                json!(name),
                model,
                typed
            ),
        )
        .await?;
        let capture = harness::capture(d, name).await?;
        let after = checked(
            d,
            &format!(
                "window.__listSortRefinement.geometry({},{},{})",
                json!(name),
                model,
                typed
            ),
        )
        .await?;
        if self.model(state, canonical, projection)? != model
            || adapter::snapshot(&state.peek())? != l
        {
            return Err("Current Sort changed during capture".into());
        }
        self.captures.insert(name.into(), capture);
        self.stages.push(json!({"stage":name,"model":model,"typed":typed,"ledger":l,"geometry":geometry,"geometryAfter":after}));
        Ok(())
    }
}
async fn wait(
    state: Signal<ShellState>,
    label: &str,
    test: impl Fn(&Value, &Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..150 {
        let s = adapter::state(&state.peek())?;
        let l = adapter::snapshot(&state.peek())?;
        if test(&s, &l) {
            return Ok(json!({"state":s,"ledger":l}));
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Current Sort did not reach {label}"))
}
async fn act(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<Value, String> {
    checked(
        d,
        &format!("window.__listViewSort.click({})", json!(format!("#{id}"))),
    )
    .await
}
async fn dismiss(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let action = act(d, "slack-list-rename-close").await?;
    wait(state, "Save review dismissed", |s, _| s["stage"].is_null()).await?;
    let focus = wait_dom(d, "window.__listSortRefinement.returned('save')").await?;
    Ok(json!({"action":action,"focus":focus}))
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("../view_sort.js")).await?;
    checked(d, include_str!("rows.js")).await?;
    let ordered = rename_fixture::ordered_sort_check_enabled();
    checked(
        d,
        &format!(
            "window.__listViewSort.setMode({})",
            json!(if ordered { "ordered" } else { "legacy" })
        ),
    )
    .await?;
    checked(d, include_str!("refinement/dom.js")).await?;
    wait_dom(d, "window.__listSortRefinement.ready()").await?;
    let initial = adapter::state(&state.peek())?;
    if initial["stage"] != Value::Null
        || initial["localEmpty"] != true
        || initial["population"]["records"].as_array().map(Vec::len) != Some(73)
        || initial["population"]["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Wrong current Sort initial seed".into());
    }
    let query = checked(d, "window.__listViewSort.query()").await?;
    wait(state, "Find debounce", |s, _| {
        s["find"]["applied"] == "Launch item 0"
            && s["find"]["revision"].as_u64().is_some_and(|n| n > 0)
    })
    .await?;
    let first_find = checked(d, "window.__listViewSort.bindFind()").await?;
    let raw = adapter::state(&state.peek())?;
    let mut proof = CurrentProof {
        ordered,
        docs: oracle::documents(&raw["document"], ordered),
        population: raw["population"].clone(),
        expanded: raw["expanded"].clone(),
        find: raw["find"].clone(),
        ids: vec![],
        stages: vec![],
        captures: Default::default(),
    };
    proof.model(state, 0, 0)?;
    let canceled = if ordered {
        flow::prelude(&mut proof, state, d).await?
    } else {
        open_choice(state, d, 2).await?
    };
    let cancel_click = act(d, "slack-list-rename-close").await?;
    let cancel_focus = wait_dom(d, "window.__listViewSort.returnedSort()").await?;
    let cancel_ledger = proof.exact(state, 0, 0, 0, false, false, false)?;
    if rename::presentation(&state.peek()).is_some() {
        return Err("Cancel retained local editor".into());
    }
    let steps = if ordered { 4 } else { 3 };
    let mut edits = vec![];
    let mut saves = vec![];
    let mut retirement = Value::Null;
    let mut count = 0;
    let mut finished = 0;
    for step in 0..steps {
        let mode = if step + 1 == steps { 0 } else { step + 1 };
        let projection_before = if step == 0 { 0 } else { step };
        let edit = open_choice(state, d, mode).await?;
        let stale = if step == 0 {
            adapter::stale_confirmation(
                &mut state.write(),
                canceled["model"]["serial"]
                    .as_u64()
                    .ok_or("Missing canceled serial")?,
            )?
        } else {
            Value::Null
        };
        let prefix = if step + 1 == steps {
            "clear"
        } else if ordered {
            ["multi", "reorder", "remove"][step]
        } else {
            ["asc", "desc"][step]
        };
        proof
            .stage(
                state,
                d,
                &format!("{prefix}-edit"),
                step,
                projection_before,
                count,
                finished,
                step,
                false,
            )
            .await?;
        let apply = act(d, "slack-list-rename-confirm").await?;
        wait(state, "local Apply", |s, l| {
            s["stage"].is_null()
                && s["effectiveSorts"] == oracle::sorts(step, ordered)
                && l["requests"].as_array().map(Vec::len) == Some(count)
        })
        .await?;
        let applied_focus = wait_dom(d, "window.__listViewSort.returnedSort()").await?;
        let projection_after = if mode == 0 { 0 } else { mode };
        proof
            .stage(
                state,
                d,
                &format!("{prefix}-applied"),
                step,
                projection_after,
                count,
                finished,
                step,
                false,
            )
            .await?;
        let save = act(d, "slack-list-refinement-save").await?;
        phase(state, Stage::Saving).await?;
        pending(state, "writePending").await?;
        let serial = adapter::state(&state.peek())?["serial"]
            .as_u64()
            .ok_or("Missing shared Save serial")?;
        if proof.ids.contains(&serial) || serial == edit["model"]["serial"].as_u64().unwrap() {
            return Err("Local/shared attempt identity reused".into());
        }
        proof.ids.push(serial);
        wait_dom(
            d,
            &format!("window.__listSortRefinement.bindSave({serial})"),
        )
        .await?;
        count += 5;
        proof
            .stage(
                state,
                d,
                &format!("{prefix}-saving"),
                step,
                projection_after,
                count,
                finished,
                step,
                true,
            )
            .await?;
        rename_fixture::release(&state.peek(), false)?;
        phase(state, Stage::Uncertain).await?;
        completed(state, finished + 1).await?;
        finished += 1;
        let released = if step == 0 {
            proof
                .stage(state, d, "uncertain", 0, 1, count, finished, 1, false)
                .await?;
            adapter::stale_file(&state.peek())?;
            act(d, "slack-list-rename-check").await?;
            phase(state, Stage::Conflict).await?;
            count += 3;
            finished += 1;
            proof
                .stage(state, d, "stale", 0, 1, count, finished, 1, false)
                .await?;
            retirement = flow::retire(&mut proof, state, d, count, finished).await?;
            count += 1;
            finished += 1;
            proof
                .stage(state, d, "retired", 0, 0, count, finished, 1, false)
                .await?;
            Value::Null
        } else {
            wait(state, "independent held preference", |_, l| {
                l["preferencePending"] == true
            })
            .await?;
            let child = proof.exact(state, count + 1, finished, step + 1, false, false, true)?;
            adapter::release_preference(&state.peek())?;
            let observed = wait(state, "independent preference observation", |s, l| {
                s["preference"] == "Your current default-view preference matches this Save."
                    && l["preferencePending"] == false
            })
            .await?;
            count += 2;
            proof.exact(state, count, finished, step + 1, false, false, false)?;
            json!({"held":child,"observed":observed})
        };
        act(d, "slack-list-rename-check").await?;
        phase(state, Stage::Observed).await?;
        count += 3;
        finished += 1;
        proof.model(state, step + 1, projection_after)?;
        let observed = adapter::state(&state.peek())?;
        let done = dismiss(state, d).await?;
        let final_name = if mode == 0 {
            "cleared"
        } else if ordered {
            ["multi", "reordered", "single"][step]
        } else {
            ["ascending", "descending"][step]
        };
        proof
            .stage(
                state,
                d,
                final_name,
                step + 1,
                projection_after,
                count,
                finished,
                step + 1,
                false,
            )
            .await?;
        edits.push(json!({"edit":edit,"staleConfirmation":stale,"apply":apply,"appliedFocus":applied_focus}));
        saves.push(json!({"open":save,"serial":serial,"preference":released,"observed":observed,"done":done}));
    }
    let ledger = proof.exact(state, count, finished, steps, false, false, false)?;
    let history = checked(d, "window.__listSortRefinement.history()").await?;
    if names().len() != proof.stages.len()
        || count != if ordered { 42 } else { 32 }
        || finished != if ordered { 10 } else { 8 }
    {
        return Err("Incomplete current Sort flow".into());
    }
    Ok(
        json!({"ok":true,"schema":3,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"owner":owner,"ordered":ordered,"initial":initial,"query":query,"firstFind":first_find,"documents":proof.docs,"population":proof.population,"find":proof.find,"cancel":{"draft":canceled,"click":cancel_click,"focus":cancel_focus,"ledger":cancel_ledger},"edits":edits,"saves":saves,"retirement":retirement,"stages":proof.stages,"captures":proof.captures,"ledger":ledger,"history":history,"registryAbsent":rename_fixture::sort_registry_empty(&state.peek())}),
    )
}
