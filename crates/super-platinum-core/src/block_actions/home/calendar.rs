//! Client-owned Home button interception: urb3 create and gEn+ connect.
use super::*;
use crate::block_actions::shortcuts::calendar::{self as cal, Provider};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Create {
        provider: Provider,
        preset: Option<String>,
    },
    Connect {
        app: String,
        provider: String,
        scope: Option<String>,
    },
}
#[derive(Clone, PartialEq)]
pub struct Entry {
    view: View,
    raw: Value,
    user: String,
    operation: Operation,
}
impl Entry {
    pub fn capture(
        view: &View,
        block: &str,
        action: &str,
        user: &str,
        today: &str,
    ) -> Result<Self, Error> {
        if !id(user) || block.is_empty() || action.is_empty() || block.starts_with("widget:") {
            return Err(invalid());
        }
        let mut blocks = view.raw()["blocks"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .filter(|b| b["block_id"].as_str() == Some(block));
        let block = blocks.next().ok_or_else(invalid)?;
        if blocks.next().is_some() {
            return Err(invalid());
        }
        let elements: Vec<&Value> = match block["type"].as_str() {
            Some("actions") => block["elements"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .collect(),
            Some("section") => block.get("accessory").into_iter().collect(),
            // Quiet input blocks are intercepted before either Calendar branch.
            _ => return Err(invalid()),
        };
        let mut elements = elements
            .into_iter()
            .filter(|v| v["action_id"].as_str() == Some(action));
        let raw = elements.next().ok_or_else(invalid)?;
        if elements.next().is_some() {
            return Err(invalid());
        }
        super::super::validation::validate_calendar_button(raw, user).map_err(|_| invalid())?;
        let value = raw["value"]
            .as_str()
            .filter(|v| v.len() <= 8192)
            .ok_or_else(invalid)?;
        let operation = if action == "calendar_hermes_connect" {
            let object: Value = serde_json::from_str(value).map_err(|_| invalid())?;
            Operation::Connect {
                app: text(&object, "app_id")?,
                provider: opaque(&object, "provider_key")?,
                scope: match object.get("scope_group") {
                    None | Some(Value::Null) | Some(Value::Bool(false)) => None,
                    Some(Value::String(s)) if s.is_empty() => None,
                    Some(_) => Some(opaque(&object, "scope_group")?),
                },
            }
        } else {
            let provider = [
                ("ADZ494LHY_CREATE_EVENT", Provider::Google),
                ("AFV5ECLBZ_CREATE_EVENT", Provider::Outlook),
                ("A01NK8UJRGC_CREATE_EVENT", Provider::OutlookGcc),
            ]
            .into_iter()
            .find(|(prefix, _)| value.starts_with(prefix))
            .map(|(_, p)| p)
            .ok_or_else(invalid)?;
            Operation::Create {
                provider,
                preset: preset(value.split(':').nth(1), today),
            }
        };
        Ok(Self {
            view: view.clone(),
            raw: raw.clone(),
            user: user.into(),
            operation,
        })
    }
    pub fn same_view(&self, view: &View, user: &str) -> bool {
        &self.view == view && self.user == user
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
    pub fn home(&self) -> &Home {
        &self.view.home
    }
    pub fn connect_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        if w.user_id != self.user {
            return Err(invalid());
        }
        let Operation::Connect {
            app,
            provider,
            scope,
        } = &self.operation
        else {
            return Err(invalid());
        };
        let mut fields = vec![
            ("app_id", app.clone()),
            ("provider_key", provider.clone()),
            ("_x_reason", "calendar_app_home_connect".into()),
        ];
        if let Some(scope) = scope {
            fields.push(("scope_group", scope.clone()));
        }
        Ok(client.rest_form(w, "apps.auth.external.start", fields))
    }
}
fn opaque(v: &Value, key: &str) -> Result<String, Error> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 255 && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(invalid)
}
fn preset(value: Option<&str>, today: &str) -> Option<String> {
    let today = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    let date = match value? {
        "today" => today,
        "tomorrow" => today.succ_opt()?,
        value => {
            // nL6P splits exactly three components and applies JS parseInt to
            // each before rejecting date overflow; it is not strict ISO parsing.
            let pieces: Vec<_> = value.split('-').collect();
            if pieces.len() != 3 {
                return None;
            }
            chrono::NaiveDate::from_ymd_opt(
                leading_integer(pieces[0])?,
                leading_integer(pieces[1])?.try_into().ok()?,
                leading_integer(pieces[2])?.try_into().ok()?,
            )?
            .max(today)
        }
    };
    Some(date.format("%Y-%m-%d").to_string())
}
fn leading_integer(value: &str) -> Option<i32> {
    let value = value
        .trim_start()
        .strip_prefix('+')
        .unwrap_or(value.trim_start());
    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}
pub fn authorization_url(raw: &Value) -> Result<String, Error> {
    cal::checked(raw)?;
    raw.get("authorization_url")
        .and_then(Value::as_str)
        .filter(|v| cal::external_url(v))
        .map(str::to_owned)
        .ok_or_else(invalid)
}

#[cfg(test)]
mod tests;
