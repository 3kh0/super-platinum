use super::*;
use crate::block_actions::external::ExternalOption;

#[derive(Clone)]
pub struct CapturedMessageExternal(CapturedElement);
pub struct CurrentMessageExternal<'a>(&'a CapturedElement);
impl CapturedMessageExternal {
    pub fn validate_element<'a>(
        owner: &MessageActionOwner,
        message: &'a Message,
        block_id: &str,
        action_id: &str,
    ) -> Result<&'a Value, MessageActionError> {
        let element =
            CapturedElement::validate(owner, message, block_id, action_id, "external_select")?;
        form::parse_message_external(element).map_err(|_| MessageActionError::MalformedButton)?;
        Ok(element)
    }
    pub fn capture(
        owner: MessageActionOwner,
        message: &Message,
        block_id: &str,
        action_id: &str,
        selected: Option<&ExternalOption>,
    ) -> Result<Self, MessageActionError> {
        let element = Self::validate_element(&owner, message, block_id, action_id)?;
        let field = form::parse_message_external(element)
            .map_err(|_| MessageActionError::MalformedButton)?;
        let mut action = form::validate_message_external(&field, selected)
            .map_err(|_| MessageActionError::MalformedButton)?;
        action["block_id"] = json!(block_id);
        action["action_id"] = json!(action_id);
        for key in ["initial_option", "placeholder"] {
            if let Some(value) = element.get(key).filter(|v| !v.is_null()) {
                action[key] = value.clone();
            }
        }
        Ok(Self(CapturedElement::capture(
            owner, message, block_id, action_id, action, element, None,
        )))
    }
    pub fn selected_option(&self) -> Option<&Value> {
        self.0
            .action
            .get("selected_option")
            .filter(|v| !v.is_null())
    }
    pub fn confirmation(&self) -> Option<&Value> {
        self.0.action.get("confirm")
    }
    pub fn revalidate<'a>(
        &'a self,
        owner: &MessageActionOwner,
        message: &Message,
    ) -> Result<CurrentMessageExternal<'a>, MessageActionError> {
        self.0.revalidate(owner, message, "external_select")?;
        Ok(CurrentMessageExternal(&self.0))
    }
}
current_accessors!(CurrentMessageExternal);
