---
id: DISK-SCREENS-DOC
title: Disk screens — Assistant, Space map, Large files, Duplicates, History, Settings
owner: Selvakumar Esra
priority: high
snapshot: specs/disk-cleanup/snapshot.html
tokens: specs/disk-cleanup/tokens.css
version: 1
---

<!-- id: DISK-SCREENS-DOC -->
# Disk screens

This spec covers the six sidebar screens that `specs/disk-cleanup.md` left as placeholders (REQ-DCLEAN-032): Assistant, Space map, Large files, Duplicates, History and Settings. They come from the same Claude Design snapshot (`specs/disk-cleanup/snapshot.html`), which is the pixel reference. Where the snapshot's prototype logic is silent, this spec fills the gap with a stated default and marks it "gap-fill".

Every rule in `specs/disk-cleanup.md` that governs the whole GUI also governs these screens:
- the engine boundary (REQ-DCLEAN-001, 027);
- size formatting (REQ-DCLEAN-028);
- control states, keyboard and accessibility (REQ-DCLEAN-012 to 016 and 020 to 025);
- the fixed window (REQ-DCLEAN-026);
- latency budgets (REQ-DCLEAN-030);
- tokens only (REQ-DCLEAN-031);
- the app shell and navigation blocking (REQ-DCLEAN-011, 032).

The CLI stays canonical (AGENTS.md "Surface scope"). Each new engine capability ships with a CLI command and a `--format json` envelope, and the GUI only calls the engine.

Non-goals:
- **Agent CLIs don't write text.** The Assistant never sends file contents anywhere, and in this version it doesn't call the selected agent CLI at all. Suggestion text is produced by the engine. The selected agent is recorded and named in the UI.
- **No background service.** Scheduled scans, proactive checks and below-goal notifications run only while the disky app is running. No launchd agent is installed.
- **No automatic cleaning.** Nothing on any screen cleans without an explicit user action, and every removal on these screens goes to the Trash.

<!-- id: REQ-DSCR-001 -->
## Preferences MUST persist in one store shared by the CLI and the GUI

The engine owns one preferences store in disky's data directory. It holds:
- trash-by-default, skip-recently-used, safe-only preselection and warn-below-goal (the four Settings toggles);
- the space goal in bytes;
- proactive checks on/off and their frequency (`hourly`, `daily` or `weekly`);
- the selected agent id;
- the scheduled-scan period (`off`, `weekly` or `monthly`);
- the excluded folders.

The defaults are the snapshot's initial state:
- toggles: all on;
- space goal: 80 GB;
- proactive checks: on, `daily`;
- agent: `claude`;
- scheduled scan: `weekly`;
- excluded folders: none.

implementations:
  - src/prefs.rs:Prefs
  - src/prefs.rs:Checks
  - src/prefs.rs:Frequency
  - src/prefs.rs:Schedule
  - src/prefs.rs:prefs_path
  - src/prefs.rs:load
  - src/prefs.rs:save
  - src/prefs.rs:update
  - src/prefs.rs:set
  - src/prefs.rs:set_all
  - src/prefs.rs:apply
  - src/prefs.rs:parse_bytes
  - src/history.rs:Lock
  - src/render.rs:prefs
  - src/contract.rs:load
  - src/contract.rs:empty_view
  - src/contract.rs:cleanup_view
  - gui/src-tauri/src/main.rs:get_prefs
  - gui/src-tauri/src/main.rs:set_prefs
  - gui/src-tauri/src/main.rs:load_cleanup_view
  - gui/src/lib/contract.ts:parsePreferences
  - gui/src/lib/engine.ts:getPrefs
  - gui/src/lib/engine.ts:setPrefs

verifies:
  - src/prefs.rs:missing_file_yields_snapshot_defaults
  - src/prefs.rs:malformed_file_is_an_error_and_never_replaced
  - src/prefs.rs:set_then_load_round_trips
  - src/prefs.rs:bad_key_or_value_is_usage_and_changes_nothing
  - src/prefs.rs:save_is_atomic_and_leaves_no_temp_file
  - src/prefs.rs:concurrent_writers_never_truncate_or_lose_the_file
  - src/prefs.rs:every_enum_value_parses
  - src/prefs.rs:byte_suffixes_are_decimal_and_binary
  - src/prefs.rs:json_shape_uses_snake_case_keys_and_values
  - src/contract.rs:view_goal_and_preferences_come_from_the_store
  - src/schema.rs:prefs_command_lists_every_key
  - tests/agentic.rs:prefs_prints_every_key_with_its_default
  - tests/agentic.rs:prefs_set_persists_and_is_shared_with_the_library
  - tests/agentic.rs:prefs_bad_key_or_value_exits_usage_and_changes_nothing
  - tests/agentic.rs:prefs_malformed_store_is_reported_not_replaced
  - tests/agentic.rs:schema_lists_the_prefs_command
  - gui/src/screens/__tests__/Settings.test.tsx:aFailedSaveIsReportedAndShowsTheStore
  - gui/src/screens/__tests__/Settings.test.tsx:aMalformedStoreIsReportedNotDefaulted

**Acceptance**
<!-- id: REQ-DSCR-001.A1 -->
- `disky prefs --format json` prints every preference. `disky prefs --set <key>=<value>` changes one and prints the result. An unknown key or invalid value exits 2 (usage) and changes nothing.
<!-- id: REQ-DSCR-001.A2 -->
- A preference changed in the GUI is what `disky prefs` reads next, and the reverse: one store, no copies.
<!-- id: REQ-DSCR-001.A3 -->
- A missing preferences file yields the defaults. A malformed file is reported as an error and never silently replaced.
<!-- id: REQ-DSCR-001.A4 -->
- Writes are atomic (temp file + rename), so a concurrent CLI and GUI write never leaves a truncated file.
<!-- id: REQ-DSCR-001.A5 -->
- The GUI reads the space goal, trash-by-default, safe-only, warn-below-goal and proactive-checks values from this store. It no longer uses hard-coded defaults (supersedes the defaults stated in `specs/disk-cleanup.md`).

<!-- id: REQ-DSCR-002 -->
## Excluded folders and skip-recently-used MUST be honoured by cleanup and every screen's results

implementations:
  - src/cleanup.rs:ScanContext.with_prefs
  - src/cleanup.rs:scan_with
  - src/cleanup.rs:in_scope
  - src/prefs.rs:Prefs.expanded_excluded
  - src/prefs.rs:Prefs.recent_cutoff
  - src/contract.rs:large_files_count_in
  - src/contract.rs:load

verifies:
  - tests/lib_integration.rs:cleanup_drops_excluded_and_recently_used_hits
  - tests/lib_integration.rs:contract_load_honours_the_preferences_store
  - tests/agentic.rs:cleanup_honours_excluded_folders_and_skip_recently_used
  - tests/agentic.rs:skip_recently_used_keeps_fresh_safe_categories
  - src/contract.rs:large_files_count_honours_exclusions_and_recent_use
  - src/prefs.rs:excluded_expands_home_and_recent_cutoff_follows_the_toggle

