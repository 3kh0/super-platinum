#!/usr/bin/env bash
# Profile Canvas section references: scripts/check-profile-canvas-section-native.py (success/missing/failure/cancel/retirement; normal/narrow).
# Profile Canvas deletion events: scripts/check-profile-canvas-deletion-event-native.py (known/mounted/held/retirement/plural; normal/narrow).
# Profile deleted Canvas reference lifecycle: scripts/check-profile-canvas-deleted-native.py
# Known nested usergroup titles: check-profile-usergroup-title-native.py (known/failure/cancel/retirement).
# Shared missing nested channel acquisition: check-profile-missing-channel-title-native.py
# --case positive|omission|failure|cancel|retirement (normal/narrow; closed owned transport).
# Profile nested public channel titles: check-profile-channel-title-native.py
# --case known/failure/cancel/retirement, same profile-sections-mentions[-narrow] fixtures.
# Known private-member titles: check-profile-private-channel-title-native.py; same cases
# plus membership restoration, with the explicit private-title flag (public checks unchanged).
# Explicit English nonmember placeholders: check-profile-nonmember-channel-title-native.py
# adds the nonmember-title flag and advances the membership case to placeholder custody.
# Profile emoji title lifecycle: use check-profile-emoji-title-native.py with the existing
# profile-sections-mentions[-narrow] fixture and scoped emoji-title flags.
# Launches real Dioxus Desktop fixture windows and captures them through the
# unchanged Super Platinum agent protocol. No Slack session or network access is used.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export SUPER_PLATINUM_UI_CAPTURE_DIR="${SUPER_PLATINUM_UI_CAPTURE_DIR:-tmp/agent-ui}"
mkdir -p "$SUPER_PLATINUM_UI_CAPTURE_DIR"

# Formatted static Canvas title paragraph edits: check-canvas-formatted-title-edit-native.py
# --fixture canvas-text or canvas-text-narrow (exact new flag; old text check unchanged).
# Plain own-comment native interactions use the existing slack-list/slack-list-narrow
# fixtures through scripts/check-list-own-edit-plain-native.py: own-edit-plain,
# own-edit-plain-storage, own-edit-plain-emoji, own-edit-plain-custom and
# own-edit-plain-custom-retired, own-edit-plain-noop, own-edit-plain-noop-storage,
# own-edit-plain-emoticon and own-edit-plain-emoticon-off.
# Smart-symbol interactions use check-list-own-edit-smart-symbols-native.py:
# own-edit-plain-smart-symbols, own-edit-plain-smart-symbols-off,
# own-edit-plain-smart-symbols-disabled (same normal/narrow fixtures).
# Go-link policy: check-list-own-edit-go-links-native.py, own-edit-plain-go-links
# Visual emoji mode: check-list-own-edit-emoji-display-native.py,
# own-edit-plain-emoji-display / own-edit-plain-emoji-display-recovery (normal/narrow).
# Member/group local plain Save/recovery: scripts/check-list-plain-member-save-native.py
# --case own-edit-plain-member --captures <fresh-directory> (normal/narrow).
# Channel-only plain Save/recovery: scripts/check-list-plain-channel-save-native.py
# --case own-edit-plain-channel --captures <fresh-directory> (normal/narrow).
  # Explicit shared emoji source: check-rich-emoji-display-source-native.py,
  # rich-emoji-display-source (normal/narrow; pending/ready/URL replacement).
  # Combined Unicode/source: check-rich-emoji-combined-native.py,
  # rich-emoji-combined (normal/narrow; unobserved/pending/ready/replacement).
