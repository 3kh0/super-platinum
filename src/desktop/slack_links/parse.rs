//! The ordinary Slack URL shapes emitted by the captured desktop client.
//!
//! Parsing never resolves a workspace or launches an operating-system URL handler.
//! In particular, a permalink hostname is not evidence that it belongs to the
//! currently selected account. The caller must resolve it against signed-in teams.

use std::collections::BTreeMap;

use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceRef {
    TeamId(String),
    Host(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    Channel {
        workspace: WorkspaceRef,
        channel: String,
        message: Option<String>,
        thread: Option<String>,
    },
    Workspace {
        team: String,
    },
    User {
        team: String,
        user: String,
    },
    External {
        url: String,
    },
    UnhandledSlack {
        command: String,
    },
}

/// Errors deliberately contain no URL, which may include private query values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseError {
    InvalidUrl,
    UnsupportedScheme,
    InvalidAuthority,
    InvalidRoute,
    InvalidQuery,
    DuplicateParameter,
    MissingParameter,
    InvalidIdentifier,
    InvalidTimestamp,
}

pub(crate) fn parse(input: &str) -> Result<Link, ParseError> {
    // URL parsing otherwise silently strips some whitespace and control bytes.
    if input.trim() != input || input.chars().any(char::is_control) {
        return Err(ParseError::InvalidUrl);
    }
    let url = Url::parse(input).map_err(|_| ParseError::InvalidUrl)?;
    match url.scheme() {
        "slack" => deep_link(&url),
        "https" | "http" if url.host_str().is_some() => {
            if let Some(host) = url.host_str().filter(|host| workspace_host(host)) {
                let parts: Vec<_> = url.path().split('/').skip(1).collect();
                // These are the captured permalink families. Unsupported web
                // families retain their existing browser fallback.
                if matches!(
                    parts.first(),
                    Some(&"archives" | &"messages" | &"conversation")
                ) {
                    return permalink(&url, host, &parts);
                }
            }
            Ok(Link::External { url: input.into() })
        }
        "https" | "http" => Err(ParseError::InvalidUrl),
        _ => Err(ParseError::UnsupportedScheme),
    }
}

fn internal_authority(url: &Url) -> Result<(), ParseError> {
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return Err(ParseError::InvalidAuthority);
    }
    if url.fragment().is_some() {
        return Err(ParseError::InvalidRoute);
    }
    Ok(())
}

fn deep_link(url: &Url) -> Result<Link, ParseError> {
    internal_authority(url)?;
    // Slack's reader accepts the command in the hostname or in the path.
    // Conflicting nonempty hostname/path commands are not an ordinary route.
    let command = match (url.host_str(), url.path().trim_matches('/')) {
        (Some(host), "") => host,
        (None, path) if !path.is_empty() && !path.contains('/') => path,
        _ => return Err(ParseError::InvalidRoute),
    };
    let query = routing_query(url)?;
    if !matches!(command, "channel" | "user" | "open") {
        return Ok(Link::UnhandledSlack {
            command: command.into(),
        });
    }
    // Channel tabs and campaign-specific open commands have separate captured
    // handlers; treating them as an ordinary channel/workspace loses their intent.
    if query.contains_key("tab") || query.contains_key("campaign") {
        return Ok(Link::UnhandledSlack {
            command: command.into(),
        });
    }
    let team = identifier(required(&query, "team")?, b"T")?.to_owned();
    match command {
        "channel" => {
            let channel = identifier(required(&query, "id")?, b"CGD")?.to_owned();
            if query.get("cid").is_some_and(|cid| cid != &channel) {
                return Err(ParseError::InvalidIdentifier);
            }
            Ok(Link::Channel {
                workspace: WorkspaceRef::TeamId(team),
                channel,
                message: query
                    .get("message")
                    .map(|value| timestamp(value))
                    .transpose()?,
                thread: query
                    .get("thread_ts")
                    .map(|value| timestamp(value))
                    .transpose()?,
            })
        }
        "user" => {
            reject_parameters(&query, &["message", "thread_ts", "cid"])?;
            Ok(Link::User {
                team,
                user: identifier(required(&query, "id")?, b"UW")?.to_owned(),
            })
        }
        "open" => {
            reject_parameters(&query, &["id", "message", "thread_ts", "cid"])?;
            Ok(Link::Workspace { team })
        }
        _ => unreachable!(),
    }
}