**Acceptance**
<!-- id: REQ-DSCR-002.A1 -->
- No cleanup hit, large file, duplicate copy or suggestion lies inside an excluded folder.
<!-- id: REQ-DSCR-002.A2 -->
- With skip-recently-used on, items modified in the last 30 days are left out of review-category cleanup hits, large files and duplicates. With it off, they are included. Safe categories (caches, temp files, logs, Trash) are rebuilt automatically and touched constantly, so they are exempt.
<!-- id: REQ-DSCR-002.A3 -->
- An excluded folder that doesn't exist is kept in the list and simply matches nothing.

<!-- id: REQ-DSCR-003 -->
## The engine MUST report a folder's immediate children by size for the space map

implementations:
  - src/map.rs:MapChild
  - src/map.rs:MapView
  - src/map.rs:map
  - src/map.rs:normalize
  - src/map.rs:MapOther
  - src/contract.rs:map_view
  - src/render.rs:map
  - src/cli.rs:Command
  - src/schema.rs:commands
  - gui/src-tauri/src/main.rs:load_map
  - gui/src/lib/contract.ts:parseMapView
  - gui/src/lib/engine.ts:loadMap

verifies:
  - src/map.rs:map_lists_immediate_children_largest_first
  - src/map.rs:map_drills_into_a_child_folder
  - src/map.rs:map_outside_the_snapshot_is_not_found
  - src/map.rs:map_leaves_out_excluded_folders
  - src/map.rs:map_of_the_filesystem_root
  - src/map.rs:map_json_shape
  - src/map.rs:fold_keeps_the_largest_and_sums_the_rest_into_other
  - src/map.rs:fold_moves_slivers_under_the_share_into_other
  - src/map.rs:fold_leaves_a_small_folder_untouched
  - tests/lib_integration.rs:map_is_fast_on_large_snapshot
  - tests/lib_integration.rs:map_view_honours_excluded_folders
  - tests/agentic.rs:map_lists_children_largest_first_and_outside_exits_not_found
  - tests/agentic.rs:schema_lists_the_map_command_and_history_rows
  - gui/src/lib/contract.test.ts:parsesAMapViewAndRejectsBadBytes

**Acceptance**
<!-- id: REQ-DSCR-003.A1 -->
- `disky map [<path>] --snapshot <ref> --format json` returns the folder's immediate children, each with:
  - name, absolute path and physical bytes (the sum of everything beneath it);
  - whether it has children of its own.

  Children are sorted by bytes, largest first. The default path is the snapshot's scan root.
<!-- id: REQ-DSCR-003.A2 -->
- The envelope carries the folder's own total and the volume capacity, so a share of disk can be shown without the GUI summing anything.
<!-- id: REQ-DSCR-003.A3 -->
- A path outside the snapshot exits 4 (not-found).
<!-- id: REQ-DSCR-003.A4 -->
- On a 1 M-row snapshot one call returns in under 1 s (release build).
<!-- id: REQ-DSCR-003.A5 -->
- `--tiles <N>` folds the view for drawing. The largest children are kept, at most N − 1 of them, and any child under 0.5% of the folder is dropped from that list.
  - Every dropped child is summed into one `other: {count, bytes}`. The bytes of the kept children plus `other.bytes` equal `total_bytes`.
  - A view with nothing to fold, or a single child that would be folded, has `other: null`. A lone child is never renamed "Other".
  - Without `--tiles`, every child is listed. The GUI always asks for at most 20 blocks.

<!-- id: REQ-DSCR-004 -->
## The engine MUST list large files with their last-opened time

implementations:
  - src/contract.rs:on_disk_only
  - src/db.rs:FileRecord
  - src/db.rs:create_schema
  - src/db.rs:append_batch
  - src/scan.rs:run_observed
  - src/large.rs:LargeFilter
  - src/large.rs:LargeFile
  - src/large.rs:LargeView
  - src/large.rs:has_atime
  - src/large.rs:is_unused
  - src/large.rs:large
  - src/contract.rs:large_view
  - src/render.rs:large
  - src/cli.rs:Command
  - src/schema.rs:commands
  - gui/src-tauri/src/main.rs:load_large
  - gui/src/lib/contract.ts:parseLargeView
  - gui/src/lib/engine.ts:loadLarge

verifies:
  - src/contract.rs:large_view_reads_the_snapshot_with_prefs
  - src/large.rs:large_lists_files_of_2gb_and_up_largest_first
  - src/large.rs:large_filters_over_10gb_and_unused_1y
  - src/large.rs:unused_1y_is_strictly_more_than_365_days
  - src/large.rs:large_honours_exclusions_and_skip_recently_used
  - src/large.rs:snapshot_without_atime_lists_null_last_opened
  - src/large.rs:filter_parses_and_serialises_kebab_case
  - src/contract.rs:large_view_reads_the_snapshot_with_prefs
  - tests/lib_integration.rs:scan_records_each_files_access_time
  - tests/agentic.rs:large_lists_files_over_2gb_with_filters_and_counts
  - tests/agentic.rs:large_bad_filter_exits_usage_and_unknown_snapshot_exits_not_found
  - tests/agentic.rs:schema_lists_the_large_and_trash_commands
  - gui/src/lib/contract.test.ts:parsesALargeViewAndRejectsBadFields

**Acceptance**
<!-- id: REQ-DSCR-004.A1 -->
- Scans record each file's last access time.
- `disky large --snapshot <ref> --format json [--filter all|over-10gb|unused-1y]` lists regular files of at least 2 GB physical, largest first. Each file has:
  - name and absolute path;
  - physical bytes;
  - last-opened time (RFC 3339);
  - an `unused_1y` flag, true when the last-opened time is more than 365 days ago.
<!-- id: REQ-DSCR-004.A2 -->
- `over-10gb` keeps files of at least 10 GB, and `unused-1y` keeps flagged files. The envelope carries the count and total bytes of the filtered list.
  - A file deleted since the scan is left out, because trashing it could only fail. The Smart Clean shortcut counts the same list.
<!-- id: REQ-DSCR-004.A3 -->
- A snapshot taken before access times were recorded lists files with a null last-opened time and `unused_1y` false, rather than failing.

<!-- id: REQ-DSCR-005 -->
## The engine MUST find duplicate files by content

implementations:
  - src/dupes.rs:DupeCopy
  - src/dupes.rs:DupeGroup
  - src/dupes.rs:DupesView
  - src/dupes.rs:dupes
  - src/dupes.rs:candidates
  - src/dupes.rs:recheck
  - src/dupes.rs:prefix_hash
  - src/dupes.rs:full_hash
  - src/dupes.rs:group
  - src/large.rs:scope_clause
  - src/contract.rs:dupes_view
  - src/render.rs:dupes
  - src/cli.rs:Command
  - src/cli.rs:parse_min_size
  - src/schema.rs:commands
  - gui/src-tauri/src/main.rs:load_dupes
  - gui/src/lib/contract.ts:parseDupesView
  - gui/src/lib/engine.ts:loadDupes

