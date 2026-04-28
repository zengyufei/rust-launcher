# Rust Launcher

Rust Launcher is a Windows-first native launcher prototype. It stores all plans as JSON, runs ordered launch sequences from the CLI, and includes a Slint GUI for read-only browsing and running plans.

## Project Layout

- `crates/launcher-core`: JSON models, validation, scheduling, execution, and platform launch adapters.
- `crates/launcher-cli`: command-line interface.
- `crates/launcher-gui`: Slint GUI for browsing real JSON plans and running the selected plan.
- `data/`: sample development data.
- `docs/interaction-prd.md`: interaction PRD distilled from the Pencil design.

## JSON Schema Direction

The current JSON schema is `version: 2`, matching the Pencil interaction design:

- `global.json` stores plan order, enabled/disabled state, stable JSON file path, one launch trigger, and schedules.
- Plan JSON stores the display name and ordered sequence.
- Groups have `description`, `pre_delay_ms`, `post_delay_ms`, `on_failure`, and child `items`.
- Items have `description`, typed `target`, delays, and `on_failure`.
- `target.kind` supports `path`, `program`, `url`, and `command`.

## CLI

```powershell
cargo run -p launcher-cli -- validate
cargo run -p launcher-cli -- list
cargo run -p launcher-cli -- run work --dry-run
cargo run -p launcher-cli -- run-item work notes --dry-run
cargo run -p launcher-cli -- new-plan music "音乐启动"
cargo run -p launcher-cli -- daemon
```

Use `--data-dir <path>` before the command to point at another JSON directory.

The CLI can now edit the JSON model directly:

```powershell
cargo run -p launcher-cli -- plan new demo "演示方案"
cargo run -p launcher-cli -- group add demo dev "开发环境" --description "开发工具" --on-failure stop
cargo run -p launcher-cli -- item add-path demo project-folder "项目目录" "D:\cache\runMain" --group dev --post-delay-ms 500
cargo run -p launcher-cli -- item add-command demo dev-server "开发服务器" "npm run dev" --group dev --shell powershell --working-dir "D:\cache\runMain"
cargo run -p launcher-cli -- item edit demo dev-server --post-delay-ms 1000 --on-failure stop
cargo run -p launcher-cli -- item target-url demo dev-server "https://example.com"
cargo run -p launcher-cli -- item move-to-root demo dev-server
cargo run -p launcher-cli -- sequence move demo dev-server top
cargo run -p launcher-cli -- launch set demo auto
cargo run -p launcher-cli -- schedule add-daily demo 09:00
cargo run -p launcher-cli -- sequence list demo
```

Supported management surfaces:

- `plan`: list, create, rename, delete catalog entries, enable/disable, and reorder plans.
- `launch`: show or set manual/auto launch trigger.
- `schedule`: list, add daily/weekly/once schedules, and remove schedules by 1-based index.
- `sequence`: list or reorder top-level groups/items.
- `group`: add, edit, delete groups, optionally keeping child items.
- `item`: add `path`, `program`, `url`, or `command` targets; edit item metadata; replace item target; move items within a container, into a group, or back to the plan root; and delete items.

## GUI

```powershell
cargo run -p launcher-gui
```

The GUI v1 follows the Pencil design in a read-only running mode: it loads real JSON, shows plans, launch modes, plan structure, selected-node properties, and execution logs. It only exposes "运行方案"; JSON editing dialogs are reserved for the next GUI phase.
