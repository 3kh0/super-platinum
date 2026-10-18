# Canvas comment Resolve / Re-open protocol

This extends the independently reviewed comment reader with one existing-thread
mutation. It does not implement comment creation, reply, edit, delete, Resolve
all, read marking or realtime collaboration. No real Slack mutation was made.

## Primary evidence

[Slack Canvas help](https://slack.com/help/articles/203950418-Use-a-canvas-in-Slack)
and [Canvas access](https://slack.com/help/articles/15678967614611-Manage-access-permissions-for-canvases-and-lists)
provide product context. Exact private wire behavior comes from the retained
shipped client, pinned with offsets and short excerpts in this package's
`references/mutation-protocol-index.json`:

- Shared `gantry-v2-shared.0b81595876785d7a.min.js`, SHA256
  `f085df09fb4f938caf425ea66fb0244c8f92a8012eea8eb298e4f629b377dbf7`.
- Boot `gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js`, SHA256
  `1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537`.
- Vendors `gantry-v2-async-gantry-v2-vendors-async.daa946d0953564d3.min.js`, SHA256
  `8904e661fc5858d74b7e2f11400b2f1f0f0f746cf188a74fa3bc8db4de7a1362`.

Boot `BRw9`5411595 reads the exact existing root, checks `Bo5c.canArchiveComment`,
finds the Quip controller, updates its archive state, then calls `qTl6.rO` for
archive or `VgKb.Et` for unarchive with `channelId`, `threadTs` and a diagnostic
reason. On `ok`, it normalizes `response.message` into the message store and
removes a resolved root from the active stream. Vendors `qTl6`9792531 and
`VgKb`9792880 name `quip.thread.archive` and `quip.thread.unarchive`.
Shared `uGgu`1594178 strips `reason`, `abortSignal` and `timeout` before the API
fetcher snake-cases the actual arguments. Therefore the mutation form is only
`channel_id`, `thread_ts` and the existing client's token, not file ID, Quip
thread ID, document delta, invented Canvas API, or a replay/idempotency key.

Boot `Bo5c`8510684 requires a file channel, writable unlocked file, an existing
`document_comment.thread_id` and positive reply count. Shared `XNVs`3757572
requires write/owner for Canvas and excludes locked files; comment-only access
does not confer resolve permission. Labels are read from ArchiveCommentButton
at boot16093654. File-channel identity uses the reader's proven F-to-C mapping.

## Implemented ownership and delivery

Presentation uses a cheap typed eligibility check. A deliberate action captures
exact normalized root content, root timestamp, Quip thread identity and desired
archive state. The background preflight rereads files.info and the canonical
root through conversations.replies. It requires current account/team/viewer,
credential/transport/epoch, Canvas visit, reader serial and exact original root.
No request follows a changed source or retired dialog. All disk and network work
is asynchronous, outside render.

The existing Canvas transport's one-attempt implementation is shared, not
reimplemented. The added entry point admits only the two exact methods and
three unique fields. No retry, redirect or offline queue is introduced. Only a
failure before the existing transport handoff is NotSent. HTTP errors, unknown
API codes, lost/malformed responses and partial acknowledgements remain
uncertain. A successful response must carry the original root/thread and desired
archive state; a fresh canonical read independently checks that state before
retiring its exact receipt. No matching future read alone manufactures an ACK.

The existing account SQLite Canvas journal gains typed comment receipts and
local review evidence. The v3 schema preserves existing structural receipt and
review rows. An exact receipt is committed before dispatch; both mutation kinds
share the file-wide lock and32 outstanding journal slots. Existing shell custody
also shares8-current-owner/32-total limits. Credential rotation/reopen cannot
turn a restored receipt into dispatch authority. Known-unsent cleanup and
ACK-backed readback can retry local cleanup, never the write. An old receipt
cannot remove a newer operation.

Uncertain results offer explicit review: fetch the exact current thread, show
requested and observed state with no default acknowledgement, then retain the
original record plus observed root and `local_unlock_outcome_unknown` decision
in the same transaction that removes only the exact lock. This is local recovery,
not proof of Slack delivery and not an automatic resend. The review uses exact
owner/read/review-visit guards. Failure retains the lock; a late completion for
the retired operation cannot delete its successor. Review evidence has a64MiB
account-local byte budget with no eviction; exhaustion refuses removal. Deleted
or inaccessible roots cannot yet be locally reviewed through this UI.

Response/root64KiB, receipt96KiB, stored review byte budget, exact raw preflight
comparison and the extra canonical readback are conservative local policies,
not Slack-advertised limits. Existing reader Retry-After fences apply to explicit
review and mutations; expiry only wakes controls, never makes a request.

## Remaining protocol and parity

The retained client shows message authoring feeding the normal prepare/send
pipeline and Quip callbacks for comment-thread messages/archive/deletion, but the
exact creation/bootstrap/reply/edit/delete end-to-end payload and merge path is
not fully recovered here. No guessed authoring API or synthetic authority is
added. This slice refreshes its owned reader after confirmed writes; it does not
optimistically edit the HTML export or implement realtime Quip controller state.
Native synthetic fixtures and source evidence do not prove live permissions,
actual server acknowledgement completeness, or full comments parity.
