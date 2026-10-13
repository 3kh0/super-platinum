use super::*;

fn channel(workspace: WorkspaceRef, message: Option<&str>, thread: Option<&str>) -> Link {
    Link::Channel {
        workspace,
        channel: "C1".into(),
        message: message.map(str::to_owned),
        thread: thread.map(str::to_owned),
    }
}

#[test]
fn ordinary_channel_links_preserve_exact_message_and_thread_timestamps() {
    for ts in [
        "1719800000.000100",
        "p1719800000000100",
        "1719800000000100",
        "p1719800000.000100",
    ] {
        assert_eq!(
            parse(&format!(
                "slack://channel?id=C1&team=T1&message={ts}&thread_ts=1719800000.000001"
            )),
            Ok(channel(
                WorkspaceRef::TeamId("T1".into()),
                Some("1719800000.000100"),
                Some("1719800000.000001")
            ))
        );
    }
    assert_eq!(
        parse("slack://channel?team=T1&id=C1"),
        Ok(channel(WorkspaceRef::TeamId("T1".into()), None, None))
    );
    assert_eq!(
        parse("slack:///channel?team=T1&id=C1"),
        parse("slack://channel?team=T1&id=C1")
    );
}

#[test]
fn timestamps_never_round_or_accept_incomplete_precision() {
    for ts in [
        "1719800000.1",
        "1719800000.0001000",
        "171980000.000100",
        "17198000000.000100",
        "1719800000e000100",
        "-1719800000.000100",
        "1719800000.00010x",
        "1719800000.00010é",
    ] {
        assert_eq!(
            parse(&format!("slack://channel?team=T1&id=C1&message={ts}")),
            Err(ParseError::InvalidTimestamp),
            "{ts}"
        );
        assert_eq!(
            parse(&format!("slack://channel?team=T1&id=C1&thread_ts={ts}")),
            Err(ParseError::InvalidTimestamp),
            "{ts}"
        );
    }
    assert_eq!(
        timestamp("0000000000.000001"),
        Ok("0000000000.000001".into())
    );
}

#[test]
fn workspace_and_user_routes_are_distinct() {
    assert_eq!(
        parse("slack://open?team=T2"),
        Ok(Link::Workspace { team: "T2".into() })
    );
    assert_eq!(
        parse("slack://user?team=T2&id=U1"),
        Ok(Link::User {
            team: "T2".into(),
            user: "U1".into()
        })
    );
    assert_eq!(
        parse("slack://user?team=T2&id=W1"),
        Ok(Link::User {
            team: "T2".into(),
            user: "W1".into()
        })
    );
    for route in [
        "slack://open",
        "slack://user?id=U1",
        "slack://channel?id=C1",
    ] {
        assert_eq!(parse(route), Err(ParseError::MissingParameter));
    }
}

#[test]
fn duplicate_routing_parameters_are_rejected_even_when_identical_or_encoded() {
    for suffix in [
        "team=T1",
        "id=C1",
        "%74eam=T1",
        "message=1719800000.000100&message=1719800000.000100",
        "thread_ts=1719800000.000100&thread_ts=1719800000.000101",
        "cid=C1&cid=C1",
        "tab=files&tab=files",
    ] {
        assert_eq!(
            parse(&format!("slack://channel?team=T1&id=C1&{suffix}")),
            Err(ParseError::DuplicateParameter),
            "{suffix}"
        );
    }
    assert_eq!(
        parse(
            "https://acme.slack.com/archives/C1/p1719800000000100?thread_ts=1719800000.000001&thread_ts=1719800000.000002"
        ),
        Err(ParseError::DuplicateParameter)
    );
}

#[test]
fn malformed_routing_values_and_ambiguous_parameters_are_rejected() {
    for route in [
        "slack://channel?team=T1&id=",
        "slack://channel?team=T1&id=C1%",
        "slack://channel?team=T1&id=C1%FF",
        "slack://channel?team=T1&id=C1%0a",
    ] {
        assert_eq!(parse(route), Err(ParseError::InvalidQuery), "{route}");
    }
    for route in [
        "slack://channel?team=t1&id=C1",
        "slack://channel?team=T1&id=U1",
        "slack://channel?team=T1&id=C1%2fC2",
        "slack://channel?team=T1&id=C1&cid=C2",
        "slack://user?team=T1&id=C1",
    ] {
        assert_eq!(parse(route), Err(ParseError::InvalidIdentifier), "{route}");
    }
    for route in [
        "slack://channel/open?team=T1&id=C1",
        "slack://open?team=T1&id=C1",
        "slack://user?team=T1&id=U1&message=1719800000.000100",
        "https://acme.slack.com/archives/C1/p1719800000000100?team=T2",
    ] {
        assert_eq!(parse(route), Err(ParseError::InvalidRoute), "{route}");
    }
}

