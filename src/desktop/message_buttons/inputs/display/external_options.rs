//! JQ84's outer ordinary trigger ignores saved arrays; a new picker uses them.
use super::*;
use super_platinum_core::block_actions::external::ExternalOption;

impl Mounted {
    pub(in crate::message_buttons::inputs) fn sync_external_options(
        &self,
        owner: ContainerKey,
        key: InputKey,
        field: &FormField,
    ) -> Vec<ExternalOption> {
        let mut state = self.0.lock().unwrap();
        if state.unmounted {
            return vec![];
        }
        let binding = Binding {
            owner,
            key,
            external: true,
            radio: false,
            date: false,
            time: false,
            user: false,
            users: false,
            channels: false,
            conversations: false,
            conversation: false,
            channel: false,
            multi: Some(false),
        };
        if state.binding.as_ref() != Some(&binding)
            || state.raw.get("initial_options") != field.raw.get("initial_options")
        {
            state.selected_options = field
                .raw
                .get("initial_options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            state.binding = Some(binding);
            state.revision = next_id();
        }
        state.raw = field.raw.clone();
        state
            .selected_options
            .iter()
            .filter_map(|o| ExternalOption::parse(o).ok())
            .collect()
    }
}
impl Action {
    /// Staging/rollback changes only the outer preview; it must not retire this
    /// picker ticket or publish shared wire state. Final commit retires it.
    pub(in crate::message_buttons::inputs) fn preview_external_options(
        &self,
        selected: &[Value],
    ) -> bool {
        let Some(mounted) = self.mounted.upgrade() else {
            return false;
        };
        let mut state = mounted.lock().unwrap();
        if state.unmounted || state.revision != self.revision {
            return false;
        }
        state.selected_options = selected.to_vec();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn outer_initial_changes_reset_removal_null_and_empty_while_old_visit_cannot_rollback() {
        let key = InputKey {
            block_id: "b".into(),
            action_id: "a".into(),
        };
        let owner = ContainerKey {
            epoch: 1,
            user: "U1".into(),
            team: "T1".into(),
            channel: "C1".into(),
            ts: "1.000001".into(),
        };
        let option = json!({"text":{"type":"plain_text","text":"Initial"},"value":"initial"});
        let raw =
            json!({"type":"multi_external_select","action_id":"a","initial_options":[option]});
        let parse = |value: &Value| {
            super_platinum_core::block_actions::form::parse_message_external_options(value).unwrap()
        };
        for replacement in [
            None,
            Some(Value::Null),
            Some(json!([])),
            Some(json!([{"text":{"type":"plain_text","text":"New"},"value":"new"}])),
        ] {
            let mount = Mounted::new();
            assert_eq!(
                mount
                    .sync_external_options(owner.clone(), key.clone(), &parse(&raw))
                    .len(),
                1
            );
            let action = mount.capture(&owner, &key).unwrap();
            assert!(action.preview_external_options(&[]));
            assert!(
                mount
                    .sync_external_options(owner.clone(), key.clone(), &parse(&raw))
                    .is_empty()
            );
            let mut changed = raw.clone();
            if let Some(value) = replacement.clone() {
                changed["initial_options"] = value;
            } else {
                changed.as_object_mut().unwrap().remove("initial_options");
            }
            let expected = changed["initial_options"].as_array().map_or(0, Vec::len);
            assert_eq!(
                mount
                    .sync_external_options(owner.clone(), key.clone(), &parse(&changed))
                    .len(),
                expected
            );
            assert!(!action.preview_external_options(&[option.clone()]));
        }
    }
}
