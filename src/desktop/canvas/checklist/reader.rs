//! Read-only canonical hydration and exact export-section mapping. No write API.
use super::*;
use dioxus::prelude::*;
use std::collections::{BTreeMap, HashMap};
use super_platinum_core::canvas::{CanvasDocument, CanvasKind, CanvasNode};

#[derive(Default)]
pub(in crate::canvas) struct Mapping {
    pub items: BTreeMap<String, Item>,
    pub owner: Option<(u64, u64)>,
    pub exported: usize,
}
pub(in crate::canvas) struct Item {
    pub checked: bool,
    pub intent: Option<Arc<super_platinum_core::canvas::edit::CheckboxIntent>>,
    pub reason: Option<String>,
}
impl Mapping {
    /// Export IDs must identify one checklist item. Slack repeats that ID on
    /// its direct inline text carrier; no other duplicate location is accepted.
    /// Never match by label, position, or a transformed/prefix-stripped ID.
    pub fn project(document: &CanvasDocument, snapshot: Option<&CanvasCollabSnapshot>) -> Self {
        let mut ids = HashMap::<&str, usize>::new();
        let mut checklist = Vec::new();
        let mut exported = 0;
        let mut nodes: Vec<_> = document.nodes.iter().collect();
        while let Some(node) = nodes.pop() {
            let CanvasNode::Element {
                kind,
                section,
                children,
            } = node
            else {
                continue;
            };
            let is_checklist = matches!(kind, CanvasKind::Item { checked: Some(_) });
            exported += usize::from(is_checklist);
            if let Some(id) = section.as_deref() {
                *ids.entry(id).or_default() += 1;
                if is_checklist {
                    let carriers = children.iter().filter(|child| matches!(child,
                        CanvasNode::Element {kind: CanvasKind::Inline, section: Some(child_id), ..} if child_id == id
                    )).count();
                    checklist.push((id, carriers));
                }
            }
            nodes.extend(children);
        }
        let Some(snapshot) = snapshot else {
            return Self {
                items: BTreeMap::new(),
                exported,
                owner: None,
            };
        };
        let canonical: HashMap<_, _> = snapshot.sections.iter().map(|s| (s.id(), s)).collect();
        let targets: Vec<_> = checklist
            .into_iter()
            .filter_map(|(id, carriers)| {
                // The live export uses li[id] > span[id]. At most one direct
                // inline carrier is permitted; siblings/outside collisions and
                // repeated actionable items still fail the global count.
                if carriers > 1 || ids.get(id).copied() != Some(1 + carriers) {
                    return None;
                }
                let section = canonical.get(id)?;
                if section.deleted().ok()? == Some(true) {
                    return None;
                }
                Some((id, !section.checked().ok()??))
            })
            .collect();
        let intents = super_platinum_core::canvas::edit::CheckboxIntent::capture_batch(
            snapshot,
            &targets,
            super::save::now_usec().unwrap_or(u64::MAX),
        );
        let items = targets
            .into_iter()
            .zip(intents)
            .map(|((id, desired), result)| {
                let (intent, reason) = match result {
                    Ok(intent) => (Some(Arc::new(intent)), None),
                    Err(error) => (None, Some(error.to_string())),
                };
                (
                    id.to_owned(),
                    Item {
                        checked: !desired,
                        intent,
                        reason,
                    },
                )
            })
            .collect();
        Self {
            items,
            exported,
            owner: None,
        }
    }
}

pub(in crate::canvas) fn update_mapping(shell: &ShellState, reader: &mut super::super::Reader) {
    let view = presentation(shell);
    let owner = view.as_ref().map(|view| (view.visit, view.read));
    reader.checklist = match (reader.document.as_deref(), view.and_then(|v| v.snapshot)) {
        (Some(document), snapshot) => Mapping::project(document, snapshot.as_deref()),
        _ => Mapping::default(),
    };
    reader.checklist.owner = owner;
    reader.text = match (
        reader.document.as_deref(),
        presentation(shell).and_then(|v| v.snapshot),
    ) {
        (Some(document), snapshot) => {
            super::super::text::Mapping::project(shell, document, snapshot.as_deref())
        }
        _ => Default::default(),
    };
    reader.text.owner = owner;
}

