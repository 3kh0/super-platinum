use super::*;
use std::collections::HashSet;
/// Request-bound, immutable canonical sections; contains private profile data,
/// therefore deliberately has no Debug implementation.
#[derive(Clone)]
pub struct OwnProfileSections {
    team: String,
    user: String,
    raw: Value,
    sections: Vec<Value>,
}
impl OwnProfileSections {
    pub fn from_response(w: &WorkspaceSession, response: &Value) -> Result<Self, Error> {
        if response["ok"] == false
            && let Some(code) = response["error"].as_str().filter(|s| !s.is_empty())
        {
            return Err(Error::Api(code.into()));
        }
        if response["ok"] != true || response.get("error").is_some_and(|v| !v.is_null()) {
            return Err(Error::Transport(
                "Slack did not confirm the profile sections read.".into(),
            ));
        }
        // XpEt uses unchanged API response.result.data.user.profileSections.
        // Live read-only production probe (2026-10-13) confirmed this envelope,
        // self user identity, Header title and its UI permission. No write proof
        // is implied by that read; extra user keys include isSelf/team/__typename.
        let user = &response["result"]["data"]["user"];
        if required(user, "id")? != w.user_id {
            return Err(invalid("The profile sections belong to another person."));
        }
        let sections = user["profileSections"]
            .as_array()
            .ok_or_else(|| invalid("Slack did not return profile sections."))?
            .clone();
        let mut ids = HashSet::new();
        for section in &sections {
            if !section.is_object() || !ids.insert(required(section, "sectionId")?) {
                return Err(invalid("Slack returned ambiguous profile sections."));
            }
        }
        Ok(Self {
            team: w.team_id.clone(),
            user: w.user_id.clone(),
            raw: response.clone(),
            sections,
        })
    }
    pub fn team_id(&self) -> &str {
        &self.team
    }
    pub fn user_id(&self) -> &str {
        &self.user
    }
    pub fn sections(&self) -> &[Value] {
        &self.sections
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn title(&self) -> Result<&str, Error> {
        text(self.title_element()?.1, "text")
    }
    pub fn title_capability(&self) -> ProfileEditCapability {
        let Ok((section, target)) = self.title_element() else {
            return ProfileEditCapability::Unknown(
                "Slack did not provide an unambiguous title field.",
            );
        };
        match target["permissions"]["ui"].as_bool() {
            Some(false) => {
                return ProfileEditCapability::Denied("Your organization manages your title.");
            }
            None => {
                return ProfileEditCapability::Unknown("Title editing permission is unavailable.");
            }
            Some(true) => {}
        }
        if convert(section).is_err() {
            return ProfileEditCapability::Unknown(
                "This profile Header contains fields that cannot yet be preserved safely.",
            );
        }
        ProfileEditCapability::Allowed
    }
    pub(super) fn title_element(&self) -> Result<(&Value, &Value), Error> {
        let mut headers = self.sections.iter().filter(|s| s["type"] == "HEADER");
        let header = headers
            .next()
            .ok_or_else(|| invalid("The profile Header is unavailable."))?;
        if headers.next().is_some() {
            return Err(invalid("The profile Header is ambiguous."));
        }
        let elements = header["profileElements"]
            .as_array()
            .ok_or_else(|| invalid("The profile Header fields are unavailable."))?;
        let mut ids = HashSet::new();
        for element in elements {
            if !ids.insert(required(element, "elementId")?) {
                return Err(invalid(
                    "The profile Header fields have duplicate identities.",
                ));
            }
        }
        let mut titles = elements.iter().filter(|e| e["elementKey"] == "title");
        let title = titles
            .next()
            .ok_or_else(|| invalid("The title field is unavailable."))?;
        if titles.next().is_some() || title["__typename"] != "ProfileTextElement" {
            return Err(invalid("The title field has an unsupported schema."));
        }
        text(title, "text")?;
        Ok((header, title))
    }
}
pub(super) fn text<'a>(e: &'a Value, key: &str) -> Result<&'a str, Error> {
    match e.get(key) {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(s)) => Ok(s),
        _ => Err(invalid("A profile field has an unsupported value.")),
    }
}
fn array(e: &Value, key: &str) -> Result<Value, Error> {
    match e.get(key) {
        None | Some(Value::Null) => Ok(json!([])),
        Some(Value::Array(v)) => Ok(Value::Array(v.clone())),
        _ => Err(invalid("A profile field has an unsupported list value.")),
    }
}
/// Exact 16NY conversion of known fields. Known read-only siblings are copied,
/// not edited. Source deliberately omits SCIM-managed Person elements.
/// Unlike source's default:null, unknown types block instead of being discarded.
pub(super) fn convert(section: &Value) -> Result<Vec<Value>, Error> {
    let elements = section["profileElements"]
        .as_array()
        .ok_or_else(|| invalid("Missing Header fields."))?;
    let mut output = Vec::with_capacity(elements.len());
    for e in elements {
        let id = required(e, "elementId")?;
        let mut item = json!({"element_id":id});
        match required(e, "__typename")? {
            "ProfileTextElement" => item["text"] = json!({"text":text(e,"text")?}),
            "ProfileCurrentLocationElement" => {
                item["currentLocation"] = json!({
                "city":text(e,"city")?,"country":text(e,"country")?,
                "description":text(e,"description")?,"state":text(e,"state")?})
            }
            "ProfileTagsElement" => item["tags"] = json!({"tags":array(e,"tags")?}),
            "ProfileDateElement" => item["date"] = json!({"date":text(e,"date")?}),
            "ProfileLinkElement" => {
                item["link"] = json!({"displayText":text(e,"displayText")?,"uri":text(e,"uri")?})
            }
            "ProfilePersonElement" => {
                match e.get("isScimManaged") {
                    Some(Value::Bool(true)) => continue,
                    None | Some(Value::Null) | Some(Value::Bool(false)) => {}
                    _ => return Err(invalid("Profile management policy is malformed.")),
                }
                item["person"] = json!({"persons":array(e,"persons")?});
            }
            "ProfileLongTextElement" => item["long_text"] = json!({"blocks":text(e,"blocks")?}),
            "ProfileFileElement" => {
                let file = text(e, "fileId")?;
                item["file"] = if file.is_empty() {
                    Value::Null
                } else {
                    json!({"fileId":file})
                };
            }
            _ => {
                return Err(invalid(
                    "An unknown Header field cannot be preserved safely.",
                ));
            }
        }
        output.push(item);
    }
    Ok(output)
}
