use super::*;
use crate::block_actions::external::ExternalOption;

#[derive(Clone)]
pub struct CapturedMessageExternalOptions(CapturedElement);
pub struct CurrentMessageExternalOptions<'a>(&'a CapturedElement);
impl CapturedMessageExternalOptions {
    pub fn validate_element<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<&'a Value, MessageActionError> {
        let element = CapturedElement::validate(
            owner,
            message,
            block_id,
            action_id,
            "multi_external_select",
        )?;
        form::parse_message_external_options(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        Ok(element)
    }
    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        selected: &[ExternalOption],
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        let field = form::parse_message_external_options(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        let mut action = form::validate_message_external_options(&field, selected)
            .map_err(|_| MessageActionError::MalformedButton)?;
        action["block_id"] = json!(block_id);
        action["action_id"] = json!(action_id);
        for key in ["initial_options", "placeholder"] {
            if let Some(value) = element.get(key).filter(|v| !v.is_null()) {
                action[key] = value.clone();
            }
        }
        Ok(Self(CapturedElement::capture(
            owner, message, block_id, action_id, action, element, None,
        )))
    }
    pub fn selected_options(&self) -> &[Value] {
        self.0.action["selected_options"]
            .as_array()
            .expect("validated options")
    }
    pub fn confirmation(&self) -> Option<&Value> {
        self.0.action.get("confirm")
    }
    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageExternalOptions<'a>, MessageActionError> {
        self.0.revalidate(owner, message, "multi_external_select")?;
        Ok(CurrentMessageExternalOptions(&self.0))
    }
}
current_accessors!(CurrentMessageExternalOptions);

impl CurrentMessageExternalOptions<'_> {
    pub fn selected_options(&self) -> &[Value] {
        self.0.action["selected_options"]
            .as_array()
            .expect("validated options")
    }
}
