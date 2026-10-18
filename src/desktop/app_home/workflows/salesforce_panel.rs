//! The source-selected Salesforce row uses actual org reads and the retained
//! connection controller. An org ID alone never routes generic OAuth here.
use super::*;
use salesforce::Status;
use salesforce_dispatch::Command;
use super_platinum_core::block_actions::home::workflow::{Auth, salesforce::Target};

pub(super) fn open(mut state: Signal<ShellState>, dialog: u64, function: String, provider: String) {
    {
        let mut s = state.write();
        if !dialog_current(&s, dialog) || super::busy(&s, dialog) {
            return;
        }
        let d = s.app_home.workflow.as_mut().unwrap();
        d.salesforce_observation = None;
        d.salesforce_view = Some((function, provider));
    }
    load(state, dialog);
}
fn close(mut state: Signal<ShellState>, dialog: u64) {
    let mut s = state.write();
    if !dialog_current(&s, dialog) {
        return;
    }
    // jgW6 Done closes settings; it does not assert workflow auth readiness.
    salesforce_dispatch::cancel(&mut s, Some(dialog));
    s.app_home.workflow.as_mut().unwrap().salesforce_view = None;
}

pub(super) fn load(mut state: Signal<ShellState>, dialog: u64) {
    {
        let mut s = state.write();
        if !dialog_current(&s, dialog) {
            return;
        }
        let d = s.app_home.workflow.as_ref().unwrap();
        if d.busy
            || d.salesforce_view.is_none()
            || !salesforce_observation::needs_context(&d.review)
            || cooling(&s, &d.origin.owner.ctx)
        {
            return;
        }
        s.app_home.workflow.as_mut().unwrap().busy = true;
    }
    dioxus::core::spawn_forever(async move {
        let result = salesforce_observation::read(state, dialog).await;
        let (effects, generic) = {
            let mut s = state.write();
            if !dialog_current(&s, dialog) {
                return;
            }
            s.app_home.workflow.as_mut().unwrap().busy = false;
            let Ok(observation) = result else {
                s.app_home.workflow.as_mut().unwrap().notice = Some("Connection settings could not be loaded. Try again when the connection is available.".into());
                return;
            };
            if !observation.current(&s) {
                return;
            }
            let d = s.app_home.workflow.as_ref().unwrap();
            let review = d.review.clone();
            let Some((function, provider)) = d.salesforce_view.clone() else {
                return;
            };
            let Some(auth) = review
                .connections()
                .iter()
                .find(|a| a.function == function && a.provider == provider)
            else {
                return;
            };
            let workspace = d.origin.owner.ctx.session.clone();
            let current = s.app_home.surface.as_ref().unwrap().view.clone().unwrap();
            s.app_home.workflow.as_mut().unwrap().salesforce_observation =
                Some(observation.clone());
            let mut effects = Vec::new();
            let generic = !observation.native().context().selected(auth);
            if generic {
                s.app_home.workflow.as_mut().unwrap().salesforce_view = None;
            } else {
                let Ok(target) = Target::capture(
                    &review,
                    &current,
                    &workspace,
                    &auth.function,
                    &auth.provider,
                    observation.native().context(),
                ) else {
                    s.app_home.workflow.as_mut().unwrap().notice = Some("Ask the workflow owner to reconnect the Salesforce organization used by this step.".into());
                    return;
                };
                if let Some((instance, Some(effect))) =
                    salesforce_dispatch::attach(&mut s, dialog, observation.clone(), target)
                {
                    effects.push((instance, effect));
                }
            }
            (effects, generic.then_some((function, provider)))
        };
        for (instance, effect) in effects {
            salesforce_dispatch::dispatch(state, instance, effect);
        }
        if let Some((function, provider)) = generic {
            super::connect_generic(state, dialog, function, provider, None);
        }
    });
}

