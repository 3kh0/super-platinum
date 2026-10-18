//! Named static surfaces: no read, preference write, room or event dispatch.
use super::*;
pub(super) fn install(s: &mut ShellState, variant: &str) {
    super::super::fixture::install(s);
    let d = s.message_buttons.shortcuts.dialog.as_ref().unwrap();
    let mut c = Calendar::new(
        cal::Provider::Google,
        d.owner.channel.clone(),
        vec![Person {
            id: "U2".into(),
            label: "Équipe 東京".into(),
        }],
        vec![cal::Choice {
            id: d.owner.channel.clone(),
            name: "release-planning".into(),
        }],
    );
    c.fixture = true;
    c.stage = Stage::Editing;
    c.account = Some(if variant.contains("modern") {
        cal::Account::Modern
    } else {
        cal::Account::Legacy {
            user: "calendar-fixture".into(),
        }
    });
    c.calendars = vec![cal::Choice {
        id: "calendar-2".into(),
        name: "Équipe calendar".into(),
    }];
    c.conferences = vec![cal::Choice {
        id: "A_CONF".into(),
        name: "Meeting provider".into(),
    }];
    c.date = "2026-10-18".into();
    c.custom = true;
    c.start_text = "09:15".into();
    c.end_text = "10:00".into();
    c.start = cal::time::parse(&c.date, &c.start_text);
    c.end = cal::time::parse(&c.date, &c.end_text);
    c.title = "Planning with Équipe 東京".into();
    c.description = "Review the release and coordinate the next steps.".into();
    c.share = true;
    c.notice = Some("Offline preview: calendar requests are disabled.".into());
    if variant.contains("connect") {
        c.stage = Stage::Connect;
        c.auth_url = Some("https://example.test/connect".into());
    }
    if variant.contains("uncertain") {
        c.stage = Stage::Uncertain;
        c.notice=Some("The event may have been created. Check your calendar. This request will not be repeated.".into());
    }
    s.message_buttons
        .shortcuts
        .dialog
        .as_mut()
        .unwrap()
        .calendar = Some(c);
}
