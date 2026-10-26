//! Home single and multiple conversation dispatch admission. Directory observations and account/view
//! leases remain caller-owned; syntax does not prove conversation availability.
use super::*;
mod dispatch;
pub use dispatch::ConversationParent;

pub(super) fn supported(block: &Value, element: &Value) -> bool {
    block["type"] == "input"
        && ["confirm", "dispatch_action_config", "visible_to_user_ids"]
            .iter()
            .all(|key| element.get(*key).is_none())
        && match element["type"].as_str() {
            Some("conversations_select") => parse_message_conversation(element).is_ok(),
            Some("multi_conversations_select") => parse_message_conversations(element).is_ok(),
            _ => false,
        }
}

impl HomeForm {
    /// Exact retained single or multiple Home field. This creates no directory, modal-submit,
    /// response-URL or developer-action authority.
    pub fn conversation_field(&self, key: &InputKey) -> Result<&FormField, FormError> {
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
            InputKind::Conversation | InputKind::MultiConversation { .. }
        ) || field.raw != block["element"]
            || field.raw["action_id"] != key.action_id
            || !supported(block, &field.raw)
            || field.optional != block["optional"].as_bool().unwrap_or(false)
            || match field.kind {
                InputKind::MultiConversation { .. } => {
                    parse_message_conversations(&field.raw)?.kind != field.kind
                }
                _ => parse::input(&field.raw, true, None, None)?.0 != field.kind,
            }
        {
            return Err(invalid());
        }
        Ok(field)
    }
}

#[cfg(test)]
mod tests;