pub(super) fn active_panel(state: Signal<ShellState>, dialog: u64, busy: bool) -> Option<Element> {
    let auth = {
        let s = state.read();
        let d = s
            .app_home
            .workflow
            .as_ref()
            .filter(|d| d.id == dialog && d.origin.current(&s))?;
        let (function, provider) = d.salesforce_view.as_ref()?;
        d.review
            .connections()
            .iter()
            .find(|a| &a.function == function && &a.provider == provider)?
            .clone()
    };
    row(state, dialog, &auth, busy)
}
fn row(state: Signal<ShellState>, dialog: u64, auth: &Auth, busy: bool) -> Option<Element> {
    let s = state.read();
    let d = s
        .app_home
        .workflow
        .as_ref()
        .filter(|d| d.id == dialog && d.origin.current(&s))?;
    if !salesforce_observation::possible_app(&auth.app) {
        return None;
    }
    let Some(observation) = &d.salesforce_observation else {
        return Some(rsx! {section {class:"app-home-workflow-step",
            h3 {"{auth.label}"}
            p {role:"status",if busy {"Loading connection settings…"}else{"Connection settings are unavailable."}}
            button {r#type:"button",disabled:busy,onclick:move |_|load(state,dialog),"Retry connection settings"}
            button {r#type:"button",onclick:move |_|close(state,dialog),"Done"}
        }});
    };
    if !observation.native().context().selected(auth) {
        return None;
    }
    let Some(session) = s.app_home.workflow_salesforce.iter().find(|v| {
        v.dialog == dialog
            && v.current(&s)
            && v.connection.target().function() == auth.function
            && v.connection.target().provider() == auth.provider
    }) else {
        return Some(
            rsx! {section {class:"app-home-workflow-step",h3 {"{auth.label}"}p {if auth.salesforce_org.is_none(){"Ask the workflow owner to reconnect this Salesforce organization."}else{"The earlier connection request is still being retained. Close settings and reopen them after it settles."}}button {r#type:"button",onclick:move |_|close(state,dialog),"Done"}}},
        );
    };
    let instance = session.id;
    let status = session.connection.status();
    let pending = busy || session.connection.busy();
    let uncertain = session.connection.uncertain();
    let connected = session.connection.org().is_some_and(|org| org.connected());
    let name = session
        .connection
        .org()
        .map(|org| org.name.clone())
        .unwrap_or_default();
    let username = session
        .connection
        .org()
        .map(|org| org.username.clone())
        .unwrap_or_default();
    let browser = session.browser.is_some();
    drop(s);
    Some(rsx! {section {class:"app-home-workflow-step",
        h3 {"{auth.label}"}
        if !name.is_empty() {p {"{name}"}}
        if !username.is_empty() {p {"{username}"}}
        match status {
            Status::Initial | Status::Loading => rsx!{p {role:"status","Checking Salesforce connection…"}},
            Status::MissingOrganization => rsx!{p {"Ask the workflow owner to reconnect this Salesforce organization."}},
            Status::WaitingForBrowser => rsx!{p {"Complete sign-in in your browser, then check the connection here."}},
            Status::ManualInProgress => rsx!{p {role:"status","Sign-in has not been confirmed."}},
            Status::Failed => rsx!{p {role:"status","The connection action was not confirmed."}},
            Status::Cancelled => rsx!{p {"This connection review has closed."}},
            Status::Ready => rsx!{p {if connected {"Connected"}else{"Not connected"}}},
        }
        if browser {
            button {r#type:"button",disabled:busy,onclick:move |_|salesforce_dispatch::command(state,instance,Command::BrowserReturn),"Check sign-in"}
        } else {
            button {r#type:"button",disabled:pending,onclick:move |_|salesforce_dispatch::command(state,instance,Command::Check),"Refresh connection"}
            if connected {
                button {r#type:"button",disabled:pending||uncertain,onclick:move |_|salesforce_dispatch::command(state,instance,Command::Disconnect),"Sign out"}
            } else {
                button {r#type:"button",disabled:pending||uncertain||status==Status::MissingOrganization,onclick:move |_|salesforce_dispatch::command(state,instance,Command::SignIn),"Sign in to Salesforce"}
            }
        }
        if uncertain {
            p {role:"status","The earlier action may have reached Salesforce. It will not be repeated. Refresh and review the current connection."}
            button {r#type:"button",disabled:pending||!matches!(status,Status::Ready|Status::ManualInProgress|Status::MissingOrganization),onclick:move |_|salesforce_dispatch::command(state,instance,Command::UseCurrent),"Use this connection status"}
        }
        button {r#type:"button",onclick:move |_|close(state,dialog),"Done"}
    }})
}
