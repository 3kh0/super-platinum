#!/usr/bin/env bash
# Launches real Dioxus Desktop fixture windows and captures them through the
# unchanged Super Platinum agent protocol. No Slack session or network access is used.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export SUPER_PLATINUM_UI_CAPTURE_DIR="${SUPER_PLATINUM_UI_CAPTURE_DIR:-tmp/agent-ui}"
mkdir -p "$SUPER_PLATINUM_UI_CAPTURE_DIR"

MANIFEST="Cargo.toml"
BINARY="${SUPER_PLATINUM_BINARY:-target/debug/super-platinum}"
FIXTURES=(
  login opening loading channel main-general-new-message-motion unreads threads
  threads-keyboard-narrow
  threads-file-paste threads-file-paste-narrow
  rail-account-snoozed
  animated-reaction reaction-picker later later-files later-files-narrow later-empty later-error later-reminder later-new-reminder
  bookmarks bookmarks-narrow bookmarks-edit bookmarks-remove bookmarks-error bookmarks-folder bookmarks-folder-remove bookmarks-move
  canvas-create canvas-create-narrow canvas-create-error
  canvas canvas-narrow canvas-loading canvas-error canvas-rename canvas-rename-error canvas-readonly
  canvas-checklist canvas-checklist-narrow canvas-checklist-saving canvas-checklist-uncertain
  canvas-text canvas-text-narrow
  files files-narrow files-empty files-error files-density files-density-narrow
  channel-files channel-files-narrow channel-links channel-links-narrow channel-links-empty channel-links-error
  pins pins-narrow pins-empty pins-error channel-pinned-message
  forward forward-narrow forward-error forward-partial forward-partial-narrow forward-people forward-people-narrow
  forward-recovery forward-recovery-narrow
  scheduled scheduled-empty scheduled-error scheduled-edit scheduled-send-confirm
  schedule-clear schedule-clear-narrow schedule-clear-thread
  drafts drafts-empty drafts-edit drafts-conflict drafts-no-destination channel-details channel-create channel-leave channel-leave-last channel-join channel-join-composer
  new-dm-directory new-dm-directory-loading new-dm-directory-error new-dm-directory-empty new-dm-directory-error-narrow
  local-drafts local-drafts-loading local-drafts-error local-drafts-error-narrow
  new-dm new-dm-selected new-dm-pending new-dm-error new-dm-narrow
  profile-hover-card profile-pane profile-dm-opening profile-vip-pending profile-pane-rich-fields
  profile-pane-min-width dm-header-compact
  channel-thread-profile-headers channel-huddle huddle-connected huddle-invite
  huddle-video huddle-video-expanded huddle-settings huddle-people huddle-captions
  chat-paused-pill
  thread-follow-on thread-follow-off thread-follow-pending thread-follow-narrow
  thread-feed thread-feed-empty thread-feed-error thread-feed-loading thread-feed-updated thread-feed-narrow
  thread-counts-mentions thread-counts-unread thread-counts-clear thread-counts-narrow
  thread-unread-divider thread-list-unread thread-marked-unread thread-marked-unread-narrow thread-target thread-error thread-paging thread-paging-error thread-grouped thread-grouped-pending thread-window-200 thread-window-2000 thread-window-10000 thread-window-target thread-window-gap main-dev settings settings-storage appearance-countertop
  appearance-blue-steel appearance-paper-bag appearance-custom-background
  image-viewer-upload image-viewer-embed-compact gif-picker-attachment
  pdf-preview pdf-preview-narrow pdf-preview-error pdf-preview-search-long
  viewer-actions-files viewer-actions-message viewer-actions-narrow viewer-actions-pdf
  file-actions file-actions-narrow file-actions-channel-narrow file-rename file-rename-narrow file-rename-channel-narrow file-recovery file-recovery-narrow file-preview-renamed-files file-preview-renamed-message
  file-preview-text file-preview-narrow file-preview-large file-preview-utf16 file-preview-empty file-preview-binary file-preview-audio
  video-viewer search search-opening search-loading search-empty search-error search-more-error search-narrow search-filters search-filters-narrow palette palette-people activity-unread activity-channel-post
  activity-thread-tools activity-paging activity-loading activity-error activity-more-error activity-empty activity-narrow activity-navigation activity-navigation-narrow dms
  dm-history-failed dm-cached-refresh-failed toast-long-error
  connection-connecting connection-no-network composer-multiline
  composer-upload-progress
  edit-message-composer accounts self-menu self-menu-notifications
  multi-paragraph-custom-emoji slack-connect-avatar message-unfurl-embed block-kit-layout button-metadata button-metadata-narrow block-control-values block-control-values-narrow
  app-modal-form app-modal-form-narrow app-modal-lifecycle app-modal-lifecycle-narrow
  app-modal-users app-modal-users-narrow
  app-modal-user app-modal-user-narrow
  app-modal-conversation app-modal-conversation-narrow
  app-modal-channel app-modal-channel-narrow
  app-modal-multi-channel app-modal-multi-channel-narrow
  app-modal-multi-conversation app-modal-multi-conversation-narrow
  message-multi-conversations-select message-multi-conversations-select-narrow
  message-multi-channels-select message-multi-channels-select-narrow
  slack-list slack-list-narrow slack-list-readonly slack-list-incomplete slack-list-error slack-list-loading
  slack-list-board-add slack-list-board-add-narrow
  self-status-editor self-status-editor-narrow
  profile-edit profile-edit-narrow
  profile-title profile-title-narrow
  message-radio-buttons message-radio-buttons-narrow
  message-multi-controls message-multi-controls-narrow
  message-datepicker message-datepicker-narrow
  message-users-select message-users-select-narrow
  message-multi-users-select message-multi-users-select-narrow
  message-channels-select message-channels-select-narrow
  message-conversations-select message-conversations-select-narrow
  composer-emoji composer-emoji-narrow
  channel-mute channel-mute-narrow
  channel-notifications channel-notifications-narrow
  notification-schedule notification-schedule-narrow
  notification-presentation notification-presentation-narrow
  global-notifications global-notifications-narrow
  thread-broadcast thread-broadcast-narrow
  message-broadcast message-broadcast-narrow
  upload-delivery-check upload-delivery-check-narrow
  upload-restart upload-restart-narrow
  message-timepicker message-timepicker-narrow
  message-static-select message-static-select-narrow
  message-external-select message-external-select-narrow
  app-modal-external-select app-modal-external-select-narrow
  message-multi-external-select message-multi-external-select-narrow
  app-modal-multi-external-select app-modal-multi-external-select-narrow
  message-overflow message-overflow-narrow
  message-button-actions message-button-actions-narrow message-button-metadata message-button-metadata-narrow
  slack-links slack-links-narrow
  rejected-thread-return rejected-thread-return-narrow
  draft-native-projection draft-native-projection-narrow
  draft-remote-review draft-remote-review-narrow
  scheduled-attachments scheduled-attachments-narrow
  user-links user-links-narrow
  rejected-send rejected-send-narrow
  media-loading-state media-failed timeline-variable-height timeline-latest-opening
  usergroup-members usergroup-channels system-messages send-outcomes send-outcomes-narrow send-outcomes-thread managed-draft-recovery managed-draft-recovery-narrow message-marked-unread message-marked-unread-narrow member-live-refresh channel-members member-add member-add-error member-remove member-add-narrow member-remove-narrow
)

