//! Home user-selection shape admission. Member source and picker ownership
//! remain renderer-owned; no modal submit or directory capability is created.
use super::*;
mod dispatch;
pub use dispatch::UserParent;

pub(super) fn supported(block: &Value, element: &Value) -> bool {
    block["type"] == "input"
        && ["confirm", "dispatch_action_config", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
        && match element["type"].as_str() {
            Some("users_select") => parse_message_user(element).is_ok(),
            Some("multi_users_select") => parse_message_users(element).is_ok(),
            _ => false,
        }
}

impl HomeForm {
    /// Validates the exact retained Home field. A caller must separately prove
    /// the current account/view/input lease and selected member eligibility.
    pub fn user_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
        if !self.unsupported.is_empty() {
            return Err(FormError::Unsupported(self.unsupported.clone()));
        }
        let field = self.fields.get(key).ok_or(FormError::UnknownField)?;
        let block = self.view["blocks"]
            .as_array()
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|block| block["block_id"] == key.block_id)
            })
            .ok_or_else(invalid)?;
        if !matches!(field.kind, InputKind::User | InputKind::MultiUser { .. })
            || field.raw != block["element"]
            || field.raw["action_id"] != key.action_id
            || !supported(block, &field.raw)
            || field.optional != block["optional"].as_bool().unwrap_or(false)
            || parse::input(&field.raw, true, None, None)?.0 != field.kind
        {
            return Err(invalid());
        }
        Ok(field)
    }
}

#[cfg(test)]
mod tests;
