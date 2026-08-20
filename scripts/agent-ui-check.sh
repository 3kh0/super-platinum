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
  animated-reaction profile-hover-card profile-pane dm-header-compact
  channel-thread-profile-headers channel-huddle chat-paused-pill
  thread-unread-divider main-dev settings appearance-countertop
  appearance-blue-steel appearance-paper-bag appearance-custom-background
  image-viewer-upload image-viewer-embed-compact gif-picker-attachment
  video-viewer search palette activity-unread activity-channel-post dms
  dm-history-failed dm-cached-refresh-failed composer-multiline
  composer-upload-progress
  edit-message-composer accounts
  multi-paragraph-custom-emoji
)

echo "agent-ui-check: building the locked Dioxus desktop shell…"
cargo build --manifest-path "$MANIFEST" --locked

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
trap cleanup EXIT INT TERM

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

  SUPER_PLATINUM_AGENT_SOCK="$socket" scripts/agentctl.sh screenshot "$png" >/dev/null
  echo "agent-ui-check: wrote $png"

  cleanup
  cleanup_pid=""
  cleanup_socket=""
done

echo "agent-ui-check: real desktop captures written to $SUPER_PLATINUM_UI_CAPTURE_DIR"
ls -la "$SUPER_PLATINUM_UI_CAPTURE_DIR"