# Optional fixture names allow a focused recheck after the full sweep.
if [[ "$#" -gt 0 ]]; then FIXTURES=("$@"); fi

echo "agent-ui-check: building the locked Dioxus desktop shell…"
cargo build --manifest-path "$MANIFEST" --locked

# A sleeping display has no composited window surface, so `screencapture -l`
# fails outright ("could not create image from window") and every fixture in the
# sweep is lost. Hold the display awake for the run instead.
awake_pid=""
if [[ "$(uname -s)" == "Darwin" ]] && command -v caffeinate >/dev/null 2>&1; then
  caffeinate -u -t 2 || true
  caffeinate -d &
  awake_pid="$!"
fi

cleanup_pid=""
cleanup_socket=""
cleanup() {
  if [[ -n "$cleanup_pid" ]] && kill -0 "$cleanup_pid" 2>/dev/null; then
    kill "$cleanup_pid" 2>/dev/null || true
    wait "$cleanup_pid" 2>/dev/null || true
  fi
  if [[ -n "$cleanup_socket" ]]; then
    rm -f "$cleanup_socket"
  fi
}
release_display() {
  if [[ -n "$awake_pid" ]] && kill -0 "$awake_pid" 2>/dev/null; then
    kill "$awake_pid" 2>/dev/null || true
  fi
}
trap 'cleanup; release_display' EXIT INT TERM

fixture_index=0
for fixture in "${FIXTURES[@]}"; do
  socket="${TMPDIR:-/tmp}/saui-$$-${fixture_index}.sock"
  fixture_index=$((fixture_index + 1))
  log="$SUPER_PLATINUM_UI_CAPTURE_DIR/${fixture}.log"
  png="$SUPER_PLATINUM_UI_CAPTURE_DIR/${fixture}.png"
  rm -f "$socket" "$png"
  cleanup_socket="$socket"

  echo "agent-ui-check: launching ${fixture}…"
  SUPER_PLATINUM_FIXTURE="$fixture" \
    SUPER_PLATINUM_AGENT=1 \
    SUPER_PLATINUM_AGENT_SOCK="$socket" \
    "$BINARY" >"$log" 2>&1 &
  cleanup_pid="$!"

  ready=false
  for _ in {1..100}; do
    if SUPER_PLATINUM_AGENT_SOCK="$socket" scripts/agentctl.sh ping >/dev/null 2>&1; then
      ready=true
      break
    fi
    sleep 0.05
  done
  if [[ "$ready" != true ]]; then
    echo "agent-ui-check: $fixture did not open its agent socket" >&2
    tail -40 "$log" >&2 || true
    exit 1
  fi

  # The agent socket can accept requests before WebKit has committed its first
  # painted frame. Give the native window one animation frame plus compositor
  # slack so a successful capture cannot be an all-white pre-paint surface.
  sleep 0.25
  if [[ "$fixture" == pdf-preview* ]]; then sleep 1; fi

  # The window can still have no capturable surface for a beat after that, which
  # `screencapture` reports as a plain failure. Retry before giving up on the
  # whole sweep.
  captured=false
  for _ in {1..10}; do
    if SUPER_PLATINUM_AGENT_SOCK="$socket" scripts/agentctl.sh screenshot "$png" >/dev/null 2>&1; then
      captured=true
      break
    fi
    sleep 0.4
  done
  if [[ "$captured" != true ]]; then
    echo "agent-ui-check: could not capture $fixture window" >&2
    tail -40 "$log" >&2 || true
    exit 1
  fi
  echo "agent-ui-check: wrote $png"

  cleanup
  cleanup_pid=""
  cleanup_socket=""
done

echo "agent-ui-check: real desktop captures written to $SUPER_PLATINUM_UI_CAPTURE_DIR"
ls -la "$SUPER_PLATINUM_UI_CAPTURE_DIR"