# (same normal/narrow fixtures; actual assignment, literal plain text, Save/readback).
# The owned runner supplies the
# exact offline case flag.
# Canonical channel/thread selected emoji uses check-channel-thread-emoji-native.py
# with channel-thread-emoji / channel-thread-emoji-narrow. Its owned runner supplies
# SUPER_PLATINUM_CHANNEL_THREAD_EMOJI_CHECK and capture acknowledgements for
# unobserved/assigned-pending/assigned-ready/socket-retired/fresh-assigned-ready.
# Canvas comment selected emoji uses check-canvas-comment-emoji-native.py with
# canvas-comment-emoji / canvas-comment-emoji-narrow. Its owned runner supplies
# SUPER_PLATINUM_CANVAS_COMMENT_EMOJI_CHECK, private reply storage and capture ACKs.
# Profile Canvas link titles use check-profile-link-title-native.py with
# profile-sections-mentions / profile-sections-mentions-narrow. The owned runner
# supplies SUPER_PLATINUM_PROFILE_LINK_TITLE_CHECK plus original files/blocks flags
# and runs the ten-stage cold/read/rename/Save/Retry/Cancel/length lifecycle.
# These guarded interaction fixtures are run through that checker, not this
# static screenshot loop.
MANIFEST="Cargo.toml"
BINARY="${SUPER_PLATINUM_BINARY:-target/debug/super-platinum}"
FIXTURES=(
  login opening loading channel main-general-new-message-motion unreads threads
  completion-directory-2000 completion-directory-2000-narrow
  completion-directory-10000 completion-directory-10000-narrow
  completion-directory-64000 completion-directory-64000-narrow
  threads-keyboard-narrow
  threads-file-paste threads-file-paste-narrow
  rail-account-snoozed
  animated-reaction reaction-picker later later-files later-files-narrow later-cold-message-navigation later-cold-message-navigation-narrow reconnect-thread-continuation reconnect-thread-continuation-narrow reconnect-unread-opening reconnect-unread-opening-narrow ordinary-history-cursor ordinary-history-cursor-narrow later-empty later-error later-reminder later-new-reminder later-read-recovery later-read-recovery-narrow
  bookmarks bookmarks-narrow bookmarks-edit bookmarks-remove bookmarks-error bookmarks-folder bookmarks-folder-remove bookmarks-move
  channel-canvas-surface channel-canvas-surface-narrow
  channel-tabs-canvas channel-tabs-canvas-narrow
  channel-tabs-manager channel-tabs-manager-narrow
  channel-tabs-enterprise channel-tabs-enterprise-narrow
  channel-tabs-knowledge channel-tabs-knowledge-narrow
  channel-tabs-folder channel-tabs-folder-narrow channel-tabs-folder-manage channel-tabs-folder-manage-narrow
  bookmarks-emoji bookmarks-emoji-narrow
  canvas-create canvas-create-narrow canvas-create-error
  canvas-creation-durable canvas-creation-durable-narrow
  canvas-links canvas-links-narrow
  canvas canvas-narrow canvas-loading canvas-error canvas-rename canvas-rename-error canvas-readonly
  canvas-comments canvas-comments-narrow
  canvas-root-attribution canvas-root-attribution-narrow
  canvas-editor-recovery canvas-editor-recovery-narrow
  canvas-file-fallback canvas-file-fallback-narrow
  canvas-canonical-file-roots canvas-canonical-file-roots-narrow
  canvas-canonical-file-custody canvas-canonical-file-custody-narrow
  canvas-canonical-native-canvas canvas-canonical-native-canvas-narrow
  canvas-canonical-native-list canvas-canonical-native-list-narrow
  canvas-comment-resolve canvas-comment-resolve-narrow canvas-comment-reply canvas-comment-reply-narrow canvas-comment-rich-reply canvas-comment-rich-reply-narrow canvas-comment-annotation canvas-comment-annotation-narrow canvas-comment-inline canvas-comment-inline-narrow canvas-comment-existing-inline canvas-comment-existing-inline-narrow canvas-comment-archived-inline canvas-comment-archived-inline-narrow canvas-comment-hidden-inline canvas-comment-hidden-inline-narrow canvas-comment-section canvas-comment-section-narrow canvas-comment-section-inline canvas-comment-section-inline-narrow canvas-comment-list canvas-comment-list-narrow canvas-comment-image canvas-comment-image-narrow canvas-comment-table canvas-comment-table-narrow canvas-comment-table-bodies canvas-comment-table-bodies-narrow canvas-comment-blockquote canvas-comment-blockquote-narrow canvas-comment-flexbox canvas-comment-flexbox-narrow canvas-comment-callout canvas-comment-callout-narrow
  canvas-checklist canvas-checklist-narrow canvas-checklist-saving canvas-checklist-uncertain
  canvas-text canvas-text-narrow
  canvas-structure canvas-structure-narrow
  # Owned snapshot/journal/capture ACK initialization comes from the dedicated runner.
  canvas-owner-delete canvas-owner-delete-narrow
  canvas-live-split-rich canvas-live-split-rich-narrow
  canvas-split-gap canvas-split-gap-narrow
  canvas-coalesced-owner-rich canvas-coalesced-owner-rich-narrow
  canvas-live-split-child-delete canvas-live-split-child-delete-narrow
  canvas-retained-pair-delete canvas-retained-pair-delete-narrow
  canvas-owner-first-pair-delete canvas-owner-first-pair-delete-narrow
  canvas-last-live-replacement canvas-last-live-replacement-narrow
  canvas-confirmed-provenance canvas-confirmed-provenance-narrow
  canvas-retirement-cancellation canvas-retirement-cancellation-narrow
  canvas-structure-mixed canvas-structure-mixed-narrow
  canvas-replacement canvas-replacement-narrow
  canvas-structure-text canvas-structure-text-narrow
  canvas-local-review canvas-local-review-narrow
  files files-narrow files-empty files-error files-density files-density-narrow
  resource-files-recovery resource-files-recovery-narrow resource-links-recovery resource-links-recovery-narrow
  channel-files channel-files-narrow channel-links channel-links-narrow channel-links-empty channel-links-error
  pins pins-narrow pins-empty pins-error channel-pinned-message
  pins-files pins-files-narrow
  forward-links forward-links-narrow
  forward forward-narrow forward-error forward-partial forward-partial-narrow forward-people forward-people-narrow
  forward-recovery forward-recovery-narrow
  forward-scheduled forward-scheduled-narrow
  forward-scheduled-dynamic forward-scheduled-dynamic-narrow
  scheduled scheduled-empty scheduled-error scheduled-edit scheduled-send-confirm
  schedule-clear schedule-clear-narrow schedule-clear-thread
  drafts drafts-empty drafts-edit drafts-conflict drafts-no-destination channel-details channel-create channel-leave channel-leave-last channel-join channel-join-composer
  new-dm-directory new-dm-directory-loading new-dm-directory-error new-dm-directory-empty new-dm-directory-error-narrow
  local-drafts local-drafts-loading local-drafts-error local-drafts-error-narrow
  new-dm new-dm-selected new-dm-pending new-dm-error new-dm-narrow
  profile-hover-card profile-hover-card-narrow profile-pane profile-pane-narrow profile-pane-min-width-narrow profile-dm-opening profile-vip-pending profile-pane-rich-fields
  profile-loading profile-loading-narrow
  profile-pane-min-width dm-header-compact
  channel-thread-profile-headers channel-huddle huddle-connected huddle-invite
  huddle-video huddle-video-expanded huddle-settings huddle-people huddle-captions
  chat-paused-pill
  thread-follow-on thread-follow-off thread-follow-pending thread-follow-narrow
  thread-feed thread-feed-empty thread-feed-error thread-feed-loading thread-feed-updated thread-feed-narrow thread-feed-refresh-retry thread-feed-refresh-retry-narrow thread-feed-older-retry thread-feed-older-retry-narrow thread-feed-initial-retry thread-feed-initial-retry-narrow
  thread-counts-mentions thread-counts-unread thread-counts-clear thread-counts-narrow
  thread-unread-divider thread-list-unread thread-marked-unread thread-marked-unread-narrow thread-target thread-error thread-paging thread-paging-error thread-grouped thread-grouped-pending thread-window-200 thread-window-2000 thread-window-10000 thread-window-target thread-window-gap main-dev settings settings-storage appearance-countertop
  appearance-blue-steel appearance-paper-bag appearance-custom-background
  image-viewer-upload image-viewer-embed-compact gif-picker-attachment
  pdf-preview pdf-preview-narrow pdf-preview-error pdf-preview-search-long
  file-share file-share-narrow file-share-dm file-share-dm-narrow file-share-group-dm file-share-group-dm-narrow file-share-new-dm file-share-new-dm-narrow file-share-pressure file-share-pressure-narrow file-share-draft-reopen
  message-file-actions message-file-actions-narrow
  viewer-actions-files viewer-actions-message viewer-actions-narrow viewer-actions-pdf
  file-actions file-actions-narrow file-actions-channel-narrow file-rename file-rename-narrow file-rename-channel-narrow file-recovery file-recovery-narrow file-preview-renamed-files file-preview-renamed-message
  original-download original-download-narrow original-download-save-dialog original-download-save-workflow
  file-preview-refresh-check file-preview-refresh-check-narrow cached-viewer-save-check
  file-preview-refresh file-preview-refresh-narrow file-preview-refresh-error file-preview-refresh-error-narrow
  file-preview-text file-preview-narrow file-preview-large file-preview-utf16 file-preview-empty file-preview-binary file-preview-audio
  video-viewer search search-opening search-loading search-empty search-error search-more-error search-narrow search-filters search-filters-narrow search-has-filters search-has-filters-narrow search-files search-files-narrow search-files-types search-files-types-narrow search-files-types-clear search-files-types-clear-narrow search-message-scopes search-message-scopes-narrow search-message-scopes-clear search-message-scopes-clear-narrow search-saved-pinned search-saved-pinned-narrow search-context-filters search-context-filters-narrow search-date search-date-narrow search-history search-history-narrow search-attachments search-attachments-narrow search-conversations search-conversations-narrow palette palette-people activity-unread activity-channel-post
  search-people-paging search-people-paging-narrow
  search-files-pages search-files-pages-narrow search-files-page-failed search-files-page-failed-narrow
  search-files-saving search-files-saving-narrow search-files-save-unconfirmed search-files-save-unconfirmed-narrow
  search-messages-pages search-messages-pages-narrow search-messages-page2 search-messages-page2-narrow
  search-messages-page-failed search-messages-page-failed-narrow
  search-people-account-type search-people-account-type-narrow search-people-account-type-deactivated search-people-account-type-deactivated-narrow
  search-people-saving search-people-saving-narrow search-people-save-unconfirmed search-people-save-unconfirmed-narrow
  search-people search-people-narrow search-people-loading search-people-loading-narrow search-people-empty search-people-empty-narrow search-people-error search-people-error-narrow
  profile-people-read-only profile-people-read-only-narrow profile-people-read-only-error profile-people-read-only-error-narrow
  activity-thread-tools activity-paging activity-loading activity-error activity-more-error activity-empty activity-narrow activity-navigation activity-navigation-narrow activity-refresh-retry activity-refresh-retry-narrow activity-older-retry activity-older-retry-narrow dms dms-cold-rows dms-cold-rows-narrow dms-paging-filtered dms-paging-filtered-narrow dms-paging-loading dms-paging-loading-narrow dms-paging-empty dms-paging-empty-narrow dms-initial-unloaded dms-initial-unloaded-narrow dms-initial-loading dms-initial-loading-narrow dms-current-facts dms-current-facts-narrow
  dm-history-failed dm-cached-refresh-failed toast-long-error
  connection-connecting connection-no-network composer-multiline
  composer-upload-progress
  edit-message-composer accounts self-menu self-menu-notifications
  multi-paragraph-custom-emoji slack-connect-avatar message-unfurl-embed block-kit-layout button-metadata button-metadata-narrow block-control-values block-control-values-narrow
  app-shortcuts app-shortcuts-narrow
  app-home app-home-narrow
  app-about app-about-narrow app-about-unknown app-about-unknown-narrow app-about-connect app-about-connect-narrow
  workflow-managed workflow-managed-narrow workflow-editor workflow-editor-narrow workflow-reply-loss workflow-reply-loss-narrow
  workflow-activity workflow-activity-narrow
  channel-initialization channel-initialization-narrow
  app-home-workflow-auth app-home-workflow-auth-narrow
  app-home-workflow-auth-held app-home-workflow-auth-held-narrow
  app-home-workflow-auth-unconfirmed app-home-workflow-auth-unconfirmed-narrow
  app-home-workflow-auth-rate-limit app-home-workflow-auth-rate-limit-narrow
  app-home-workflow app-home-workflow-narrow
  app-home-workflow-held app-home-workflow-held-narrow
  app-home-workflow-unconfirmed app-home-workflow-unconfirmed-narrow
  app-home-workflow-rate-limit app-home-workflow-rate-limit-narrow
  app-home-workflow-meeting-known app-home-workflow-meeting-known-narrow
  app-home-workflow-meeting-all-day app-home-workflow-meeting-all-day-narrow
  app-home-workflow-meeting-title-only app-home-workflow-meeting-title-only-narrow
  app-home-workflow-meeting-status-unavailable app-home-workflow-meeting-status-unavailable-narrow
  app-home-workflow-meeting-disconnected app-home-workflow-meeting-disconnected-narrow
  app-calendar app-calendar-narrow app-calendar-modern app-calendar-modern-narrow
  app-calendar-connect app-calendar-connect-narrow app-calendar-uncertain app-calendar-uncertain-narrow
  app-home-inputs app-home-inputs-narrow
  app-home-external-select app-home-external-select-narrow
  app-home-external-dispatch app-home-external-dispatch-narrow
  app-home-multi-external-dispatch app-home-multi-external-dispatch-narrow
  app-home-number-input app-home-number-input-narrow
  app-home-email-input app-home-email-input-narrow
  app-home-focus-on-load app-home-focus-on-load-narrow
  app-home-multi-channel-select app-home-multi-channel-select-narrow
  # Ordered Home channels: check-app-home-multi-channel-select-native.py.
  app-home-conversation-select app-home-conversation-select-narrow
  app-home-multi-conversation-select app-home-multi-conversation-select-narrow
  app-home-text-dispatch app-home-text-dispatch-narrow
  app-home-text-button-sync app-home-text-button-sync-narrow
  app-home-text-focus app-home-text-focus-narrow
  app-home-static-dispatch app-home-static-dispatch-narrow
  app-home-static-remount app-home-static-remount-narrow
  app-home-static-wrappers app-home-static-wrappers-narrow
  app-home-multi-static-dispatch app-home-multi-static-dispatch-narrow
  app-home-checkbox-dispatch app-home-checkbox-dispatch-narrow
  app-home-radio-dispatch app-home-radio-dispatch-narrow
  app-home-date-dispatch app-home-date-dispatch-narrow
  app-home-date-focus app-home-date-focus-narrow
  app-home-date-popup app-home-date-popup-narrow
  app-home-time-focus app-home-time-focus-narrow
  app-home-datetime-editing app-home-datetime-editing-narrow
  app-home-datetime-popup app-home-datetime-popup-narrow
  app-home-time-dispatch app-home-time-dispatch-narrow
  app-home-datetime-dispatch app-home-datetime-dispatch-narrow
  app-home-channel-select app-home-channel-select-narrow
  # Public channel Home: check-app-home-channel-select-native.py.
  app-home-user-dispatch app-home-user-dispatch-narrow
  app-home-multi-user-dispatch app-home-multi-user-dispatch-narrow
  app-home-channel-dispatch app-home-channel-dispatch-narrow
  app-home-multi-channel-dispatch app-home-multi-channel-dispatch-narrow
  app-home-conversation-dispatch app-home-conversation-dispatch-narrow
  app-home-multi-conversation-dispatch app-home-multi-conversation-dispatch-narrow
  app-home-user-select app-home-user-select-narrow
  app-home-multi-user-select app-home-multi-user-select-narrow
  # Stateful known-member coverage: check-app-home-user-select-native.py.
  # Ordered Home people: check-app-home-multi-user-select-native.py.
  app-home-multi-external-select app-home-multi-external-select-narrow
  # Stateful multi coverage: check-app-home-multi-external-select-native.py.
  app-home-unified-context app-home-unified-context-narrow
  # Stateful coverage uses check-app-home-external-select-native.py with exact closed flags.
  app-home-calendar app-home-calendar-narrow
  app-modal-form app-modal-form-narrow app-modal-lifecycle app-modal-lifecycle-narrow
  app-modal-email app-modal-email-narrow app-modal-email-error app-modal-email-error-narrow
  app-modal-url app-modal-url-narrow app-modal-url-error app-modal-url-error-narrow
  app-modal-url-prefill app-modal-url-prefill-narrow
  app-modal-required-integer app-modal-required-integer-narrow
  app-modal-signed-integer app-modal-signed-integer-narrow
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
  slack-list-board-heading slack-list-board-heading-narrow slack-list-board-heading-checked-narrow
  slack-list-hydration slack-list-hydration-narrow
  slack-list-column-rename slack-list-column-rename-narrow
  slack-list-number-precision slack-list-number-precision-narrow
  slack-list-number-rename slack-list-number-rename-narrow
  slack-list-field-visibility slack-list-field-visibility-narrow
  slack-list-view-filter slack-list-view-filter-narrow
  slack-list-select-filter slack-list-select-filter-narrow
  slack-list-number-filter slack-list-number-filter-narrow
  slack-list-compound-filter slack-list-compound-filter-narrow
  slack-list-text-field-create slack-list-text-field-create-narrow
  slack-list-number-field-create slack-list-number-field-create-narrow
  slack-list-number-precision-field-create slack-list-number-precision-field-create-narrow
  slack-list-checkbox-field-create slack-list-checkbox-field-create-narrow
  slack-list-date-field-create slack-list-date-field-create-narrow
  slack-list-select-field-create slack-list-select-field-create-narrow
  slack-list-multi-select-field-create slack-list-multi-select-field-create-narrow
  slack-list-people-field-create slack-list-people-field-create-narrow
  slack-list-link-field-create slack-list-link-field-create-narrow
  slack-list-channel-field-create slack-list-channel-field-create-narrow
  slack-list-rating-field-create slack-list-rating-field-create-narrow
  slack-list-rating-cells slack-list-rating-cells-narrow
  slack-list-people-defaults-single-create slack-list-people-defaults-single-create-narrow
  slack-list-people-defaults-multiple-create slack-list-people-defaults-multiple-create-narrow
  slack-list-people-defaults-null-create slack-list-people-defaults-null-create-narrow
  slack-list-people-defaults-omitted-create slack-list-people-defaults-omitted-create-narrow
  slack-list-channel-defaults-single-create slack-list-channel-defaults-single-create-narrow
  slack-list-channel-defaults-multiple-create slack-list-channel-defaults-multiple-create-narrow
  slack-list-channel-defaults-null-create slack-list-channel-defaults-null-create-narrow
  slack-list-channel-defaults-omitted-create slack-list-channel-defaults-omitted-create-narrow
  slack-list-vote-create-default slack-list-vote-create-default-narrow
  slack-list-vote-create-custom slack-list-vote-create-custom-narrow
  slack-list-rating-default-field-create slack-list-rating-default-field-create-narrow
  slack-list-rating-standard-field-create slack-list-rating-standard-field-create-narrow
  slack-list-rating-custom-field-create slack-list-rating-custom-field-create-narrow
  slack-list-vote-field-create slack-list-vote-field-create-narrow
  slack-list-vote-cells slack-list-vote-cells-narrow
  slack-list-vote-board slack-list-vote-board-narrow
  slack-list-vote-detail slack-list-vote-detail-narrow
  slack-list-people-avatars slack-list-people-avatars-narrow slack-list-people-avatars-board slack-list-people-avatars-board-narrow slack-list-people-avatars-detail slack-list-people-avatars-detail-narrow slack-list-people-settings slack-list-people-settings-narrow
  slack-list-channel-settings slack-list-channel-settings-narrow
  slack-list-view-sort slack-list-view-sort-narrow
  slack-list-ordered-sort slack-list-ordered-sort-narrow
  slack-list-task-date slack-list-task-date-narrow slack-list-task-reminder-route slack-list-task-reminder-route-narrow
  slack-list-task-assignment slack-list-task-assignment-narrow
  slack-list-task-assignment-recovery slack-list-task-assignment-recovery-narrow
  slack-list-todo-completion
  slack-list-todo-completion-narrow
  slack-list-todo-reminder slack-list-todo-reminder-narrow
  slack-list-field-delete slack-list-field-delete-narrow
  slack-list-view-grouping slack-list-view-grouping-narrow
  slack-list-board-grouping slack-list-board-grouping-narrow
  slack-list-board-move slack-list-board-move-narrow slack-list-board-move-checkbox slack-list-board-move-checkbox-narrow
  slack-list-find slack-list-find-narrow slack-list-find-board slack-list-find-board-narrow slack-list-find-empty slack-list-find-empty-narrow
  slack-list-view-rename slack-list-view-rename-narrow
  slack-list-board-card-drag slack-list-board-card-drag-narrow
  slack-list-board-card-cross slack-list-board-card-cross-narrow
  slack-list-board-card-tail slack-list-board-card-tail-narrow
  slack-list-board-group-delete slack-list-board-group-delete-narrow
  slack-list-board-choice slack-list-board-choice-narrow
  slack-list-board-choice-add slack-list-board-choice-add-narrow
  slack-list-board-choice-color slack-list-board-choice-color-narrow
  slack-list-board-local-order slack-list-board-local-order-narrow
  slack-list-board-refinement-save slack-list-board-refinement-save-narrow
  slack-list-board-refinement-save-new slack-list-board-refinement-save-new-narrow
  slack-list-board-refinement-save-lost slack-list-board-refinement-save-lost-narrow
  slack-list-board-refinement slack-list-board-refinement-narrow
  slack-list-board-refinement-new slack-list-board-refinement-new-narrow
  slack-list-board-lane-order slack-list-board-lane-order-narrow
  slack-list-view-order slack-list-view-order-narrow
  slack-list-view-create slack-list-view-create-narrow
  slack-list-subitem-create slack-list-subitem-create-narrow slack-list-subitem-uncertain slack-list-subitem-uncertain-narrow
  self-status-editor self-status-editor-narrow
  profile-edit profile-edit-narrow
  profile-sections profile-sections-narrow
  profile-sections-save profile-sections-save-narrow
  profile-sections-mentions profile-sections-mentions-narrow
  profile-channel-recovery profile-channel-recovery-narrow
  profile-member-recovery profile-member-recovery-narrow
  profile-member-known profile-member-known-narrow
  profile-member-freshness profile-member-freshness-narrow
  profile-group-recovery profile-group-recovery-narrow
  profile-pronunciation profile-pronunciation-narrow
  profile-recording profile-recording-narrow
  profile-recording-recovery profile-recording-recovery-narrow
  profile-recording-cleanup profile-recording-cleanup-narrow
  profile-recording-upload profile-recording-upload-narrow
  profile-recording-cancelled profile-recording-cancelled-narrow
  profile-photo profile-photo-narrow
  profile-title profile-title-narrow
  message-radio-buttons message-radio-buttons-narrow
  message-multi-controls message-multi-controls-narrow
  message-datepicker message-datepicker-narrow
  message-users-select message-users-select-narrow
  message-multi-users-select message-multi-users-select-narrow
  message-channels-select message-channels-select-narrow
  message-conversations-select message-conversations-select-narrow
  composer-emoji composer-emoji-narrow
  emoji-native emoji-native-narrow
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
  draft-update-recovery draft-update-recovery-narrow
  draft-schedule-recovery draft-schedule-recovery-narrow
  managed-draft-send managed-draft-send-narrow
  managed-draft-schedule managed-draft-schedule-narrow
  scheduled-attachments scheduled-attachments-narrow
  user-links user-links-narrow
  read-preferences read-preferences-narrow
  read-navigation read-navigation-narrow
  reading-shortcuts reading-shortcuts-narrow
  bulk-read bulk-read-narrow
  rejected-send rejected-send-narrow
  media-loading-state media-failed timeline-variable-height timeline-latest-opening
  usergroup-members usergroup-channels system-messages send-outcomes send-outcomes-narrow send-outcomes-thread managed-draft-recovery managed-draft-recovery-narrow message-marked-unread message-marked-unread-narrow member-live-refresh channel-members member-add member-add-error member-remove member-add-narrow member-remove-narrow
)

