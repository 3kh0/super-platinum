//! Plain opening keeps current initialization separate from inert draft custody.
use super::*;
mod emoji;
mod emoticons;
use super_platinum_core::slack_lists::comments::ordered::MessageWitness;

#[derive(Clone)]
pub(super) struct Mount {
    pub policy: delta_editor::Policy,
    preferences_revision: u64,
    emoticons: Option<Arc<emoticons::Observed>>,
    emoji: Option<Arc<emoji::Observed>>,
}
impl Mount {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.preferences_revision == crate::app_home::autocomplete::preferences_revision(shell)
            && self.emoticons.as_ref().is_none_or(|e| e.current(shell))
            && self.emoji.as_ref().is_none_or(|e| e.current(shell))
    }

    /// The actual initialized or durable restored Delta owns the reached
    /// appearance set. The inert journal is never catalog/locale authority.
    pub(super) fn observe_content(&mut self, delta: &PlainDelta) -> Result<(), Error> {
        if let Some(facts) = &self.emoji {
            for op in delta.ops() {
                let Some(value) = op.get("insert").and_then(|v| v.get("slackemoji")) else {
                    continue;
                };
                let text = value
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(unavailable)?;
                let appearance = facts.live.appearance(text).ok_or_else(unavailable)?;
                if let Some(old) = self.policy.emoji.iter().find(|r| r.name == text) {
                    if old != &appearance {
                        return Err(unavailable());
                    }
                } else {
                    self.policy.emoji.push(appearance);
                }
            }
        }
        Ok(())
    }
}

pub(super) fn original(page: &Page, ts: &str) -> Result<MessageWitness, Error> {
    let response = page.ordered_response().ok_or_else(unavailable)?;
    let rows = response.response()["messages"]
        .as_array()
        .ok_or_else(unavailable)?;
    let mut matching = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row["ts"].as_str() == Some(ts));
    let (index, _) = matching.next().ok_or_else(unavailable)?;
    if matching.next().is_some() {
        return Err(unavailable());
    }
    response.message(index).map_err(|_| unavailable())
}

pub(super) fn mount_policy(
    shell: &ShellState,
    observed: &own_edit_policy::Observation,
) -> Result<Mount, Error> {
    if !observed.current(shell) {
        return Err(unavailable());
    }
    Ok(Mount {
        preferences_revision: crate::app_home::autocomplete::preferences_revision(shell),
        emoticons: None,
        emoji: None,
        policy: native_policy(&observed.proof())?,
    })
}

// Rendering-policy construction is separate from the original observation gate
// above so admission tests use the same complete-carrier decoding as mounting.
fn native_policy(proof: &Value) -> Result<delta_editor::Policy, Error> {
    let policy =
        plain_observations::native_policy(&proof["userPreferences"], &proof["assignments"])?;
    // These native modules are not implemented yet. Refuse the actual enabled
    // policy before installing an editor that cannot mount it.
    if !policy.plain_text_mode || policy.markdown_mode || policy.supports_portable_content {
        return Err(Error::Api("list_plain_native_policy_unavailable".into()));
    }
    // O_Q_ plain formats exclude link; its queued linkifier also refuses plain.
    // Retain the observed assignment without blocking this inert native branch.
    Ok(delta_editor::Policy {
        slugs_enabled: policy.slugs_enabled,
        mac: cfg!(target_os = "macos"),
        enter_creates_newline: policy.enter_creates_newline,
        use_pre_newlines: policy.use_pre_newlines,
        spellcheck: policy.spellcheck,
        underline_links: policy.underline_links,
        convert_emoticons: policy.convert_emoticons,
        emoji_as_text: policy.emoji_as_text,
        composer_smart_symbols: policy.composer_smart_symbols,
        disable_smart_symbols: policy.disable_smart_symbols,
        detect_go_links: policy.detect_go_links,
        flatten_tree_list_before_paste: policy.flatten_tree_list_before_paste,
        supports_portable_content: policy.supports_portable_content,
        markdown_mode: policy.markdown_mode,
        // Initial emoji becomes text. acquire_mount fills the finite
        // keyboard appearances when conversion is enabled; semantic paste
        // still acquires its own independent appearance observations.
        emoji: Vec::new(),
        live_emoji: None,
    })
}