pub(in crate::canvas) async fn load(
    mut state: Signal<ShellState>,
    mut reader: Signal<super::super::Reader>,
    read: Read,
) {
    let mut read = read;
    let mut retries = 1;
    loop {
        let context = read.request(&state.peek());
        let Some(ctx) = context else {
            // The event may arrive after scheduling but before this task polls.
            let next = retry_invalidated_read(&mut state.write(), &read, &mut retries);
            if let Some(next) = next {
                read = next;
                continue;
            }
            return;
        };
        let result = super_platinum_core::slack::api::fetch_canvas_collab(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            &read.file,
        )
        .await
        .map(Arc::new)
        .map_err(|error| {
            if error.is_offline() {
                "Reconnect to refresh canvas state. The current view is still here."
            } else {
                "Checklist state could not refresh. Open in Slack to check it."
            }
        });
        let successful = result.is_ok();
        let mut shell = state.write();
        if !successful {
            event_reads::block_after_error(&mut shell, &read);
        }
        if !finish_read(&mut shell, &read, result) {
            // A matching file_change can race our own ACK's readback. Coalesce
            // it into one fresh read, but respect all errors (including 429)
            // and never repeat the mutation or adopt a replacement visit.
            if successful
                && let Some(next) = retry_invalidated_read(&mut shell, &read, &mut retries)
            {
                read = next;
                drop(shell);
                continue;
            }
            return;
        }
        if let Some(attempt) = pending_attempt(&shell) {
            confirm_readback(&mut shell, &attempt, &read);
        }
        update_mapping(&shell, &mut reader.write());
        return;
    }
}

pub(in crate::canvas) fn notice(view: &Presentation) -> Option<&'static str> {
    if let Some((_, _, stage)) = view.operation.as_ref() {
        return Some(match stage {
            Stage::Preparing => "Checking this canvas change…",
            Stage::Saving => "Saving canvas change…",
            Stage::AwaitingRead if view.loading => "Change accepted. Refreshing canvas state…",
            Stage::AwaitingRead => "Change accepted. Refresh to check canvas state.",
            Stage::Uncertain => {
                "Could not confirm the canvas change. Check this canvas in Slack before editing again."
            }
        });
    }
    view.error
        .or(view.capacity_reason)
        .or_else(|| view.loading.then_some("Refreshing canvas state…"))
        .or_else(|| {
            view.stale
                .then_some("This canvas changed. Refresh to continue editing.")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::checklist::tests::fixtures;
    fn document(body: &str) -> CanvasDocument {
        CanvasDocument::parse(&format!("<div class='quip-canvas-content'><div data-section-style='7'><ul>{body}</ul></div></div>")).unwrap()
    }
    #[test]
    fn exact_unique_ids_use_canonical_boolean_instead_of_lagging_export() {
        let map = Mapping::project(
            &document("<li id='item'>Same text</li><li id='other'>Same text</li>"),
            Some(&fixtures::snapshot(true, 1000)),
        );
        assert_eq!(map.items.get("item").map(|item| item.checked), Some(true));
        assert!(!map.items.contains_key("other"));
        let unchecked = Mapping::project(
            &document("<li id='item' class='checked'>Old export</li>"),
            Some(&fixtures::snapshot(false, 1000)),
        );
        assert_eq!(
            unchecked.items.get("item").map(|item| item.checked),
            Some(false)
        );
    }
    #[test]
    fn slack_direct_inline_identity_carrier_maps_to_one_canonical_item() {
        let map = Mapping::project(
            &document("<li id='item'><span id='item'>Checklist content</span></li>"),
            Some(&fixtures::local_snapshot(false, 1000)),
        );
        assert_eq!(map.exported, 1);
        assert_eq!(map.items.len(), 1);
        let item = map.items.get("item").unwrap();
        assert!(!item.checked);
        assert!(
            item.intent.is_some(),
            "canonical permission/schema checks still pass"
        );
    }
    #[test]
    fn unavailable_canonical_state_retains_only_exported_count() {
        let map = Mapping::project(
            &document("<li>Missing identity</li><li id='item'>Item</li>"),
            None,
        );
        assert_eq!(map.exported, 2);
        assert!(map.items.is_empty());
    }
    #[test]
    fn duplicate_and_transformed_ids_never_map() {
        for body in [
            "<li id='item'>One</li><li id='item'>Two</li>",
            "<li id='section-item'>One</li>",
            "<li>One</li>",
            "<li id='item'>One</li><span id='item'>Outside</span>",
            "<li id='item'><b><span id='item'>Nested</span></b></li>",
            "<li id='item'><span id='item'>One</span><span id='item'>Two</span></li>",
            "<li id='item'><span id='item'>One</span></li><li id='item'>Two</li>",
        ] {
            assert!(
                Mapping::project(&document(body), Some(&fixtures::snapshot(true, 1000)))
                    .items
                    .is_empty()
            );
        }
    }
}
