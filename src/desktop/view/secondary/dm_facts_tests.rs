//! Actual generated DM panel facts, not native layout, keyboard or live proof.
use super::*;
use dioxus::core::{AttributeValue, Mutation};
use std::{cell::Cell, rc::Rc};
#[test]
fn generated_dm_panel_shows_cold_bag_badge_and_filters_the_current_group_name() {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    for alias in ["dms-current-facts", "dms-current-facts-narrow"] {
        let slot = Slot::default();
        let mut dom = VirtualDom::new_with_props(
            |(slot, alias): (Slot, &'static str)| {
                let state = use_signal(|| {
                    ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias)
                });
                slot.set(Some(state));
                dm_list_panel(state, &state.read())
            },
            (slot.clone(), alias),
        );
        let edits = dom.rebuild_to_vec();
        let mut state = slot.get().unwrap();
        assert!(state.peek().dm_unread_only);
        assert!(edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"data-dm-id",value:AttributeValue::Text(v),..}if v=="D_UNREAD_FACTS")));
        assert!(
            edits
                .edits
                .iter()
                .any(|m| matches!(m,Mutation::CreateTextNode{value,..}if value=="3"))
        );
        assert!(!edits.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"data-dm-id",value:AttributeValue::Text(v),..}if v=="G_NAME_FACTS")));
        {
            let mut s = state.write();
            s.set_dm_unread_only(false);
            s.set_dm_query("release planning".into());
        }
        let changed = dom.render_immediate_to_vec();
        assert!(changed.edits.iter().any(|m|matches!(m,Mutation::SetAttribute{name:"data-dm-id",value:AttributeValue::Text(v),..}if v=="G_NAME_FACTS")));
        assert!(
            changed.edits.iter().any(
                |m| matches!(m,Mutation::CreateTextNode{value,..}if value=="Release Planning")
            )
        );
    }
}