pub(super) async fn acquire_mount(
    state: Signal<ShellState>,
    source: &Source,
    serial: u64,
    observed: &own_edit_policy::Observation,
) -> Result<Mount, Error> {
    let mut mount = mount_policy(&state.peek(), observed)?;
    if !mount.policy.emoji_as_text {
        let facts = emoji::acquire(state, source, serial, observed).await?;
        // Keyboard insertion uses its original base names; the selected live
        // catalog also supplies every reached initial/restored appearance.
        if mount.policy.convert_emoticons {
            let names: std::collections::BTreeMap<String, String> =
                serde_json::from_str(delta_editor::EMOTICON_TABLE).map_err(|_| unavailable())?;
            for name in names
                .into_values()
                .collect::<std::collections::BTreeSet<_>>()
            {
                mount.policy.emoji.push(
                    facts
                        .live
                        .appearance(&format!(":{name}:"))
                        .ok_or_else(unavailable)?,
                );
            }
        }
        mount.policy.live_emoji = Some(facts.live.clone());
        mount.emoji = Some(Arc::new(facts));
    } else if mount.policy.convert_emoticons {
        let facts = emoticons::acquire(state, source, serial, observed).await?;
        mount.policy.emoji = facts.appearances.clone();
        mount.emoticons = Some(Arc::new(facts));
    }
    if !source.current(&state.peek())
        || !source.owner.reading(&state.peek(), serial)
        || !observed.current(&state.peek())
        || !mount.current(&state.peek())
    {
        return Err(unavailable());
    }
    Ok(mount)
}

pub(super) async fn initial(
    state: Signal<ShellState>,
    source: &Source,
    serial: u64,
    loaded: &network::Loaded,
    policy: &own_edit_policy::Observation,
) -> Result<(OwnEditSeed, Mount), Error> {
    let mut mount = acquire_mount(state, source, serial, policy).await?;
    let original = original(&loaded.page, &source.ts)?;
    let captured = plain_observations::capture(state, source, serial, policy, &original).await?;
    let shell = state.peek();
    if !captured.current(&shell) || !mount.current(&shell) || !policy.current(&shell) {
        return Err(unavailable());
    }
    let known = loaded
        .page
        .messages()
        .iter()
        .find(|row| row.ts.as_deref() == Some(source.ts.as_str()))
        .ok_or_else(unavailable)?;
    let (_, team, user) = source.owner.scope.stable_owner();
    let seed = OwnEditSeed::capture_plain(
        &team,
        &user,
        &loaded.page,
        known,
        &captured.seed,
        policy.policy(),
        run::millis(),
    )
    .map_err(|e| Error::Api(e.to_string()))?;
    mount.observe_content(seed.plain_delta().ok_or_else(unavailable)?)?;
    Ok((seed, mount))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proof(group: &str, disabled: Value) -> Value {
        serde_json::json!({
            "userPreferences":{"ok":true,"prefs":{
                "plain_text_mode":true,"emoji_mode":"as_text","disable_smart_symbols":disabled}},
            "assignments":{"ok":true,"assignments":{"composer_smart_symbols":{"group":group}}}
        })
    }
    #[test]
    fn smart_symbol_mount_preserves_actual_on_off_and_disabled_policy() {
        for (group, disabled) in [("on", false), ("off", false), ("on", true)] {
            let value = native_policy(&proof(group, Value::Bool(disabled))).unwrap();
            assert_eq!(value.composer_smart_symbols, group == "on");
            assert_eq!(value.disable_smart_symbols, disabled);
            assert!(value.emoji.is_empty());
        }
        // Original JavaScript truthiness, not parsing the preference as text.
        assert!(
            native_policy(&proof("on", serde_json::json!("false")))
                .unwrap()
                .disable_smart_symbols
        );
    }
    #[test]
    fn smart_symbol_mount_does_not_relax_other_modes_or_invent_carriers() {
        for pref in ["plain_text_mode"] {
            let mut raw = proof("on", Value::Bool(false));
            raw["userPreferences"]["prefs"]
                .as_object_mut()
                .unwrap()
                .remove(pref);
            assert!(native_policy(&raw).is_err());
        }
        let mut raw = proof("on", Value::Bool(false));
        raw["userPreferences"]["prefs"]["emoji_mode"] = serde_json::json!("native");
        let policy = native_policy(&raw).unwrap();
        assert!(!policy.emoji_as_text);
        // Decoding policy grants no mount; acquire_mount must supply the full
        // current display observation before the caller can publish a session.
        assert!(policy.live_emoji.is_none());
        for key in ["userPreferences", "assignments"] {
            let mut raw = proof("on", Value::Bool(false));
            raw[key] = Value::Null;
            assert!(native_policy(&raw).is_err());
        }
    }
    #[test]
    fn go_link_mount_retains_exact_assignment_and_other_admission_guards() {
        for (group, enabled) in [
            (serde_json::json!("on"), true),
            (serde_json::json!("off"), false),
            (serde_json::json!(true), false),
            (Value::Null, false),
        ] {
            let mut raw = proof("off", Value::Bool(false));
            raw["assignments"]["assignments"]["detect_go_links"] =
                serde_json::json!({"group":group});
            assert_eq!(native_policy(&raw).unwrap().detect_go_links, enabled);
            raw["userPreferences"]["prefs"]["plain_text_mode"] = Value::Bool(false);
            assert!(native_policy(&raw).is_err());
        }
        let mut raw = proof("off", Value::Bool(false));
        raw["assignments"]["assignments"]["detect_go_links"] = serde_json::json!({"group":"on"});
        raw["assignments"]["ok"] = Value::Bool(false);
        assert!(native_policy(&raw).is_err());
    }
}