verifies:
  - src/dupes.rs:same_content_groups_name_alone_never_does
  - src/dupes.rs:below_min_size_ignored
  - src/dupes.rs:copies_ordered_oldest_mtime_first
  - src/dupes.rs:groups_sorted_by_reclaimable_with_totals
  - src/dupes.rs:changed_or_vanished_file_dropped_and_group_below_two_omitted
  - src/dupes.rs:hard_links_count_once
  - src/dupes.rs:excluded_folder_left_out
  - src/dupes.rs:recently_modified_left_out_when_skip_recently_used
  - src/dupes.rs:prefix_prefilter_spares_the_full_hash
  - src/dupes.rs:full_hash_decides_past_the_prefix
  - src/dupes.rs:unreadable_file_is_dropped
  - src/dupes.rs:home_shown_as_tilde_in_location
  - src/contract.rs:dupes_view_reads_the_snapshot_with_prefs
  - src/schema.rs:dupes_command_is_listed_with_its_record
  - tests/agentic.rs:dupes_cli_json_envelope
  - tests/agentic.rs:dupes_bad_min_size_exits_2
  - tests/agentic.rs:schema_lists_the_dupes_command
  - gui/src/lib/contract.test.ts:parsesADupesViewAndRejectsBadGroups

**Acceptance**
<!-- id: REQ-DSCR-005.A1 -->
- `disky dupes --snapshot <ref> --format json [--min-size <bytes>]` groups regular files whose contents are byte-identical. It compares sizes first, then a full content hash, and never decides by name alone. Each group has:
  - a display name (the first copy's basename);
  - the bytes of each copy;
  - every copy's absolute path, oldest modification time first.

  The default minimum size is 1 MB.
<!-- id: REQ-DSCR-005.A2 -->
- Groups are sorted by reclaimable bytes (bytes × (copies − 1)), largest first. The envelope carries the group count, the extra-copy count and the reclaimable total.
<!-- id: REQ-DSCR-005.A3 -->
- A file that has changed or vanished since the scan is re-checked and left out of its group. A group that drops below two copies is omitted.
<!-- id: REQ-DSCR-005.A4 -->
- Hard links to the same inode count as one file, never as duplicates.

<!-- id: REQ-DSCR-006 -->
## The engine MUST move reviewed files to the Trash and record a titled history entry

implementations:
  - src/contract.rs:TrashResult
  - src/contract.rs:trash_reviewed
  - src/contract.rs:user_trash_root
  - src/contract.rs:physical_bytes_on_disk
  - src/contract.rs:trash_dupes
  - src/cleanup.rs:apply_observed
  - src/render.rs:trash
  - src/cli.rs:Command
  - src/schema.rs:commands
  - gui/src-tauri/src/main.rs:trash_large
  - gui/src/lib/contract.ts:parseTrashResult
  - gui/src/lib/engine.ts:trashLarge
  - gui/src-tauri/src/main.rs:trash_dupes
  - gui/src/lib/engine.ts:trashDupes

verifies:
  - src/contract.rs:trash_reviewed_moves_listed_files_and_writes_one_titled_entry
  - src/contract.rs:trash_reviewed_refuses_a_path_not_listed
  - src/contract.rs:trash_reviewed_refuses_every_copy_of_a_group
  - src/contract.rs:trash_dupes_keeps_one_copy_of_every_group
  - tests/agentic.rs:trash_moves_named_paths_and_records_one_titled_entry
  - tests/agentic.rs:schema_lists_the_large_and_trash_commands
  - gui/src/lib/contract.test.ts:parsesATrashResult

**Acceptance**
<!-- id: REQ-DSCR-006.A1 -->
- `disky trash <path>... --title <title> --format json` moves each path to the Trash. It reports the handled paths, the failed paths with reasons, and the physical bytes freed, then appends one history entry with that title (REQ-DCLEAN-034).
<!-- id: REQ-DSCR-006.A2 -->
- The GUI can trash only paths the engine listed in the large-files or duplicates result the user is looking at. Any other path is refused, exactly as `clean` refuses unreviewed paths (REQ-DCLEAN-029.A1).
<!-- id: REQ-DSCR-006.A3 -->
- Trashing every copy of a duplicate group is refused. At least one copy always stays.
<!-- id: REQ-DSCR-006.A4 -->
- Tests use a temporary Trash root and log, never the real `~/.Trash` or history.

<!-- id: REQ-DSCR-007 -->
## The engine MUST detect which agent CLIs are installed

implementations:
  - src/agents.rs:AGENTS
  - src/agents.rs:Agent
  - src/agents.rs:display_name
  - src/agents.rs:search_dirs
  - src/agents.rs:detect_in
  - src/agents.rs:detect
  - src/render.rs:agents
  - src/cli.rs:Command
  - src/schema.rs:commands
  - gui/src/lib/contract.ts:parseAgent

verifies:
  - src/agents.rs:agents_are_listed_in_order_with_display_names
  - src/agents.rs:search_covers_path_then_the_extra_folders
  - src/agents.rs:detect_finds_executables_and_never_runs_them
  - src/agents.rs:the_first_folder_holding_the_cli_wins_and_folders_are_skipped
  - src/agents.rs:json_shape_is_snake_case
  - tests/agentic.rs:agents_lists_three_in_order_and_never_runs_them
  - tests/agentic.rs:schema_lists_the_agents_and_suggest_commands
  - src/schema.rs:agents_and_suggest_commands_are_listed_with_their_records

**Acceptance**
<!-- id: REQ-DSCR-007.A1 -->
- `disky agents --format json` lists Claude Code (`claude`), Codex CLI (`codex`) and Gemini CLI (`gemini`), in that order. Each has `found` and, when found, the resolved path. Lookup covers `PATH` plus `~/.local/bin`, `/opt/homebrew/bin` and `/usr/local/bin`.
<!-- id: REQ-DSCR-007.A2 -->
- Detection only checks for an executable file. It never runs the CLI.

<!-- id: REQ-DSCR-008 -->
## The engine MUST produce the Assistant's suggestions from the latest snapshot and the preferences

implementations:
  - src/assistant.rs:Suggestion
  - src/assistant.rs:SuggestionKey
  - src/assistant.rs:suggestions
  - src/assistant.rs:format_bytes
  - src/contract.rs:suggestions_for
  - src/contract.rs:assistant_inputs
  - src/cleanup.rs:CleanupHit
  - src/render.rs:suggestions
  - src/cli.rs:Command
  - src/schema.rs:commands
  - gui/src/lib/contract.ts:parseSuggestion

verifies:
  - src/assistant.rs:goal_triggers_below_the_goal_and_offers_the_safe_categories
  - src/assistant.rs:ok_triggers_at_or_above_the_goal_with_no_action
  - src/assistant.rs:dev_names_the_projects_their_folder_and_month
  - src/assistant.rs:dev_without_known_age_or_shared_folder_still_states_real_figures
  - src/assistant.rs:dl_lists_downloads_untouched_for_three_months
  - src/assistant.rs:left_names_the_uninstalled_apps
  - src/assistant.rs:a_category_with_no_hits_produces_no_suggestion
  - src/assistant.rs:no_snapshot_returns_only_goal_or_ok_and_goal_has_no_action
  - src/assistant.rs:every_category_id_comes_from_the_engine_table
  - src/assistant.rs:json_shape_uses_snake_case_keys
  - src/assistant.rs:bytes_are_formatted_as_the_gui_does
  - src/contract.rs:suggestions_leave_out_hits_gone_since_the_scan
  - tests/agentic.rs:suggest_reports_goal_or_ok_then_category_suggestions

**Acceptance**
<!-- id: REQ-DSCR-008.A1 -->
- `disky suggest --format json` returns suggestions in this order. Each has a key, a kind, its text, its bytes and its category ids.
  - **`goal`** (kind "Goal"), when free space is below the goal: offers the safe categories' total.
  - **`ok`** (kind "On track"), when free space is at or above the goal: no action, no bytes.
  - **`dev`** (kind "Suggestion"), when the developer categories have hits.
  - **`dl`** (kind "Suggestion"), when old downloads have hits.
  - **`left`** (kind "Heads-up"), when app leftovers have hits.
<!-- id: REQ-DSCR-008.A2 -->
- Text follows the snapshot's sentences with real figures. For example, the goal text reads "Free space is down to <free>, under your <goal> goal. I found <safe> in safe categories: caches, temp files, logs and the Trash. Shall I clean them?". No figure is invented: every number comes from the snapshot or the disk.
<!-- id: REQ-DSCR-008.A3 -->
- A category with no hits produces no suggestion. With no snapshot, only `goal` or `ok` is returned, and `goal` carries no action.

<!-- id: REQ-DSCR-009 -->
## The Assistant screen MUST show the suggestion feed and MUST only ever open a review

implementations:
  - gui/src-tauri/src/main.rs:cached_load
  - src/contract.rs:assistant_view_from
  - src/contract.rs:view_key
  - gui/src/screens/Assistant.tsx:Assistant
  - gui/src/screens/assistant/store.ts:visibleSuggestions
  - gui/src/screens/assistant/store.ts:actionableCount
  - gui/src/screens/assistant/store.ts:dismiss
  - gui/src/screens/assistant/assistantCopy.ts:statusLine
  - gui/src/screens/assistant/assistantCopy.ts:actionLabels
  - gui/src/screens/smartClean/phase.ts:reviewSuggestion
  - gui/src/screens/smartClean/phase.ts:reviewFromNotification
  - gui/src/lib/contract.ts:parseAssistantView
  - gui/src/lib/engine.ts:loadAssistant
  - gui/src-tauri/src/main.rs:load_assistant
  - src/contract.rs:AssistantView
  - src/contract.rs:assistant_view_of
  - src/contract.rs:assistant_view
  - gui/src/shell/AppShell.tsx:AppShell
  - gui/src/App.tsx:SCREENS

verifies:
  - gui/src/screens/__tests__/Assistant.test.tsx:reviewFailureIsShownOnItsCard
  - gui/src/screens/__tests__/Assistant.test.tsx:slowReviewShowsTheSpinnerOnItsCard
  - gui/src/screens/__tests__/Assistant.test.tsx:feedRendersTheEngineCardsWithTheAgentName
  - gui/src/screens/__tests__/Assistant.test.tsx:dismissingACardHidesIt
  - gui/src/screens/__tests__/Assistant.test.tsx:checksOffShowsTheEmptyFeedAndPaused
  - gui/src/screens/__tests__/Assistant.test.tsx:badgeAndShortcutCountTheActionableCards
  - gui/src/screens/__tests__/Assistant.test.tsx:reviewLandsOnResultsWithTheCardsCategories
  - gui/src/screens/__tests__/Assistant.test.tsx:reviewAndConfirmOpensTheDialogAndCleansAsTheAgent
  - gui/src/screens/__tests__/Assistant.test.tsx:belowGoalNotificationNamesTheAgent
  - gui/src/screens/__tests__/assistantCopy.test.ts:statusLineNamesTheAgentAndFrequencyOrPaused
  - gui/src/screens/__tests__/assistantCopy.test.ts:emptyFeedAndPrivacyCopyAreTheSnapshots
  - gui/src/screens/__tests__/assistantCopy.test.ts:buttonsAndTagsFollowTheCardKind
  - gui/src/screens/__tests__/assistantCopy.test.ts:whenReadsLikeTheSnapshot
  - gui/src/screens/__tests__/phase.test.ts:reviewSuggestionSelectsExactlyItsCategoriesAndExpandsTheFirst
  - gui/src/screens/__tests__/phase.test.ts:reviewAndConfirmOpensTheDialogPreparedByTheAgent
  - gui/src/screens/__tests__/phase.test.ts:assistantReviewsChangeNothingUnlessIdle
  - gui/src/lib/contract.test.ts:parsesAnAssistantViewAndRejectsBadSuggestions
  - gui/src/shell/__tests__/Shell.test.tsx:everyNavRouteRendersARealScreen
  - src/contract.rs:assistant_view_names_the_agent_and_checks_off_empties_the_feed
  - gui/tests/visual/assistant.spec.ts:assistantMatchesSnapshot
  - gui/tests/visual/assistant.spec.ts:assistantPassesAxe
  - gui/tests/visual/assistant.spec.ts:keyboardReviewLandsOnResultsWithTheKicker

**Acceptance**
<!-- id: REQ-DSCR-009.A1 -->
- The layout matches the snapshot's Assistant screen at 1200×780 within 1% (masked dynamic numbers):
  - **Header:** the status line "Watching · <agent> · checks <frequency>", or "Paused" when proactive checks are off, above the "Assistant" heading.
  - **Feed:** one card per suggestion. The goal card uses the accent-100 surface.
  - **Side panel:** on the right.
<!-- id: REQ-DSCR-009.A2 -->
- Each card shows the agent icon, the selected agent's name, when, the kind tag, the text and, when actionable, the primary and secondary buttons plus its size.
  - The goal card's primary is "Review & confirm" and its secondary is "Not now".
  - The others use "Review" and "Dismiss".
<!-- id: REQ-DSCR-009.A3 -->
- "Review & confirm" opens Smart Clean results with exactly the safe categories selected and the confirmation dialog open. "Review" opens results with exactly that suggestion's categories selected and the first one expanded.
  - In both cases the results kicker reads "Prepared by <agent> · review before cleaning", and the history title is "Assistant cleanup".
  - Nothing is cleaned until "Clean now".
  - While the review loads, the clicked button reads busy, can't be pressed again, and shows the spinner once the wait passes 200 ms. A failure is shown on that card, never only at the top of the feed.
  - The review reuses the view the feed was built from when the snapshot and preferences are unchanged, so it doesn't rebuild it.
<!-- id: REQ-DSCR-009.A4 -->
- "Not now" / "Dismiss" hides that card for the rest of the session.
- With proactive checks off, the feed is empty. An empty feed shows "Nothing to review. You’ll hear from the assistant when something changes."
<!-- id: REQ-DSCR-009.A5 -->
- The sidebar's Assistant entry shows a badge with the number of actionable cards, and none when zero.
- The Smart Clean shortcut reads "<n> suggestions waiting" or "Watching your space goal". The below-goal notification header names the selected agent ("Assistant · <agent>").

<!-- id: REQ-DSCR-010 -->
## The Assistant side panel MUST edit the space goal, proactive checks and agent

implementations:
  - gui/src/screens/Assistant.tsx:Assistant
  - gui/src/screens/assistant/assistantCopy.ts:goalToSlider
  - gui/src/screens/assistant/assistantCopy.ts:sliderToGoal
  - gui/src/screens/assistant/assistantCopy.ts:goalFill
  - gui/src/screens/assistant/assistantCopy.ts:goalNote
  - gui/src/screens/assistant/assistantCopy.ts:agentStatus
  - gui/src/screens/assistant/store.ts:restoreGoalCard
  - gui/src/screens/assistant/store.ts:setAssistantView
  - gui/src/lib/diskState.ts:setGoal
  - gui/src/lib/diskState.ts:setFromView

verifies:
  - gui/src/screens/__tests__/Assistant.test.tsx:changingTheGoalSavesItAndBringsTheGoalCardBack
  - gui/src/screens/__tests__/Assistant.test.tsx:checksOffShowsTheEmptyFeedAndPaused
  - gui/src/screens/__tests__/Assistant.test.tsx:frequencySaves
  - gui/src/screens/__tests__/Assistant.test.tsx:pickingAnAgentSavesItAndAMissingOneIsDisabled
  - gui/src/screens/__tests__/assistantCopy.test.ts:goalCardFiguresComeFromTheEngine
  - gui/src/screens/__tests__/assistantCopy.test.ts:agentRowsSayFoundInUseOrNotInstalled

**Acceptance**
<!-- id: REQ-DSCR-010.A1 -->
- **Space goal card:**
  - the goal tag;
  - "Keep at least <goal> free";
  - a range control from 20 to 200 GB in 5 GB steps, with an accessible name;
  - a fill bar showing free ÷ goal, capped at 100%;
  - the note "<free> free now. When it drops below the goal, the assistant prepares a cleanup and asks you first."
<!-- id: REQ-DSCR-010.A2 -->
- Changing the goal saves it to the preferences store. The meter's goal marker, the goal tag and the suggestions update without a restart, and a dismissed goal card comes back.
<!-- id: REQ-DSCR-010.A3 -->
- **Proactive checks card:**
  - a switch (`role="switch"`, `aria-checked`) with the label "Proactive checks";
  - an Hourly / Daily / Weekly segmented radio group;
  - the note "Never cleans on its own. Every cleanup needs your confirmation."

  Both controls save to the store.
<!-- id: REQ-DSCR-010.A4 -->
- **Agent card:** lists the three agents from REQ-DSCR-007, showing "Found · <path>" (plus " · in use" for the selected agent) or "Not installed".
  - A missing agent is disabled and can't be picked.
  - Picking an agent saves it.
  - The privacy note reads "Only file paths, sizes and dates are shared with the agent. File contents never leave this Mac."

<!-- id: REQ-DSCR-011 -->
## Proactive checks and scheduled scans MUST run only while the app runs and MUST never clean

implementations:
  - src/db.rs:open_read
  - gui/src-tauri/src/scheduler.rs:Scheduler
  - gui/src-tauri/src/scheduler.rs:Action
  - gui/src-tauri/src/scheduler.rs:Clock
  - gui/src-tauri/src/scheduler.rs:notification_text
  - gui/src-tauri/src/scheduler.rs:notification_title
  - gui/src-tauri/src/checks.rs:spawn
  - gui/src-tauri/src/checks.rs:run
  - gui/src-tauri/src/checks.rs:perform
  - gui/src-tauri/src/checks.rs:start_background_scan
  - gui/src-tauri/src/checks.rs:notify
  - gui/src-tauri/src/main.rs:focus_smart_clean
  - src/assistant.rs:check_period_secs
  - src/assistant.rs:scan_period_secs
  - src/assistant.rs:scan_due
  - src/assistant.rs:latest_scan_secs
  - gui/src/lib/engine.ts:onAssistantView
  - gui/src/lib/engine.ts:onBackgroundScanned
  - gui/src/lib/engine.ts:onFocusHome

verifies:
  - tests/agentic.rs:snapshot_stays_readable_while_another_reader_holds_it
  - gui/src-tauri/src/scheduler.rs:checks_reload_once_per_period_and_not_when_paused
  - gui/src-tauri/src/scheduler.rs:scheduled_scan_starts_at_launch_and_at_checks_when_the_snapshot_is_old
  - gui/src-tauri/src/scheduler.rs:no_background_scan_while_smart_clean_scans_or_cleans
  - gui/src-tauri/src/scheduler.rs:below_goal_notifies_once_per_episode
  - gui/src-tauri/src/scheduler.rs:notification_is_worded_from_engine_figures
  - gui/src-tauri/src/scheduler.rs:scheduler_actions_never_clean_trash_or_delete
  - gui/src-tauri/src/scheduler.rs:scheduler_has_no_path_to_clean_or_trash
  - src/assistant.rs:check_and_scan_periods_follow_the_preferences

**Acceptance**
<!-- id: REQ-DSCR-011.A1 -->
- With proactive checks on, the app reloads the view and suggestions at the chosen frequency (hourly, daily or weekly) while it is running.
<!-- id: REQ-DSCR-011.A2 -->
- With a scheduled scan of `weekly` or `monthly`, the app starts a background scan when the latest snapshot is older than that period. This happens at launch and at each check. Navigation is not blocked by a background scan, and it is never started while a Smart Clean scan or clean is running.
<!-- id: REQ-DSCR-011.A3 -->
- With warn-below-goal on and free space below the goal, a macOS notification is posted once per below-goal episode while the app runs. Clicking it brings the window forward on Smart Clean.
<!-- id: REQ-DSCR-011.A4 -->
- No check, scan or notification ever cleans, trashes or deletes anything.
<!-- id: REQ-DSCR-011.A5 -->
- Reading a snapshot never locks anyone else out. Background checks, the screens and the CLI read a snapshot read-only, so all of them can read it at the same time. None of them fails with `lock-held` because another one is reading. Only a scan, while it writes a snapshot, holds it exclusively.

<!-- id: REQ-DSCR-012 -->
## The Space map screen MUST show a treemap of the selected folder and let the user drill in

implementations:
  - src/contract.rs:check_on_disk
  - gui/src/screens/SpaceMap.tsx:SpaceMap
  - gui/src/screens/spaceMap/treemap.ts:layout
  - gui/src/screens/spaceMap/treemap.ts:tileColor
  - gui/src/screens/spaceMap/mapCopy.ts:crumbLine
  - gui/src/screens/spaceMap/mapCopy.ts:mapTitle
  - gui/src/screens/spaceMap/mapCopy.ts:tileLabel
  - gui/src/screens/spaceMap/mapCopy.ts:shareOfDisk
  - gui/src/screens/spaceMap/mapCopy.ts:otherDetail
  - gui/src/lib/engine.ts:revealInFinder
  - gui/src-tauri/src/main.rs:reveal_in_finder
  - gui/src/App.tsx:ROUTES

verifies:
  - src/contract.rs:a_path_gone_since_the_scan_is_explained
  - gui/tests/visual/spaceMap.spec.ts:sliverBlockStaysInsideItsSlot
  - gui/src/screens/__tests__/treemap.test.ts:matchesTheSnapshotSplitExactly
  - gui/src/screens/__tests__/treemap.test.ts:areaIsProportionalToWeight
  - gui/src/screens/__tests__/treemap.test.ts:tilesCycleTheSnapshotsSevenTokens
  - gui/src/screens/__tests__/treemap.test.ts:crumbTitleAndShareAreWorded
  - gui/src/screens/__tests__/SpaceMap.test.tsx:rendersTilesLargestSelectedOnEntry
  - gui/src/screens/__tests__/SpaceMap.test.tsx:clickSelectsAndOpenFolderOnlyWithChildren
  - gui/src/screens/__tests__/SpaceMap.test.tsx:openFolderDrillsInAndUpGoesBack
  - gui/src/screens/__tests__/SpaceMap.test.tsx:enterOnTheSelectedTileOpensIt
  - gui/src/screens/__tests__/SpaceMap.test.tsx:revealAsksTheBackendForTheSelectedPath
  - gui/src/screens/__tests__/SpaceMap.test.tsx:noSnapshotOffersStartScan
  - gui/src/screens/__tests__/SpaceMap.test.tsx:foldedFolderEndsWithAnOtherBlock
  - gui/tests/visual/spaceMap.spec.ts:crowdedFolderStaysInsideTheTreemap
  - gui/tests/visual/spaceMap.spec.ts:spaceMapMatchesSnapshot
  - gui/tests/visual/spaceMap.spec.ts:spaceMapPassesAxe
  - gui/tests/visual/spaceMap.spec.ts:keyboardDrillsIntoAFolder
  - gui/tests/visual/spaceMap.spec.ts:noSnapshotOffersStartScan

**Acceptance**
<!-- id: REQ-DSCR-012.A1 -->
- The layout matches the snapshot's Space map at 1200×780 within 1%. It has:
  - a crumb line ("Macintosh HD  /  …") and the title "<folder> · <size>";
  - an Up button, shown only below the root;
  - the treemap;
  - the "Selected" card;
  - the hint "Click a block to inspect it, double-click to open."
<!-- id: REQ-DSCR-012.A2 -->
- **Treemap:** tiles come from REQ-DSCR-003's children, laid out with the snapshot's split algorithm.
  - Each tile's area is proportional to its bytes.
  - Tile colours cycle through the snapshot's seven tile tokens.
  - Each tile shows its name and size.
  - Every tile stays inside the treemap's box, and its padding never makes it larger than its slot.
  - A tile too small to read hides its size, and then its name. A name that doesn't fit ends in an ellipsis. Its accessible name always carries both.
  - When the engine folded children (REQ-DSCR-003.A5), the last tile is "Other", labelled "Other, <n> smaller items, <size>". Selecting it shows the count in the Selected card. It offers neither Open folder nor Reveal in Finder.
<!-- id: REQ-DSCR-012.A3 -->
- **Selection:**
  - Clicking a tile selects it, and the selection gets the inset accent-700 ring.
  - Double-clicking, or "Open folder" on a tile with children, drills in.
  - Up goes back one level.
  - The largest tile is selected on entry.
<!-- id: REQ-DSCR-012.A4 -->
- **Selected card:** shows the name, full path, size and share of disk (one decimal).
  - "Open folder" appears only when the selection has children.
  - "Reveal in Finder" reveals the path in Finder via the backend. A path removed since the scan is not revealed: the screen says "<name> was removed after the last scan. Scan again to refresh the map." It never shows a raw `open` exit status.
<!-- id: REQ-DSCR-012.A5 -->
- **Keyboard:** tiles are buttons with an accessible name of "<name>, <size>". Enter or Space selects, and Enter on the selected tile opens it.
- **No snapshot:** shows "No scans yet" with a "Start scan" action that goes to Smart Clean.

<!-- id: REQ-DSCR-013 -->
## The Large files screen MUST list large files with filters and move the selected ones to the Trash

implementations:
  - gui/src/screens/LargeFiles.tsx:LargeFiles
  - gui/src/screens/largeFiles/largeCopy.ts:kicker
  - gui/src/screens/largeFiles/largeCopy.ts:rowsBytes
  - gui/src/screens/largeFiles/largeCopy.ts:selectionNote
  - gui/src/screens/largeFiles/largeCopy.ts:trashLabel
  - gui/src/screens/largeFiles/largeCopy.ts:openedLabel
  - gui/src/screens/largeFiles/largeCopy.ts:openedTagClass
  - gui/src/screens/largeFiles/largeCopy.ts:failureMessage
  - gui/src/App.tsx:SCREENS

verifies:
  - gui/src/screens/__tests__/LargeFiles.test.tsx:listsTheEnginesRowsWithHeaderAndColumns
  - gui/src/screens/__tests__/LargeFiles.test.tsx:filtersReQueryTheEngine
  - gui/src/screens/__tests__/LargeFiles.test.tsx:rowOrCheckTogglesSelectionAndFooter
  - gui/src/screens/__tests__/LargeFiles.test.tsx:trashSendsExactlyTheSelectionAndRemovesHandledRows
  - gui/src/screens/__tests__/LargeFiles.test.tsx:emptyListAndNoSnapshotStates
  - gui/src/screens/__tests__/LargeFiles.test.tsx:copyIsWordedFromEngineFigures
  - gui/tests/visual/largeFiles.spec.ts:largeFilesMatchesSnapshot
  - gui/tests/visual/largeFiles.spec.ts:largeFilesPassesAxe
  - gui/tests/visual/largeFiles.spec.ts:keyboardSelectsTwoRowsAndTrashesThem
  - gui/tests/visual/largeFiles.spec.ts:noSnapshotOffersStartScan

**Acceptance**
<!-- id: REQ-DSCR-013.A1 -->
- The layout matches the snapshot's Large files screen at 1200×780 within 1%. It has:
  - the header "<count> files · <total>" and the heading "Large & old files";
  - the All / Over 10 GB / Unused 1 year+ segmented radio group;
  - the table (check, Name, Location, Last opened, Size);
  - the footer.
<!-- id: REQ-DSCR-013.A2 -->
- Rows come from REQ-DSCR-004 for the chosen filter. "Last opened" is a relative time; its tag is accent when `unused_1y` is set and neutral otherwise. "Location" is the parent folder with the home directory shown as `~`.
<!-- id: REQ-DSCR-013.A3 -->
- Clicking a row or its round check toggles selection. The footer note reads either:
  - "<n> selected · restorable from History for 30 days", or
  - "Select files to move them to the Trash."
<!-- id: REQ-DSCR-013.A4 -->
- "Move <selected total> to Trash" is disabled with nothing selected. When clicked, it trashes exactly the selected paths through REQ-DSCR-006 with the title "Large files".
  - Handled rows disappear.
  - The meter refreshes.
  - Failed paths stay listed, with an error in `--color-danger`.

  It always uses the Trash, whatever the trash-by-default setting.
<!-- id: REQ-DSCR-013.A5 -->
- An empty list reads "No files over 2 GB." (gap-fill). No snapshot shows "No scans yet" with "Start scan".

<!-- id: REQ-DSCR-014 -->
## The Duplicates screen MUST let the user choose which copy to keep and remove the rest to the Trash

implementations:
  - gui/src-tauri/src/cache.rs:Keyed
  - gui/src/screens/Duplicates.tsx:Duplicates
  - gui/src/screens/duplicates/dupesCopy.ts:kicker
  - gui/src/screens/duplicates/dupesCopy.ts:eachLabel
  - gui/src/screens/duplicates/dupesCopy.ts:extraCopies
  - gui/src/screens/duplicates/dupesCopy.ts:freedBytes
  - gui/src/screens/duplicates/dupesCopy.ts:footNote
  - gui/src/screens/duplicates/dupesCopy.ts:removeLabel
  - gui/src/screens/duplicates/dupesCopy.ts:failureMessage
  - gui/src/lib/engine.ts:trashDupes
  - gui/src-tauri/src/main.rs:trash_dupes
  - src/contract.rs:trash_dupes
  - gui/src/App.tsx:SCREENS

verifies:
  - gui/src-tauri/src/cache.rs:same_key_within_ttl_is_reused
  - gui/src-tauri/src/cache.rs:new_key_expiry_or_clear_rebuilds
  - gui/src/screens/__tests__/Duplicates.test.tsx:listsGroupsAsCardsWithKickerAndEachTag
  - gui/src/screens/__tests__/Duplicates.test.tsx:firstCopyIsKeptAndClickingChoosesAnother
  - gui/src/screens/__tests__/Duplicates.test.tsx:arrowKeysMoveTheKeptCopyWithinAGroup
  - gui/src/screens/__tests__/Duplicates.test.tsx:removeTrashesEveryNonKeptCopyThenHandledVanish
  - gui/src/screens/__tests__/Duplicates.test.tsx:aRefusalIsShownAndNothingVanishes
  - gui/src/screens/__tests__/Duplicates.test.tsx:emptyNoSnapshotAndBusyStates
  - gui/src/screens/__tests__/Duplicates.test.tsx:copyIsWordedFromEngineFigures
  - gui/tests/visual/duplicates.spec.ts:duplicatesMatchesSnapshot
  - gui/tests/visual/duplicates.spec.ts:duplicatesPassesAxe
  - gui/tests/visual/duplicates.spec.ts:keyboardChoosesACopyThenRemovesExtras
  - gui/tests/visual/duplicates.spec.ts:noSnapshotOffersStartScan
  - src/contract.rs:trash_dupes_keeps_one_copy_of_every_group

**Acceptance**
<!-- id: REQ-DSCR-014.A1 -->
- The layout matches the snapshot's Duplicates screen at 1200×780 within 1%. It has:
  - the header "<n> groups · one copy of each is kept, you choose which" and the heading "Duplicates";
  - a two-column grid of group cards;
  - the footer.
<!-- id: REQ-DSCR-014.A2 -->
- Each card shows the group name, a "<size> each" neutral tag, and one row per copy with a dot, the path and a "Keep" tag (accent-2) or "Remove" tag (outline). Exactly one copy per group is kept, and the first copy by default.
<!-- id: REQ-DSCR-014.A3 -->
- Choosing a copy makes it the kept one. The copy rows of one group form a radio group: arrow keys move between them and the kept copy is `aria-checked`.
<!-- id: REQ-DSCR-014.A4 -->
- **Footer:**
  - The note reads "Frees <reclaimable> · removed copies go to the Trash".
  - "Remove <n> extra copies" trashes every non-kept copy through REQ-DSCR-006, with the title "Duplicates".
  - Handled copies vanish, and groups left with one copy vanish.
  - The meter refreshes.
<!-- id: REQ-DSCR-014.A5 -->
- No groups reads "No duplicates found." (gap-fill). No snapshot shows "No scans yet" with "Start scan". While duplicates are being computed, the screen shows the busy indicator per REQ-DCLEAN-017.
<!-- id: REQ-DSCR-014.A6 -->
- A return to the screen reuses the last result while the snapshot and preferences are unchanged, for up to 15 minutes, instead of hashing again. Any clean, trash or restore discards it, so the next visit finds duplicates afresh.

<!-- id: REQ-DSCR-015 -->
## The History screen MUST list every recorded clean and restore eligible ones

implementations:
  - src/contract.rs:HistoryRow
  - src/contract.rs:HistoryState
  - src/contract.rs:HistoryView
  - src/contract.rs:history_detail
  - src/contract.rs:history_state
  - src/contract.rs:history_view_of
  - src/contract.rs:history_view
  - src/contract.rs:history_view_now
  - src/history.rs:RESTORE_WINDOW_DAYS
  - src/render.rs:history
  - gui/src-tauri/src/main.rs:list_history
  - gui/src/lib/contract.ts:parseHistoryView
  - gui/src/lib/engine.ts:listHistory
  - gui/src/lib/restoreNotices.ts:restoreNotices
  - gui/src/screens/History.tsx:History
  - gui/src/screens/history/historyCopy.ts:headerLine
  - gui/src/screens/history/historyCopy.ts:rowMeta

verifies:
  - src/contract.rs:history_rows_newest_first_with_total
  - src/contract.rs:detail_uses_category_names
  - src/contract.rs:detail_lists_file_names_up_to_three
  - src/contract.rs:state_deleted_for_delete_mode
  - src/contract.rs:state_emptied_at_thirty_days
  - src/contract.rs:state_restored_when_restored_at
  - tests/agentic.rs:history_records_carry_detail_state_and_total
  - gui/src/lib/contract.test.ts:parsesHistoryRowsWithDerivedFields
  - gui/src/lib/contract.test.ts:rejectsAnUnknownHistoryState
  - gui/src/screens/__tests__/History.test.tsx:rendersOneRowPerEntryNewestFirst
  - gui/src/screens/__tests__/History.test.tsx:rowShowsTitleWhenDetailAndSize
  - gui/src/screens/__tests__/History.test.tsx:restorableRowRestoresAndShowsRestoredTag
  - gui/src/screens/__tests__/History.test.tsx:missingItemsAreReported
  - gui/src/screens/__tests__/History.test.tsx:emptiedAndDeletedShowTagsNoRestore
  - gui/src/screens/__tests__/History.test.tsx:emptyLogReadsNothingCleanedYet
  - gui/src/screens/__tests__/History.test.tsx:aFailedRestoreIsReported
  - gui/tests/visual/history.spec.ts:historyMatchesSnapshot
  - gui/tests/visual/history.spec.ts:historyPassesAxe
  - gui/tests/visual/history.spec.ts:keyboardRestoresARow

**Acceptance**
<!-- id: REQ-DSCR-015.A1 -->
- The layout matches the snapshot's History screen at 1200×780 within 1%. It has the header "<total> cleared · items stay restorable for 30 days" and the heading "History", plus one row per history entry, newest first.
<!-- id: REQ-DSCR-015.A2 -->
- Each row shows the trash icon, the entry title, "<when> · <detail>" and the freed size. "When" is a relative date. "Detail" is the category names, or for "Large files" and "Duplicates" entries the file names, or "<n> files" past three.
<!-- id: REQ-DSCR-015.A3 -->
- A trash entry younger than 30 days and not restored shows "Restore". Restore runs `disky restore` through the backend and refreshes the meter. Afterwards the row shows the "Restored" tag. Items no longer in the Trash are reported per REQ-DCLEAN-034.A4.
<!-- id: REQ-DSCR-015.A4 -->
- An entry 30 days or older shows the "Emptied" neutral tag. A delete-mode entry shows a "Deleted" neutral tag and no Restore (gap-fill).
<!-- id: REQ-DSCR-015.A5 -->
- No entries reads "Nothing cleaned yet." (gap-fill).

<!-- id: REQ-DSCR-016 -->
## The Settings screen MUST edit every preference and the excluded folders

implementations:
  - gui/src/screens/Settings.tsx:Settings
  - gui/src/screens/settings/settingsCopy.ts:SETTINGS_TOGGLES
  - gui/src/screens/settings/settingsCopy.ts:removeFolderLabel
  - gui/src/lib/engine.ts:pickFolder
  - gui/src-tauri/src/main.rs:pick_folder
  - src/prefs.rs:abbreviate_home
  - gui/src/App.tsx:ROUTES

verifies:
  - gui/src/screens/__tests__/Settings.test.tsx:rendersEveryPreferenceFromTheStore
  - gui/src/screens/__tests__/Settings.test.tsx:eachSwitchSavesImmediately
  - gui/src/screens/__tests__/Settings.test.tsx:scheduleSavesThePickedPeriod
  - gui/src/screens/__tests__/Settings.test.tsx:addFolderUsesThePickerAndSaves
  - gui/src/screens/__tests__/Settings.test.tsx:cancellingThePickerSavesNothing
  - gui/src/screens/__tests__/Settings.test.tsx:chipRemoveButtonIsNamedAndSaves
  - gui/src/screens/__tests__/Settings.test.tsx:trashOffMakesSmartCleanDeletePermanently
  - gui/tests/visual/settings.spec.ts:settingsMatchesSnapshot
  - gui/tests/visual/settings.spec.ts:settingsPassesAxe
  - gui/tests/visual/settings.spec.ts:keyboardTogglesASwitch
  - gui/tests/visual/settings.spec.ts:foldersAreAddedAndRemovedThroughTheStore
  - src/prefs.rs:picked_folders_abbreviate_home

**Acceptance**
<!-- id: REQ-DSCR-016.A1 -->
- The layout matches the snapshot's Settings screen at 1200×780 within 1%. It has:
  - the heading "Settings";
  - four toggle rows, each with a label, a description and a switch (`role="switch"`, `aria-checked`);
  - the "Scheduled scan" row with an Off / Weekly / Monthly segmented radio group;
  - the "Never touch these folders" card.
<!-- id: REQ-DSCR-016.A2 -->
- Toggle labels and descriptions are the snapshot's text: "Move to Trash instead of deleting", "Skip recently used files", "Preselect only safe categories" and "Warn when below my space goal". Each switch saves its preference immediately.
<!-- id: REQ-DSCR-016.A3 -->
- "Add folder" opens the native folder picker and adds the chosen folder. Each excluded folder is shown as a chip with a remove button whose accessible name is "Remove <folder>" (gap-fill). Every change is saved immediately.
<!-- id: REQ-DSCR-016.A4 -->
- Turning trash-by-default off makes the Smart Clean confirmation default to "Delete permanently" and its footer note warn per the snapshot ("Trash is off in Settings, so items will be deleted permanently."). Large files and Duplicates still use the Trash.

<!-- id: REQ-DSCR-017 -->
## Every new screen MUST be pixel-checked against the snapshot

implementations:
  - gui/tests/visual/journeys.spec.ts:stubWholeApp
  - gui/tests/visual/engineStub.ts:stubEngine

verifies:
  - gui/tests/visual/journeys.spec.ts:everySidebarEntryOpensItsScreen
  - gui/tests/visual/journeys.spec.ts:smartCleanScanCleanUndoShowsInHistory
  - gui/tests/visual/journeys.spec.ts:everyAssistantCardOpensItsReview
  - gui/tests/visual/journeys.spec.ts:assistantPanelSavesEveryControl
  - gui/tests/visual/journeys.spec.ts:spaceMapDrillRevealUp
  - gui/tests/visual/journeys.spec.ts:largeFilesTrashShowsInHistory
  - gui/tests/visual/journeys.spec.ts:duplicatesRemoveShowsInHistory
  - gui/tests/visual/journeys.spec.ts:historyRestoresAnEntry
  - gui/tests/visual/journeys.spec.ts:settingsSaveAndTheAssistantReadsThem

**Acceptance**
<!-- id: REQ-DSCR-017.A1 -->
- Each of the six screens has a Playwright visual test at 1200×780 with `maxDiffPixelRatio` 0.01 against the snapshot rendered with `startScreen` set to that screen. Each test feeds fixtures shaped like the snapshot's sample data and masks only live numbers and the traffic-light slot.
<!-- id: REQ-DSCR-017.A2 -->
- Each screen passes an axe-core run with zero serious or critical violations, and the keyboard-only test covers its primary action.
<!-- id: REQ-DSCR-017.A3 -->
- No placeholder screen remains: every sidebar entry renders its real screen.
<!-- id: REQ-DSCR-017.A4 -->
- Every test above, and every regression test added for a bug, runs on every push and pull request: the CI `gui` job runs the GUI suite, and the `test` job runs the engine's. `scripts/regression.sh` runs the same stages locally.
