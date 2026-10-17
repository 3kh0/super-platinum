//! Actual controls drive every configuration transition; expectations are literal.
use super::*;
use crate::slack_lists::rename::text_create::entity as settings;
#[derive(Clone)]
pub(super) struct Step {
    pub action: Value,
    pub wire: Value,
}
fn wire(channel: bool, multiple: bool, show: bool, ids: &[&str], empty: Option<bool>) -> Value {
    let mut v =
        json!({"format":if multiple{"multi_entity"}else{"single_entity"},"show_member_name":show});
    match empty {
        Some(true) => {}
        Some(false) => v["default_value"] = Value::Null,
        None => {
            v["default_value"] = json!(ids.join("|"));
            v["default_value_typed"] = if channel {
                json!({"channel":ids})
            } else {
                json!({"user":ids})
            };
        }
    };
    v
}
pub(super) fn plan(case: EntityCase) -> Vec<Step> {
    let channel = case.channel();
    let [b, a] = case.ids();
    let show = channel;
    let mut out = vec![];
    if !channel {
        out.push(Step {
            action: json!({"kind":"show","value":false}),
            wire: wire(false, false, false, &[], Some(false)),
        });
    }
    for (action, multiple, ids, empty) in [
        (
            json!({"kind":"multiple","value":true}),
            true,
            vec![],
            Some(false),
        ),
        (json!({"kind":"activate","value":b}), true, vec![b], None),
        (json!({"kind":"activate","value":a}), true, vec![b, a], None),
        (
            json!({"kind":"multiple","value":false}),
            false,
            vec![b, a],
            None,
        ),
        (json!({"kind":"activate","value":b}), false, vec![b], None),
        (
            json!({"kind":"activate","value":b}),
            false,
            vec![],
            Some(false),
        ),
        (
            json!({"kind":"multiple","value":true}),
            true,
            vec![],
            Some(false),
        ),
        (json!({"kind":"activate","value":b}), true, vec![b], None),
        (json!({"kind":"remove","value":b}), true, vec![], Some(true)),
        (json!({"kind":"activate","value":b}), true, vec![b], None),
        (json!({"kind":"activate","value":a}), true, vec![b, a], None),
    ] {
        out.push(Step {
            action,
            wire: wire(channel, multiple, show, &ids, empty),
        });
    }
    if !case.multiple() {
        out.push(Step {
            action: json!({"kind":"multiple","value":false}),
            wire: wire(channel, false, show, &[b, a], None),
        });
    }
    match case.empty() {
        Some(false) => {
            out.push(Step {
                action: json!({"kind":"activate","value":b}),
                wire: wire(channel, false, show, &[b], None),
            });
            out.push(Step {
                action: json!({"kind":"activate","value":b}),
                wire: wire(channel, false, show, &[], Some(false)),
            });
        }
        Some(true) => {
            out.push(Step {
                action: json!({"kind":"remove","value":b}),
                wire: wire(channel, true, show, &[a], None),
            });
            out.push(Step {
                action: json!({"kind":"remove","value":a}),
                wire: wire(channel, true, show, &[], Some(true)),
            });
        }
        None => {}
    }
    out
}
pub(super) fn model(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    if shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Disconnected entity fixture required".into());
    }
    let p = settings::presentation(&shell).ok_or("Missing creation-owned entity settings")?;
    let outer = rename::presentation(&shell).ok_or("Missing entity dialog")?;
    if !outer.text_creation
        || outer.serial != p.stamp.serial
        || outer.field_choice != Some(p.stamp.choice)
    {
        return Err("Entity settings do not own current field".into());
    }
    Ok(
        json!({"serial":p.stamp.serial,"visit":p.stamp.visit,"selection":p.stamp.selection,"choice":p.stamp.choice.revision,"epoch":p.stamp.epoch,"enabled":p.enabled&&outer.stage==Stage::Editing,"wire":p.options.wire(),"selected":p.selected.iter().map(|c|json!([c.id,c.label])).collect::<Vec<_>>(),"directory":{"query":p.directory.query,"loading":p.directory.loading,"error":p.directory.error,"candidates":p.directory.candidates.iter().map(|c|json!([c.id,c.label])).collect::<Vec<_>>()}}),
    )
}
pub(super) async fn verify(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    expected: &Value,
) -> Result<Value, String> {
    let model = model(state)?;
    if model["wire"] != *expected
        || model["directory"]["query"] != ""
        || model["directory"]["loading"] != false
        || !model["directory"]["error"].is_null()
    {
        return Err("Entity settings raw configuration/directory mismatch".into());
    }
    let dom = wait_dom(
        d,
        &format!("window.__listEntityCreate.transition({})", model),
    )
    .await?;
    if self::model(state)? != model {
        return Err("Entity model changed during DOM observation".into());
    }
    Ok(json!({"model":model,"dom":dom}))
}
pub(super) async fn configure(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: EntityCase,
) -> Result<Value, String> {
    let ledger = rename_fixture::snapshot(&state.peek())?;
    let initial = model(state)?;
    if initial["wire"] != wire(case.channel(), false, true, &[], Some(false))
        || initial["enabled"] != true
    {
        return Err("Entity did not begin at exact default Null".into());
    }
    let mut steps = vec![];
    for step in plan(case) {
        let before = model(state)?;
        let action = checked(
            d,
            &format!(
                "window.__listEntityCreate.action({},{})",
                step.action, before
            ),
        )
        .await?;
        let mut changed = None;
        for _ in 0..150 {
            let current = model(state)?;
            if current["wire"] == step.wire && current["epoch"] != before["epoch"] {
                if ["serial", "visit", "selection", "choice"]
                    .into_iter()
                    .any(|k| current[k] != before[k])
                {
                    return Err("Settings transition changed field owner".into());
                }
                changed = Some(current);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
        let changed = changed.ok_or("Native entity control did not commit exact transition")?;
        let verified = verify(state, d, &step.wire).await?;
        if rename_fixture::snapshot(&state.peek())? != ledger {
            return Err("Configuration issued a request".into());
        }
        steps.push(json!({"before":before,"action":action,"after":changed,"verified":verified}));
    }
    let final_state = verify(state, d, &case.options()).await?;
    Ok(
        json!({"initial":initial,"steps":steps,"final":final_state,"ledgerBefore":ledger,"ledgerAfter":rename_fixture::snapshot(&state.peek())?}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::slack_lists::{EntityCreateDomain, EntityCreateOptions};
    #[test]
    fn literal_plans_cover_retention_null_omitted_and_exact_final_options() {
        for case in EntityCase::ALL {
            let steps = plan(case);
            assert_eq!(steps.last().unwrap().wire, case.options());
            let domain = if case.channel() {
                EntityCreateDomain::Channel
            } else {
                EntityCreateDomain::People
            };
            for step in &steps {
                assert_eq!(
                    EntityCreateOptions::from_wire(domain, &step.wire)
                        .unwrap()
                        .wire(),
                    step.wire
                );
            }
            assert!(steps.iter().any(|s| s.wire["format"] == "single_entity"
                && s.wire["default_value"] == case.ids().join("|")));
            assert!(
                steps
                    .iter()
                    .any(|s| s.wire.get("default_value") == Some(&Value::Null))
            );
            assert!(steps.iter().any(|s| s.wire.get("default_value").is_none()));
            assert!(
                steps
                    .iter()
                    .all(|s| s.wire["show_member_name"] == case.channel())
            );
        }
    }
}