# Owned workflows require their dedicated runner and explicit ownership/readiness.
# Keep this one classifier authoritative for both default selection and refusal.
owned_fixture_route() {
  owned_route=""
  case "$1" in
    ordinary-history-cursor|ordinary-history-cursor-narrow)
      owned_route="Use python scripts/check-ordinary-history-cursor-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner owns disconnected page releases and capture ACKs."
      ;;
    reconnect-unread-opening|reconnect-unread-opening-narrow)
      owned_route="Use python scripts/check-reconnect-unread-opening-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner owns disconnected history releases and capture ACKs."
      ;;
    reconnect-thread-continuation|reconnect-thread-continuation-narrow)
      owned_route="Use python scripts/check-reconnect-thread-continuation-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner owns disconnected read releases and capture ACKs."
      ;;
    later-cold-message-navigation|later-cold-message-navigation-narrow)
      owned_route="Use python scripts/check-later-cold-message-navigation-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner owns offline metadata releases and capture ACKs."
      return 0 ;;
    canvas-owner-first-pair-delete|canvas-owner-first-pair-delete-narrow)
      owned_route="Use python scripts/check-canvas-owner-first-pair-delete-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner initializes both retained histories, private journal and capture ACKs."
      return 0 ;;
    canvas-retained-pair-delete|canvas-retained-pair-delete-narrow)
      owned_route="Use python scripts/check-canvas-retained-pair-delete-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner initializes both retained histories, private journal and capture ACKs."
      return 0 ;;
    canvas-retirement-cancellation|canvas-retirement-cancellation-narrow)
      owned_route="Use the isolated canvas-retirement-cancellation passive checker with its private journal and capture ACKs; the legacy Canvas launcher is not supported."
      return 0 ;;
    canvas-confirmed-provenance|canvas-confirmed-provenance-narrow)
      owned_route="Use the confirmed-provenance milestone isolated native checker with linked closed DB/snapshot phases and passive 250ms guards."
      ;;
    canvas-last-live-replacement|canvas-last-live-replacement-narrow)
      owned_route="Use the milestone isolated check-canvas-last-live-replacement-native.py with a verified binary, fresh captures and private journal; this fixture requires passive 250ms readiness and capture-edge guards."
      return 0 ;;
    canvas-live-split-child-delete|canvas-live-split-child-delete-narrow)
      owned_route="Use python scripts/check-canvas-live-split-child-delete-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner initializes the saved pair, private journal and capture ACKs."
      ;;
    canvas-coalesced-owner-rich|canvas-coalesced-owner-rich-narrow)
      owned_route="Use python scripts/check-canvas-coalesced-owner-rich-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner initializes parent coalescence, private journal and capture ACKs."
      ;;
    canvas-split-gap|canvas-split-gap-narrow)
      owned_route="Use python scripts/check-canvas-split-gap-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner initializes the saved pair, private journal and capture ACKs."
      ;;
    canvas-live-split-rich|canvas-live-split-rich-narrow)
      owned_route="Use python scripts/check-canvas-live-split-rich-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner initializes the saved live pair, private journal and capture ACKs."
      return 0 ;;
    canvas-owner-delete|canvas-owner-delete-narrow)
      owned_route="Use python scripts/check-canvas-owner-delete-native.py --binary <verified-binary> --captures <new-absolute-directory> --fixture $1; this runner initializes the offline snapshot, journal and capture ACKs. It also owns --coalesced and recovery scenarios."
      return 0 ;;
    canvas-editor-recovery|canvas-editor-recovery-narrow)
      owned_route="Use scripts/check-canvas-editor-recovery-native.py with a protected-native-fixture binary for synthetic-session, no-HTTP interaction checks."
      return 0 ;;
    canvas-file-fallback|canvas-file-fallback-narrow)
      owned_route="Use scripts/check-canvas-file-fallback-native.py with a protected-native-fixture binary for offline interaction checks."
      return 0 ;;
    canvas-root-attribution|canvas-root-attribution-narrow)
      owned_route="Use scripts/check-canvas-root-attribution-native.py with a protected-native-fixture binary for synthetic-session, no-HTTP interaction checks."
      return 0 ;;
    app-home-workflow-auth|app-home-workflow-auth-narrow|app-home-workflow-auth-held|app-home-workflow-auth-held-narrow|app-home-workflow-auth-unconfirmed|app-home-workflow-auth-unconfirmed-narrow|app-home-workflow-auth-rate-limit|app-home-workflow-auth-rate-limit-narrow|app-home-workflow|app-home-workflow-narrow|app-home-workflow-held|app-home-workflow-held-narrow|app-home-workflow-unconfirmed|app-home-workflow-unconfirmed-narrow|app-home-workflow-rate-limit|app-home-workflow-rate-limit-narrow|app-home-workflow-meeting-*)
      owned_route="Requires the protected-test-support workflow native driver; an initial static capture does not verify the workflow."
      return 0 ;;
    slack-list-people-defaults-single-create|slack-list-people-defaults-single-create-narrow|slack-list-people-defaults-multiple-create|slack-list-people-defaults-multiple-create-narrow|slack-list-people-defaults-null-create|slack-list-people-defaults-null-create-narrow|slack-list-people-defaults-omitted-create|slack-list-people-defaults-omitted-create-narrow|slack-list-channel-defaults-single-create|slack-list-channel-defaults-single-create-narrow|slack-list-channel-defaults-multiple-create|slack-list-channel-defaults-multiple-create-narrow|slack-list-channel-defaults-null-create|slack-list-channel-defaults-null-create-narrow|slack-list-channel-defaults-omitted-create|slack-list-channel-defaults-omitted-create-narrow)
      owned_route="Use the sealed configured Entity creation runner with owned captures and readiness."
      return 0 ;;
    slack-list-vote-create-default|slack-list-vote-create-default-narrow|slack-list-vote-create-custom|slack-list-vote-create-custom-narrow)
      owned_route="Use the sealed Vote field-create runner with owned captures and readiness."
      return 0 ;;
    slack-list-rating-default-field-create|slack-list-rating-default-field-create-narrow|slack-list-rating-standard-field-create|slack-list-rating-standard-field-create-narrow|slack-list-rating-custom-field-create|slack-list-rating-custom-field-create-narrow)
      owned_route="Use the sealed Rating field-create runner with owned captures and trusted keyboard actions."
      return 0 ;;
    slack-list-view-order|slack-list-view-order-narrow)
      owned_route="Use the sealed saved-view ordering runner with owned captures and readiness."
      return 0 ;;
    slack-list-field-delete|slack-list-field-delete-narrow)
      owned_route="Use the sealed field deletion runner with owned captures and readiness."
      return 0 ;;
    slack-list-view-grouping|slack-list-view-grouping-narrow|slack-list-board-grouping|slack-list-board-grouping-narrow)
      owned_route="Use the sealed existing-view Group by runner with owned captures and readiness."
      return 0 ;;
    cached-viewer-save-check)
      owned_route="Use the sealed cached-viewer Save runner with exact private destinations and UIA reviews."
      return 0 ;;
    file-preview-refresh-check|file-preview-refresh-check-narrow)
      owned_route="Use the sealed file-preview refresh runner with its owned marker and Save sentinel."
      return 0 ;;
    file-share|file-share-narrow|file-share-dm|file-share-dm-narrow|file-share-group-dm|file-share-group-dm-narrow|file-share-new-dm|file-share-new-dm-narrow|file-share-draft-reopen|file-share-pressure|file-share-pressure-narrow)
      owned_route="Use the sealed Share native runner with an owned capability marker and private journals."
      return 0 ;;
    original-download-save-workflow)
      owned_route="Use scripts/check-save-workflow-windows.py for the owned actual Save workflow."
      return 0
      ;;
    canvas-canonical-file-roots|canvas-canonical-file-roots-narrow|canvas-canonical-file-custody|canvas-canonical-file-custody-narrow)
      owned_route="Use scripts/check-canvas-canonical-files-native.py with a protected-native-fixture binary for offline interaction checks."
      ;;
    canvas-canonical-native-canvas|canvas-canonical-native-canvas-narrow|canvas-canonical-native-list|canvas-canonical-native-list-narrow)
      owned_route="Use scripts/check-canvas-canonical-native-destinations.py with a protected-native-fixture binary for offline interaction checks."
      ;;
    original-download-save-dialog)
      owned_route="Use scripts/check-save-dialog-discovery-windows.py for the owned native Save dialog diagnostic."
      return 0
      ;;
    original-download|original-download-narrow)
      owned_route="Use scripts/check-original-download-native.py for the owned local streaming workflow."
      return 0 ;;
    slack-list-subitem-create|slack-list-subitem-create-narrow|slack-list-subitem-uncertain|slack-list-subitem-uncertain-narrow)
      owned_route="Use scripts/check-slack-list-subitem-native.py for the owned subitem workflow."
      return 0 ;;
    protected-owned-baseline|protected-owned-baseline-narrow)
      owned_route="Requires the sealed configured-backend baseline driver and an explicitly feature-built binary."
      return 0 ;;
    protected-owned-slash|protected-owned-slash-narrow)
      owned_route="Requires the sealed Slash fixture driver; native acceptance runner is not yet prepared."
      return 0 ;;
    protected-adoption|protected-adoption-narrow)
      owned_route="Use scripts/check-protected-adoption-native.py with an explicitly feature-built binary."
      return 0
      ;;
    protected-enrollment|protected-enrollment-narrow)
      owned_route="Use scripts/check-protected-enrollment-native.py with an explicitly feature-built binary."
      return 0
      ;;
    protected-owned-send|protected-owned-send-narrow)
      owned_route="Use scripts/check-protected-owned-send-native.py with an explicitly feature-built binary."
      return 0 ;;
    protected-drafts|protected-drafts-narrow)
      owned_route="Use scripts/check-protected-drafts-native.py with an explicitly feature-built binary."
      return 0 ;;
    *) return 1 ;;
  esac
}

