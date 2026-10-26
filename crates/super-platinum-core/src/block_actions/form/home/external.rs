//! Home single and multiple external dispatch share strict suggestions admission.
use super::*;
mod dispatch;
pub use dispatch::ExternalParent;

pub(super) fn supported(block: &Value, element: &Value) -> bool {
    block["type"] == "input"
        && block
            .get("dispatch_action")
            .is_none_or(|value| value.is_boolean())
        && ["confirm", "dispatch_action_config", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
        && match element["type"].as_str() {
            Some("external_select") => parse_message_external(element).is_ok(),
            Some("multi_external_select") => parse_message_external_options(element).is_ok(),
            _ => false,
        }
}

impl HomeForm {
    /// Shape admission only. The renderer must retain exact Home/query/result
    /// ownership before accepting a remotely returned option.
    pub fn external_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
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
        if !matches!(
            field.kind,
            InputKind::ExternalSelect { .. } | InputKind::MultiExternalSelect { .. }
        ) || field.raw != block["element"]
            || field.raw["action_id"] != key.action_id
            || !supported(block, &field.raw)
        {
            return Err(invalid());
        }
        Ok(field)
    }
}

#[cfg(test)]
mod tests;
