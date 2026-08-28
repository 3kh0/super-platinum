#!/usr/bin/env bash
# Launches real Dioxus Desktop fixture windows and captures them through the
# unchanged Super Platinum agent protocol. No Slack session or network access is used.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export SUPER_PLATINUM_UI_CAPTURE_DIR="${SUPER_PLATINUM_UI_CAPTURE_DIR:-tmp/agent-ui}"
mkdir -p "$SUPER_PLATINUM_UI_CAPTURE_DIR"

MANIFEST="Cargo.toml"
BINARY="target/debug/super-platinum"
FIXTURES=(
  login loading channel main-general-new-message-motion unreads threads
  rail-account-snoozed
  animated-reaction profile-hover-card profile-pane profile-pane-rich-fields
  profile-pane-min-width dm-header-compact
  channel-thread-profile-headers channel-huddle chat-paused-pill
  thread-unread-divider main-dev settings settings-storage appearance-countertop
  appearance-blue-steel appearance-paper-bag appearance-custom-background
  image-viewer-upload image-viewer-embed-compact gif-picker-attachment
  video-viewer search palette activity-unread activity-channel-post
  activity-thread-tools dms
  dm-history-failed dm-cached-refresh-failed toast-long-error
  connection-connecting connection-no-network composer-multiline
  composer-upload-progress
  edit-message-composer accounts self-menu self-menu-notifications
  multi-paragraph-custom-emoji message-unfurl-embed block-kit-layout
  media-loading-state
)

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
