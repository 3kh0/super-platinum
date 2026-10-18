//! Actual hXqI speedbump variants and callback arguments; raw response retained.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, PartialEq, Eq)]
pub enum InputType {
    Conversation,
    Canvas,
    User,
    Text,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    pub title: String,
    pub description: String,
    pub kind: InputType,
}
#[derive(Clone, PartialEq)]
pub struct Auth {
    pub app: String,
    pub function: String,
    pub provider: String,
    pub label: String,
    pub tokens: Vec<AuthToken>,
    pub salesforce_org: Option<String>,
}
#[derive(Clone, PartialEq)]
pub struct AuthToken {
    pub id: String,
    pub user: String,
    pub default: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Default,
    Scopes,
    Ai,
    Inputs,
    Authentication,
    Multiple,
}
#[derive(Clone, PartialEq)]
pub struct Preflight {
    pub(super) workflow: Workflow,
    raw: Value,
    variant: Variant,
    context: Vec<Input>,
    custom: Vec<Input>,
    pub(super) auth: Vec<Auth>,
    bot_scopes: Vec<Value>,
    user_scopes: Vec<Value>,
    required_context: Vec<Value>,
    contains_ai: bool,
    show_auth: bool,
    show_scopes: bool,
    show_inputs: bool,
    pub(super) auth_confirmed: bool,
    inputs_confirmed: bool,
    values: BTreeMap<String, String>,
    custom_values: BTreeMap<String, String>,
    custom_order: Vec<String>,
}
fn flag(v: &Value, k: &str) -> Result<bool, Error> {
    v.get(k).and_then(Value::as_bool).ok_or_else(invalid)
}
fn array(v: &Value) -> Result<Vec<Value>, Error> {
    v.as_array()
        .filter(|a| a.len() <= 100)
        .cloned()
        .ok_or_else(invalid)
}
fn list(v: &Value) -> Result<Vec<Value>, Error> {
    if v.is_array() {
        array(v)
    } else {
        v.as_object()
            .filter(|o| o.len() <= 100)
            .map(|o| o.values().cloned().collect())
            .ok_or_else(invalid)
    }
}
fn label(v: &Value, k: &str) -> Result<String, Error> {
    match v.get(k) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) if s.len() <= 8192 => Ok(s.clone()),
        _ => Err(invalid()),
    }
}
fn parameter(v: &Value, context: bool) -> Result<Input, Error> {
    let name = bounded(v, "name", 255)?;
    let kind = if context {
        match name.as_str() {
            "channel_id" => InputType::Conversation,
            "canvas_id" => InputType::Canvas,
            _ => return Err(invalid()),
        }
    } else {
        match v["type"].as_str() {
            Some("slack#/types/channel_id") => InputType::Conversation,
            Some("slack#/types/user_id") => InputType::User,
            Some("string") => InputType::Text,
            _ => return Err(invalid()),
        }
    };
    Ok(Input {
        name,
        title: label(v, "title")?,
        description: label(v, "description")?,
        kind,
    })
}
fn parameters(values: Vec<Value>, context: bool) -> Result<Vec<Input>, Error> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|v| {
            let p = parameter(v, context)?;
            if !seen.insert(p.name.clone()) {
                return Err(invalid());
            }
            Ok(p)
        })
        .collect()
}
fn chosen(kind: &InputType, value: &str) -> bool {
    if value.is_empty() || value.len() > 10000 {
        return false;
    }
    match kind {
        InputType::Text => true,
        InputType::Conversation => {
            id(value) && matches!(value.as_bytes().first(), Some(b'C' | b'G' | b'D' | b'Z'))
        }
        InputType::Canvas => id(value) && value.starts_with('F'),
        InputType::User => id(value) && matches!(value.as_bytes().first(), Some(b'U' | b'W')),
    }
}
impl Preflight {
    pub fn variant(&self) -> Variant {
        self.variant
    }
    pub fn context(&self) -> &[Input] {
        &self.context
    }
    pub fn custom(&self) -> &[Input] {
        &self.custom
    }
    pub fn connections(&self) -> &[Auth] {
        &self.auth
    }
    pub fn bot_scopes(&self) -> &[Value] {
        &self.bot_scopes
    }
    pub fn user_scopes(&self) -> &[Value] {
        &self.user_scopes
    }
    pub fn required_context(&self) -> &[Value] {
        &self.required_context
    }
    pub fn contains_ai(&self) -> bool {
        self.contains_ai
    }
    pub fn show_auth(&self) -> bool {
        self.show_auth
    }
    pub fn show_scopes(&self) -> bool {
        self.show_scopes
    }
    pub fn show_inputs(&self) -> bool {
        self.show_inputs
    }
    pub(super) fn decode(workflow: Workflow, raw: Value) -> Result<Self, Error> {
        if !raw.is_object() {
            return Err(invalid());
        }
        let auth = &raw["third_party_auths"];
        let context = &raw["missing_context_parameters"];
        let custom = &raw["missing_customizable_input_parameters"];
        let scopes = &raw["scopes_acknowledgements"];
        let show_auth = flag(auth, "is_shown")?;
        let auth_confirmed = flag(auth, "is_satisfied")?;
        let show_inputs = flag(context, "is_shown")? || flag(custom, "is_shown")?;
        // Parse both booleans even when short-circuiting would skip one.
        flag(custom, "is_shown")?;
        let show_scopes = flag(scopes, "is_shown")?;
        let context = parameters(list(&context["context_parameters"])?, true)?;
        let custom = parameters(array(&custom["customizable_input_parameters"])?, false)?;
        let mut auths = Vec::new();
        let mut seen = BTreeSet::new();
        for value in array(&auth["auths"])? {
            let app = bounded(&value, "app_id", 255)?;
            let function = bounded(&value, "function_id", 255)?;
            let provider = bounded(&value, "provider_key", 255)?;
            if !id(&app) || !id(&function) || !seen.insert((function.clone(), provider.clone())) {
                return Err(invalid());
            }
            let mut tokens = Vec::new();
            let mut ids = BTreeSet::new();
            for token in array(&value["valid_tokens"])? {
                let token_id = bounded(&token, "external_token_id", 255)?;
                if !ids.insert(token_id.clone())
                    || token
                        .get("user_id")
                        .is_some_and(|u| u.as_str() != Some(&workflow.user))
                {
                    return Err(invalid());
                }
                tokens.push(AuthToken {
                    id: token_id,
                    user: label(&token, "external_user_id")?,
                    default: flag(&token, "is_default")?,
                });
            }
            let salesforce_org = match value.get("salesforce_org_id") {
                None | Some(Value::Null) => None,
                Some(_) => Some(bounded(&value, "salesforce_org_id", 255)?),
            };
            auths.push(Auth {
                app,
                function,
                provider,
                label: label(&value, "provider_name")?,
                tokens,
                salesforce_org,
            });
        }
        let contains_ai = match raw.get("contains_ai_steps") {
            None => false,
            Some(v) => v.as_bool().ok_or_else(invalid)?,
        };
        let required_context = match raw.get("required_context_parameters") {
            None => vec![],
            Some(v) => array(v)?,
        };
        let ai = contains_ai && !required_context.is_empty();
        let steps = [show_auth, show_inputs, show_scopes, ai]
            .into_iter()
            .filter(|x| *x)
            .count();
        let variant = if steps > 1 {
            Variant::Multiple
        } else if show_auth {
            Variant::Authentication
        } else if show_inputs {
            Variant::Inputs
        } else if show_scopes {
            Variant::Scopes
        } else if ai {
            Variant::Ai
        } else {
            Variant::Default
        };
        let bot_scopes = array(&scopes["bot_scopes"])?;
        let user_scopes = array(&scopes["user_scopes"])?;
        Ok(Self {
            workflow,
            raw,
            variant,
            context,
            custom,
            auth: auths,
            bot_scopes,
            user_scopes,
            required_context,
            contains_ai,
            show_auth,
            show_scopes,
            show_inputs,
            auth_confirmed,
            inputs_confirmed: !show_inputs,
            values: BTreeMap::new(),
            custom_values: BTreeMap::new(),
            custom_order: Vec::new(),
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }
    pub fn set_context(&mut self, name: &str, value: String) -> Result<(), Error> {
        let field = self
            .context
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(invalid)?;
        if !value.is_empty() && !chosen(&field.kind, &value) {
            return Err(invalid());
        }
        self.inputs_confirmed = false;
        self.values.insert(name.into(), value);
        Ok(())
    }
    pub fn set_custom(&mut self, name: &str, value: String) -> Result<(), Error> {
        let field = self
            .custom
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(invalid)?;
        if !value.is_empty() && !chosen(&field.kind, &value) {
            return Err(invalid());
        }
        if !self.custom_values.contains_key(name) {
            self.custom_order.push(name.into());
        }
        self.inputs_confirmed = false;
        self.custom_values.insert(name.into(), value);
        Ok(())
    }
    pub fn value(&self, name: &str, custom: bool) -> Option<&str> {
        if custom {
            self.custom_values.get(name)
        } else {
            self.values.get(name)
        }
        .map(String::as_str)
    }
    pub fn inputs_ready(&self) -> bool {
        self.context
            .iter()
            .all(|f| self.values.get(&f.name).is_some_and(|v| chosen(&f.kind, v)))
            && self.custom.iter().all(|f| {
                self.custom_values
                    .get(&f.name)
                    .is_some_and(|v| chosen(&f.kind, v))
            })
    }
    pub fn auth_ready(&self) -> bool {
        self.auth.iter().all(|a| !a.tokens.is_empty())
    }
    /// An explicit Done/Start after actual token updates, not a fabricated OAuth completion.
    pub fn review_connections(&mut self) -> Result<(), Error> {
        if !self.auth_ready() {
            return Err(invalid());
        }
        self.auth_confirmed = true;
        Ok(())
    }
    pub fn inputs_confirmed(&self) -> bool {
        self.inputs_confirmed
    }
    pub fn connections_confirmed(&self) -> bool {
        self.auth_confirmed
    }
    pub fn review_inputs(&mut self) -> Result<(), Error> {
        if !self.inputs_ready() {
            return Err(invalid());
        }
        self.inputs_confirmed = true;
        Ok(())
    }
    pub fn ready(&self) -> bool {
        (!self.show_inputs
            || self.inputs_ready() && (self.variant != Variant::Multiple || self.inputs_confirmed))
            && (!self.show_auth
                || self.auth_ready() && (self.variant != Variant::Multiple || self.auth_confirmed))
    }
    pub fn prepare_start(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        token: &str,
    ) -> Result<PreparedRequest, Error> {
        if !self.ready() {
            return Err(invalid());
        }
        let mut context = self.workflow.context();
        if matches!(self.variant, Variant::Inputs | Variant::Multiple) {
            let selected: serde_json::Map<String, Value> = self
                .context
                .iter()
                .filter_map(|f| self.values.get(&f.name).map(|v| (f.name.clone(), json!(v))))
                .collect();
            // Object.values orders canonical array-index keys first, then other
            // keys in insertion order. Numeric parameter names are legal here.
            let mut order: Vec<_> = self.custom_order.iter().collect();
            order.sort_by_key(|name| match name.parse::<u32>() {
                Ok(n) if n != u32::MAX && n.to_string() == name.as_str() => (0, n),
                _ => (1, 0),
            });
            let custom: Vec<Value> = order
                .into_iter()
                .filter_map(|name| {
                    self.custom_values
                        .get(name)
                        .map(|v| json!({"name":name,"value":v}))
                })
                .collect();
            context = json!({"location":"speedbump","wrapped_context":context,"speedbump_input_parameters":selected,"customizable_input_parameters":custom});
        }
        self.workflow
            .request(client, w, current, token, context, false)
    }
}