#[test]
fn permalink_keeps_workspace_authority_and_thread_target() {
    for host in ["acme.slack.com", "acme.enterprise.slack.com"] {
        for family in ["archives", "messages", "conversation"] {
            assert_eq!(
                parse(&format!(
                    "https://{host}/{family}/C1/p1719800000000100?thread_ts=1719800000.000001&cid=C1"
                )),
                Ok(channel(
                    WorkspaceRef::Host(host.into()),
                    Some("1719800000.000100"),
                    Some("1719800000.000001")
                ))
            );
        }
        assert_eq!(
            parse(&format!("https://{host}/archives/C1")),
            Ok(channel(WorkspaceRef::Host(host.into()), None, None))
        );
    }
    assert_eq!(
        parse("https://acme.slack.com/archives/C1/p1719800000000100?cid=C2"),
        Err(ParseError::InvalidIdentifier)
    );
    assert_eq!(
        parse("https://acme.slack.com/archives/C1/p171980000000010"),
        Err(ParseError::InvalidTimestamp)
    );
}

#[test]
fn lookalike_and_unrelated_web_hosts_are_external() {
    for host in [
        "slack.com.evil.test",
        "acme.fakeslack.com",
        "acme.slack.com.evil.test",
        "acme.evil.slack.com",
        "slack.com",
        "example.com",
        "acme.slack.com.",
    ] {
        let url = format!("https://{host}/archives/C1/p1719800000000100");
        assert_eq!(parse(&url), Ok(Link::External { url }), "{host}");
    }
    let url = "https://example.com/private?token=opaque&x=1&x=2#anchor";
    assert_eq!(parse(url), Ok(Link::External { url: url.into() }));
}

#[test]
fn unsupported_slack_commands_never_become_external_handlers() {
    for command in ["app", "connect", "unknown", "magic-login"] {
        assert_eq!(
            parse(&format!("slack://{command}?team=T1")),
            Ok(Link::UnhandledSlack {
                command: command.into()
            })
        );
    }
    assert_eq!(
        parse("slack://channel?team=T1&id=C1&tab=files"),
        Ok(Link::UnhandledSlack {
            command: "channel".into()
        })
    );
    assert_eq!(
        parse("slack://open?team=T1&campaign=miles_davis"),
        Ok(Link::UnhandledSlack {
            command: "open".into()
        })
    );
    assert_eq!(
        parse("https://acme.slack.com/archives/C1/tabs/files"),
        Ok(Link::UnhandledSlack {
            command: "archives".into()
        })
    );
    for url in [
        "javascript:alert(1)",
        "data:text/plain,hello",
        "file:///tmp/example",
        "mailto:a@example.com",
    ] {
        assert_eq!(parse(url), Err(ParseError::UnsupportedScheme));
    }
}

#[test]
fn internal_credentials_ports_fragments_and_input_control_bytes_are_rejected() {
    for url in [
        "slack://user:pass@channel?team=T1&id=C1",
        "https://user@acme.slack.com/archives/C1",
        "https://acme.slack.com:8443/archives/C1",
    ] {
        assert_eq!(parse(url), Err(ParseError::InvalidAuthority));
    }
    assert_eq!(
        parse("slack://channel?team=T1&id=C1#other"),
        Err(ParseError::InvalidRoute)
    );
    assert_eq!(
        parse("https://acme.slack.com/archives/C1#other"),
        Err(ParseError::InvalidRoute)
    );
    for url in [
        " slack://open?team=T1",
        "slack://open?team=T1\n",
        "https://acme.slack.com/arch\nives/C1",
    ] {
        assert_eq!(parse(url), Err(ParseError::InvalidUrl));
    }
}