if [[ "$#" -gt 0 ]]; then
  FIXTURES=("$@")
else
  # Preserve the complete catalog above, but run only ordinary static fixtures.
  static_fixtures=()
  for fixture in "${FIXTURES[@]}"; do
    if owned_fixture_route "$fixture"; then
      printf 'agent-ui-check: excluded from static sweep: %s. %s\n' "$fixture" "$owned_route" >&2
    else
      static_fixtures+=("$fixture")
    fi
  done
  FIXTURES=("${static_fixtures[@]}")
  printf 'agent-ui-check: static fixture sweep (%s fixtures); owned workflows are routed separately.\n' "${#FIXTURES[@]}"
fi

# Preflight the whole explicit selection before building or launching anything.
for fixture in "${FIXTURES[@]}"; do
  if owned_fixture_route "$fixture"; then
    printf '%s\n' "$owned_route" >&2
    exit 2
  fi
done

echo "agent-ui-check: building the locked Dioxus desktop shellâ€¦"
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

  echo "agent-ui-check: launching ${fixture}â€¦"
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

# Plain member Agentforce access: check-list-plain-member-agents-native.py --case own-edit-plain-member-agents

# Plain group Save: check-list-plain-group-save-native.py --case own-edit-plain-group (normal/narrow).

# Mixed same-label Save: check-list-plain-mixed-save-native.py --case own-edit-plain-mixed-{tie,group} (normal/narrow).

# Native Enter split: check-canvas-split-native.py --scenario success|stale|partial --fixture canvas-structure[-narrow] --binary <built-binary>.

# profile-sections-mentions[-narrow]: check-profile-missing-usergroup-title-native.py drives shared missing group titles (positive/omission/failure/cancel/retirement/restricted).

# Profile private Canvas references: scripts/check-profile-canvas-unavailable-native.py
# Exact files.info status carriers; template recovery, sticky access, failure, Cancel, and retirement.
