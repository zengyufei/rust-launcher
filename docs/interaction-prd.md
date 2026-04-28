# Rust Launcher GUI Interaction PRD

Source: Pencil design `/C:/Users/dyl/AppData/Local/Programs/Pencil/rust lanuch.pen`.

## Core Screens

- Home screen shows plan list, searchable sidebar, current plan sequence, launch mode inspector, selected item inspector, storage hint, and execution log.
- Sequence tree is the primary work area. It shows top-level launch items and groups as ordered timeline rows.
- Each timeline row has a checkbox for selection and an edit action. Groups appear as editable top-level rows that contain child items.
- Bulk selection shows an action bar: delete, group, ungroup, move up, move down, move top, move bottom.
- Plan context menu supports edit plan, delete plan, move top, move up, move down, move bottom, and disable plan.

## Plan Interactions

- Create plan dialog collects plan name, stable plan id, and plan JSON file path.
- Edit plan dialog only changes the display name; id and file path are read-only stable identifiers.
- Delete plan requires confirmation.
- Disabled plans stay in `global.json`, remain visible in the UI, and are skipped by auto/scheduled execution.

## Group Interactions

- Edit group dialog edits group name, description, and group-level failure behavior.
- Group failure behavior controls whether remaining child items continue after any child item fails.
- Child item editing happens through item dialogs, not inside the group dialog.
- Group/ungroup operations transform selected timeline rows while preserving order.

## Item Interactions

- Add item dialog always collects name, item id, description, target kind, delay values, and item failure behavior.
- `path` opens files, folders, text files, music, images, and similar resources through the system default association.
- `program` launches an executable path with optional args and optional working directory.
- `url` launches a browser/protocol URL.
- `command` executes text through an explicit shell and may set a working directory.

## Launch Mode Interactions

- Launch mode is single-select: manual or app-start auto.
- Schedules are shown and active only when app-start auto is selected.
- Schedule rules support daily time, weekly weekday/time, and once datetime.
- Once rules use a human-readable datetime in the UI, such as `2026-05-01 10:00`.

## Data Model Implications

- `global.json` owns plan order, enabled state, stable plan file path, launch trigger, and schedules.
- Each plan JSON owns the editable plan name and the ordered launch sequence.
- Groups are first-class sequence nodes with description and failure behavior.
- Launch items include description and a typed target.
- Schema version is bumped to `2` because launch modes and group/item fields changed incompatibly.
