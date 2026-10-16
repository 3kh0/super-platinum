use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::file_share) enum Variant {
    Exact,
    Changed,
    Missing,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::file_share) enum Input {
    Text,
    DestinationOnly,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Marker {
    pub schema: u8,
    pub kind: String,
    pub run: String,
    pub nonce: String,
    pub variant: Variant,
    pub input: Input,
    pub handoff_sha256: String,
    pub source_sha256: String,
    pub executable_sha256: String,
    pub build_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pin {
    pub size: u64,
    pub sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Handoff {
    pub schema: u8,
    pub kind: String,
    pub run: String,
    pub capture_nonce: String,
    pub variant: Variant,
    pub input: Input,
    pub account: String,
    pub team: String,
    pub user: String,
    pub file: String,
    pub source_sha256: String,
    pub executable_sha256: String,
    pub build_sha256: String,
    pub database: Pin,
    pub draft: Pin,
    pub audit: Pin,
    pub schema_sha256: String,
    pub user_version: u8,
    pub counts: Counts,
}
#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Counts {
    pub identity: u8,
    pub drafts: u8,
    pub receipts: u8,
    pub associations: u8,
    pub cooldowns: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExitAudit {
    pub schema: u8,
    pub kind: String,
    pub run: String,
    pub nonce: String,
    pub pid: u32,
    pub creation_time: u64,
    pub exited: bool,
    pub terminal_ok: bool,
    pub pointer_unchanged: bool,
    pub cleanup_errors: Vec<String>,
    pub jobs: u8,
    pub scopes: u8,
    pub running: u8,
    pub api_reads: u8,
    pub begins: u8,
    pub dispatches: u8,
}
pub(super) fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn nonce(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        && s.as_bytes()[14] == b'4'
        && matches!(s.as_bytes()[19], b'8' | b'9' | b'a' | b'b')
}
pub(super) fn valid(marker: &Marker, handoff: &Handoff, audit: &ExitAudit) -> bool {
    marker.schema == 1
        && marker.kind == "file-share-draft-reopen"
        && nonce(&marker.run)
        && nonce(&marker.nonce)
        && marker.run != marker.nonce
        && handoff.schema == 1
        && handoff.kind == "file-share-draft-handoff"
        && handoff.run == marker.run
        && handoff.variant == marker.variant
        && handoff.input == marker.input
        && nonce(&handoff.capture_nonce)
        && handoff.capture_nonce != marker.nonce
        && handoff.capture_nonce != marker.run
        && handoff.account == ACCOUNT
        && handoff.team == TEAM
        && handoff.user == USER
        && handoff.file == FILE
        && marker.source_sha256 == handoff.source_sha256
        && marker.executable_sha256 == handoff.executable_sha256
        && marker.build_sha256 == handoff.build_sha256
        && [
            &marker.handoff_sha256,
            &marker.source_sha256,
            &marker.executable_sha256,
            &marker.build_sha256,
            &handoff.schema_sha256,
            &handoff.database.sha256,
            &handoff.draft.sha256,
            &handoff.audit.sha256,
        ]
        .into_iter()
        .all(|s| sha(s))
        && handoff.schema_sha256 == include_str!("schema.sha256").trim()
        && handoff.user_version == 2
        && handoff.counts
            == (Counts {
                identity: 1,
                drafts: 1,
                receipts: 0,
                associations: 0,
                cooldowns: 0,
            })
        && audit.schema == 1
        && audit.kind == "file-share-draft-exit"
        && audit.run == marker.run
        && audit.nonce == handoff.capture_nonce
        && audit.pid > 0
        && audit.pid != std::process::id()
        && audit.creation_time > 0
        && audit.exited
        && audit.terminal_ok
        && audit.pointer_unchanged
        && audit.cleanup_errors.is_empty()
        && [
            audit.jobs,
            audit.scopes,
            audit.running,
            audit.api_reads,
            audit.begins,
            audit.dispatches,
        ] == [0; 6]
}
