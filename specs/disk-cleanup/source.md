# Disk Cleanup — Claude Design import (handoff bundle)

**Imported:** 2026-11-24
**Handoff bundle:** /private/tmp/claude-502/-Users-superdeveloper-dev-claude-projects-disky/bc0b213e-b304-414f-90ff-9504fc78968c/scratchpad/handoff-disk-cleanup/disk-cleanup-tool-design
**Chosen file:** `project/Disk Cleanup.dc.html` (62,140 bytes; `cmp` verified identical)

**Snapshot:** snapshot.html (byte-for-byte capture)
**Tokens:** tokens.css (extracted CSS custom properties)

**Assets copied alongside (relative paths preserved):**
- `support.js` — Claude Design runtime (`<x-dc>`, `sc-for`, `sc-if`, `DCLogic`)
- `_ds/organic-2eb7c489-beff-43c2-a70f-35438744f718/styles.css` — "Organic" design system tokens + component classes
- `_ds/organic-2eb7c489-beff-43c2-a70f-35438744f718/_ds_bundle.js`, `_ds_manifest.json`, `readme.md`, `_adherence.oxlintrc.json`

Not copied: `project/.thumbnail` (preview image, not referenced by the HTML).

**Path note:** the import instructions gave the path as
`…/project/project/Disk Cleanup.dc.html`. That path does not exist. The
file actually lives at `…/project/Disk Cleanup.dc.html`, which is the one
copied here.

## Design decision record (verbatim)

**MISSING.** The bundle has no `decision-record.md`. It contains only
`README.md` (generic Claude Design handoff boilerplate: "read the chosen
file in full, follow its imports, recreate pixel-perfectly, ask if
ambiguous") and `project/`. So the stated behaviour has to come from the
prototype logic in the `<script type="text/x-dc">` block of
`snapshot.html`: state machine, data fixtures, copy strings, and props
`startScreen`, `safeMode`, `showNotification`.
