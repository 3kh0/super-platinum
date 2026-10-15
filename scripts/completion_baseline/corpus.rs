//! Deterministic synthetic workload, not a sampled Slack population.
use super_platinum_core::{
    composer_completion::{Kind, Trigger},
    config::WorkspaceSession,
    slack::models::{Channel, User, UserGroup, UserProfile},
    state::Workspace,
};

pub const VERSION: &str = "completion-directory-v1";
pub const SIZES: [usize; 3] = [2_000, 10_000, 64_000];
pub const QUERIES: [&str; 24] = [
    "@",
    "@a",
    "@al",
    "@alex",
    "@alex.000001",
    "@kim",
    "@morgan",
    "@nguyen",
    "@engineering",
    "@here",
    "@everyone",
    "@zzzzzzmissing",
    "@josé",
    "@田中",
    "#",
    "#p",
    "#proj",
    "#project",
    "#project-000001",
    "#platform",
    "#support",
    "#esign",
    "#zzzzzzmissing",
    "#日本語",
];

pub fn trigger(text: &str) -> Trigger {
    Trigger::at(text, text.len(), text.len()).expect("benchmark query trigger")
}

pub fn workspace(total: usize) -> Workspace {
    assert!(SIZES.contains(&total));
    let mut ws = Workspace::from_session(&WorkspaceSession {
        team_id: "TPERF".into(),
        user_id: "UPERFSELF".into(),
        name: "Synthetic directory".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
        enterprise_id: None,
    });
    populate(&mut ws, total);
    ws
}

/// Replaces only directory maps. Call before fixture projection; no live callers.
pub fn populate(ws: &mut Workspace, total: usize) {
    assert!(SIZES.contains(&total));
    ws.users.clear();
    ws.channels.clear();
    ws.usergroups.clear();
    let users = total * 7 / 8;
    let groups = total / 160;
    let channels = total - users - groups;
    let first = [
        "Alex",
        "Morgan",
        "Sam",
        "Maya",
        "Jordan",
        "Taylor",
        "Avery",
        "Robin",
        "José",
        "Chloé",
        "田中",
        "İpek",
        "Олена",
        "Ravi",
        "Nadia",
        "Kim",
    ];
    let last = [
        "Kim", "Patel", "Chen", "Nguyen", "Smith", "Garcia", "Lee", "Wilson", "Brown", "Singh",
        "Rivera", "Müller", "Martin", "Walker", "Clark", "Young",
    ];
    for i in 0..users {
        let given = first[(i * 13) % first.len()];
        let family = last[(i / 17) % last.len()];
        let handle = if i == 1 {
            "alex.000001".into()
        } else {
            format!("{}.{i:06}", given.to_lowercase())
        };
        let id = format!("UPERF{i:06}");
        let display = format!("{given} {family} {i:06}");
        ws.users.insert(
            id.clone(),
            User {
                id,
                name: Some(handle),
                real_name: Some(display.clone()),
                deleted: i % 20 == 0,
                is_bot: i % 11 == 0,
                profile: Some(UserProfile {
                    display_name: (i % 8 != 0).then_some(display),
                    real_name: Some(format!("{given} {family}")),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
    }
    let prefixes = [
        "project",
        "team",
        "engineering",
        "support",
        "design",
        "platform",
        "social",
        "日本語",
    ];
    for i in 0..channels {
        let id = format!("CPERF{i:06}");
        let name = if i == 1 {
            "project-000001".into()
        } else {
            format!("{}-{i:06}", prefixes[i % prefixes.len()])
        };
        ws.channels.insert(
            id.clone(),
            Channel {
                id,
                name: Some(name),
                is_archived: i % 10 == 0,
                is_im: i % 10 == 2,
                is_mpim: i % 20 == 3,
                is_private: i % 5 == 1,
                ..Default::default()
            },
        );
    }
    for i in 0..groups {
        let id = format!("SPERF{i:06}");
        ws.usergroups.insert(
            id.clone(),
            UserGroup {
                id,
                team_id: ws.team_id.clone(),
                handle: format!("engineering-{i:04}"),
                name: format!("Engineering on call {i:04}"),
                date_delete: u64::from(i % 20 == 0),
                ..Default::default()
            },
        );
    }
    assert_eq!(
        ws.users.len() + ws.channels.len() + ws.usergroups.len(),
        total
    );
}

pub fn counts(ws: &Workspace) -> serde_json::Value {
    serde_json::json!({
        "users":ws.users.len(), "deleted_users":ws.users.values().filter(|u|u.deleted).count(),
        "bots":ws.users.values().filter(|u|u.is_bot).count(),
        "channels":ws.channels.len(), "eligible_channels":ws.channels.values().filter(|c|!c.is_archived && !c.is_im && !c.is_mpim).count(),
        "groups":ws.usergroups.len(), "eligible_groups":ws.usergroups.values().filter(|g|g.date_delete==0).count(),
    })
}

pub fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Person => "person",
        Kind::Channel => "channel",
    }
}