fn permalink(url: &Url, host: &str, parts: &[&str]) -> Result<Link, ParseError> {
    internal_authority(url)?;
    let query = routing_query(url)?;
    // Team/channel/message come from the authority/path for this URL family.
    reject_parameters(&query, &["team", "id", "message"])?;
    let channel = identifier(
        parts.get(1).copied().ok_or(ParseError::InvalidRoute)?,
        b"CGD",
    )?;
    if query.get("cid").is_some_and(|cid| cid != channel) {
        return Err(ParseError::InvalidIdentifier);
    }
    if parts.len() > 3 || query.contains_key("tab") {
        return Ok(Link::UnhandledSlack {
            command: parts[0].into(),
        });
    }
    let message = match parts.get(2).copied() {
        None | Some("") => None,
        Some(value) if value.starts_with('p') => Some(timestamp(value)?),
        Some(_) => return Err(ParseError::InvalidRoute),
    };
    Ok(Link::Channel {
        workspace: WorkspaceRef::Host(host.into()),
        channel: channel.into(),
        message,
        thread: query
            .get("thread_ts")
            .map(|value| timestamp(value))
            .transpose()?,
    })
}

/// Production workspace hosts only: the suffix boundary excludes lookalikes,
/// and the captured enterprise workspace spelling remains a distinct authority.
fn workspace_host(host: &str) -> bool {
    let Some(prefix) = host.strip_suffix(".slack.com") else {
        return false;
    };
    let workspace = prefix.strip_suffix(".enterprise").unwrap_or(prefix);
    !workspace.is_empty()
        && !workspace.contains('.')
        && !workspace.starts_with('-')
        && !workspace.ends_with('-')
        && workspace
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn identifier<'a>(value: &'a str, prefixes: &[u8]) -> Result<&'a str, ParseError> {
    let bytes = value.as_bytes();
    if bytes.len() < 2
        || !prefixes.contains(&bytes[0])
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(ParseError::InvalidIdentifier);
    }
    Ok(value)
}

/// Captured aW5l normalizer: /^p?(\d{10})\.? (\d{6})$/ (without the space).
/// Never pass these identifiers through floating-point numbers.
fn timestamp(value: &str) -> Result<String, ParseError> {
    let value = value.strip_prefix('p').unwrap_or(value);
    let (seconds, fraction) = match value.len() {
        16 if value.is_ascii() => value.split_at(10),
        17 if value.is_ascii() && value.as_bytes()[10] == b'.' => (&value[..10], &value[11..]),
        _ => return Err(ParseError::InvalidTimestamp),
    };
    if !seconds
        .bytes()
        .chain(fraction.bytes())
        .all(|byte| byte.is_ascii_digit())
    {
        return Err(ParseError::InvalidTimestamp);
    }
    Ok(format!("{seconds}.{fraction}"))
}

fn routing_query(url: &Url) -> Result<BTreeMap<String, String>, ParseError> {
    let raw = url.query().unwrap_or_default().as_bytes();
    for (index, byte) in raw.iter().enumerate() {
        if *byte == b'%'
            && !(raw.get(index + 1).is_some_and(u8::is_ascii_hexdigit)
                && raw.get(index + 2).is_some_and(u8::is_ascii_hexdigit))
        {
            return Err(ParseError::InvalidQuery);
        }
    }
    let mut query = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if !matches!(
            key.as_ref(),
            "team" | "id" | "message" | "thread_ts" | "cid" | "tab" | "campaign"
        ) {
            continue;
        }
        if value.is_empty() || value.chars().any(char::is_control) || value.contains('\u{fffd}') {
            return Err(ParseError::InvalidQuery);
        }
        if query.insert(key.into_owned(), value.into_owned()).is_some() {
            return Err(ParseError::DuplicateParameter);
        }
    }
    Ok(query)
}

fn required<'a>(query: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, ParseError> {
    query
        .get(key)
        .map(String::as_str)
        .ok_or(ParseError::MissingParameter)
}

fn reject_parameters(query: &BTreeMap<String, String>, keys: &[&str]) -> Result<(), ParseError> {
    if keys.iter().any(|key| query.contains_key(*key)) {
        Err(ParseError::InvalidRoute)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
