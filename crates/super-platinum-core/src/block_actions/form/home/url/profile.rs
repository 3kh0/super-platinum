use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Data incarnation only. The renderer owns the original form/visit admission.
#[derive(Clone, Debug)]
pub(in super::super) struct Profile {
    pub(in super::super) identity: Arc<()>,
    keys: BTreeSet<InputKey>,
    // Native domain verdicts are committable parent data, never buffer markers.
    verdicts: BTreeMap<InputKey, (String, bool)>,
}
impl PartialEq for Profile {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity) && self.keys == other.keys
    }
}
impl Profile {
    pub(in super::super) fn all_keys(&self) -> impl Iterator<Item = &InputKey> {
        self.keys.iter()
    }
    pub(in super::super) fn contains(&self, key: &InputKey) -> bool {
        self.keys.contains(key)
    }
    pub(in super::super) fn store_verdict(&mut self, key: InputKey, literal: String, valid: bool) {
        self.verdicts.insert(key, (literal, valid));
    }
    pub(in super::super) fn error(&self, key: &InputKey, value: &InputValue) -> Option<String> {
        let InputValue::Url(Some(literal)) = value else {
            return None;
        };
        self.verdicts
            .get(key)
            .filter(|(seen, valid)| seen == literal && !valid)
            .map(|_| "Enter a valid URL.".to_owned())
    }
    fn capture(view: &Value) -> Result<Self, FormError> {
        if view.get("raw_state").is_some() {
            return Err(invalid());
        }
        let mut keys = BTreeSet::new();
        for block in view["blocks"].as_array().ok_or_else(invalid)? {
            let element = &block["element"];
            if element["type"] != "url_text_input" {
                continue;
            }
            parse(block, element)?;
            let key = InputKey {
                block_id: id(&block["block_id"])?,
                action_id: id(&element["action_id"])?,
            };
            if view["state"]["values"]
                .get(&key.block_id)
                .and_then(|v| v.get(&key.action_id))
                .is_some()
                || !keys.insert(key)
            {
                return Err(invalid());
            }
        }
        Ok(Self {
            identity: Arc::new(()),
            keys,
            verdicts: BTreeMap::new(),
        })
    }
}
impl HomeForm {
    /// A named data profile for the renderer's actual first form/owned Reset.
    /// This is not original-owner, native event or app-action authority.
    pub fn parse_first_owned_urls(view: &Value) -> Result<Self, FormError> {
        let Ok(profile) = Profile::capture(view) else {
            return Self::parse(view);
        };
        if profile.keys.is_empty() {
            return Self::parse(view);
        }
        Self::parse_profile(view, Some(profile))
    }
    pub fn has_owned_urls(&self) -> bool {
        self.url_profile.is_some()
    }
    pub fn owned_url_view(&self, raw: &Value) -> bool {
        self.url_profile.is_some()
            && self.raw_view() == raw
            && self.url_state_inventory_valid()
            && self.state().is_ok()
    }
    pub(in super::super) fn continuation_url_profile(
        &self,
        view: &Value,
    ) -> Result<Option<Profile>, FormError> {
        let Some(profile) = &self.url_profile else {
            return Ok(None);
        };
        let next = Profile::capture(view)?;
        if profile.keys != next.keys {
            return Err(FormError::ConflictingUpdate);
        }
        Ok(Some(profile.clone()))
    }
}
