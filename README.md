# Rust Launcher

<p align="center">
  <img src="docs/assets/readme/gui-workbench.png" alt="Rust Launcher GUI" width="100%" />
</p>

<p align="center">
  <strong>JSON-driven Rust launcher for plans, groups, and launch items.</strong><br />
  <strong>适合把文件、程序、网址、命令整理成一键启动方案的原生 Rust 启动器。</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2021-f46623?style=flat-square" alt="Rust 2021" />
  <img src="https://img.shields.io/badge/Platform-Windows%20first-2563eb?style=flat-square" alt="Windows first" />
  <img src="https://img.shields.io/badge/Storage-JSON-16a34a?style=flat-square" alt="JSON storage" />
  <img src="https://img.shields.io/badge/GUI-Slint-0f766e?style=flat-square" alt="Slint GUI" />
  <img src="https://img.shields.io/badge/CLI-Automation-1d4ed8?style=flat-square" alt="CLI automation" />
</p>

<p align="center">
  <a href="#中文版">中文版</a> ·
  <a href="#english">English</a>
</p>

---

## 中文版

### 项目简介

Rust Launcher 是一个 Windows 优先的原生启动器。它把你的工作环境拆成清晰的三层结构：

```text
方案 Plan
  组 Group
    启动项 Item
  启动项 Item
```

你可以把文件夹、程序、网址、命令行任务整理进一个方案里，再按顺序执行。所有配置都以 JSON 存储在磁盘上，CLI 适合自动化和批量维护，GUI 适合日常浏览、编辑和启动。

### 核心亮点

- JSON 持久化，所有配置都可以直接看见、导出、导入和版本管理。
- 原生 Rust CLI，适合验证、dry-run、脚本化和批量编辑。
- Slint GUI，适合可视化查看方案结构、属性、启动方式和执行日志。
- 支持顺序执行、分组执行、前后延迟和失败策略。
- 支持手动启动、应用启动自动触发，以及 daily/weekly/once 定时规则。
- Windows 下对路径、URL、程序和命令做了针对性的启动适配。

### 功能总览

| 能力 | 说明 |
| --- | --- |
| 方案管理 | 新建、重命名、删除、启用、禁用、导入、导出、排序 |
| 组管理 | 创建组、编辑组、删除组、控制前后延迟和失败策略 |
| 启动项管理 | 支持 `path`、`program`、`url`、`command` 四类目标 |
| 启动控制 | 整个方案运行、单项运行、dry-run 预演 |
| 触发方式 | `manual` / `auto` |
| 定时调度 | `daily` / `weekly` / `once` |
| 数据组织 | `global.json` 记录目录与触发规则；每个方案独立 JSON |

### 截图预览

#### GUI 工作台

![Rust Launcher GUI](docs/assets/readme/gui-workbench.png)

#### CLI 命令总览

![Rust Launcher CLI Overview](docs/assets/readme/cli-overview.png)

#### CLI 运行与编辑示例

![Rust Launcher CLI Run And Edit](docs/assets/readme/cli-run-and-edit.png)

### 数据目录

默认情况下，程序会在可执行文件旁边读取或创建 `data/` 目录：

```text
launcher.exe
launcher-gui.exe
data/
  global.json
  plans/
    demo.json
```

开发阶段如果直接从 workspace 运行，常见位置是项目根目录下的 `data/`。如果直接双击 `target\debug\launcher-gui.exe`，默认数据目录通常会变成 `target\debug\data\`。

CLI 可以用全局参数覆盖数据目录：

```powershell
.\target\debug\launcher.exe --data-dir .\data validate
```

### 快速开始

```powershell
cargo build --workspace
cargo build --workspace --release

# 发布版产物：
# .\target\release\launcher.exe
# .\target\release\launcher-gui.exe

.\target\debug\launcher.exe --data-dir .\data validate
.\target\debug\launcher.exe --data-dir .\data list
.\target\debug\launcher.exe --data-dir .\data run demo --dry-run

# 直接运行导出的单个方案 JSON，无需 data 目录或 global.json：
.\target\debug\launcher.exe run .\huizhou.json --dry-run
.\target\debug\launcher.exe validate .\huizhou.json

.\target\debug\launcher-gui.exe
```

### GUI 用法

GUI 面向“日常使用”和“可视化维护”。推荐使用流程：

1. 启动 `launcher-gui.exe`。
2. 在左侧方案栏选择一个方案，也可以通过搜索框过滤方案与启动项。
3. 在中间结构区查看组与启动项的顺序关系。
4. 点击任意方案、组或启动项，右侧检查器会同步展示属性与启动方式。
5. 通过右上角的“运行方案”按钮真实执行当前方案。
6. 在底部执行日志里查看每个启动项的成功、失败和汇总结果。
7. 通过新建方案、导入方案、新增启动项、编辑节点、定时面板等入口维护 JSON 配置。

GUI 当前覆盖的重点能力：

- 方案浏览与搜索
- 方案新建、重命名、删除、导入、导出、启用禁用、排序
- 启动项与组的新增、编辑、删除、移动
- 启动方式切换
- 定时规则编辑
- 执行日志展示

### CLI 完整用法

所有命令都支持把全局参数写在子命令前：

```powershell
.\target\debug\launcher.exe --data-dir .\data <COMMAND>
```

#### 顶层命令

```text
launcher validate [PLAN_JSON_PATH]
launcher list
launcher run <PLAN_ID|PLAN_JSON_PATH> [--dry-run]
launcher run-item <PLAN_ID> <ITEM_ID> [--dry-run]
launcher daemon
launcher new-plan <ID> <NAME>
launcher plan <SUBCOMMAND>
launcher launch <SUBCOMMAND>
launcher schedule <SUBCOMMAND>
launcher sequence <SUBCOMMAND>
launcher group <SUBCOMMAND>
launcher item <SUBCOMMAND>
```

导出的方案 JSON 也可以直接被 CLI 执行或校验，不需要先导入工作区：

```powershell
launcher run .\huizhou.json
launcher run .\huizhou.json --dry-run
launcher validate .\huizhou.json
```

#### 方案管理

```text
launcher plan list
launcher plan new <ID> <NAME> [--file <FILE>]
launcher plan rename <ID> <NAME>
launcher plan delete <ID> [--delete-file]
launcher plan enable <ID>
launcher plan disable <ID>
launcher plan move <ID> <top|up|down|bottom>
launcher plan export <ID> <OUTPUT_PATH>
launcher plan import <SOURCE_PATH> [--overwrite]
```

#### 启动方式

```text
launcher launch show <PLAN_ID>
launcher launch set <PLAN_ID> <manual|auto>
```

#### 定时调度

```text
launcher schedule list <PLAN_ID>
launcher schedule add-daily <PLAN_ID> <TIME>
launcher schedule add-weekly <PLAN_ID> <monday|tuesday|wednesday|thursday|friday|saturday|sunday> <TIME>
launcher schedule add-once <PLAN_ID> <AT>
launcher schedule remove <PLAN_ID> <INDEX>
```

#### 顶层顺序调整

```text
launcher sequence list <PLAN_ID>
launcher sequence move <PLAN_ID> <NODE_ID> <top|up|down|bottom>
```

#### 组管理

```text
launcher group add <PLAN_ID> <ID> <NAME> [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher group edit <PLAN_ID> <GROUP_ID> [--name <NAME>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher group delete <PLAN_ID> <GROUP_ID> [--keep-items]
```

#### 启动项管理

新增启动项：

```text
launcher item add-path <PLAN_ID> <ID> <NAME> <VALUE> [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item add-program <PLAN_ID> <ID> <NAME> <VALUE> [--arg <ARG>]... [--working-dir <DIR>] [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item add-url <PLAN_ID> <ID> <NAME> <VALUE> [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item add-command <PLAN_ID> <ID> <NAME> <VALUE> [--shell <power-shell|cmd|sh>] [--working-dir <DIR>] [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
```

编辑与移动启动项：

```text
launcher item edit <PLAN_ID> <ITEM_ID> [--name <NAME>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item target-path <PLAN_ID> <ITEM_ID> <VALUE>
launcher item target-program <PLAN_ID> <ITEM_ID> <VALUE> [--arg <ARG>]... [--working-dir <DIR>]
launcher item target-url <PLAN_ID> <ITEM_ID> <VALUE>
launcher item target-command <PLAN_ID> <ITEM_ID> <VALUE> [--shell <power-shell|cmd|sh>] [--working-dir <DIR>]
launcher item move <PLAN_ID> <ITEM_ID> <top|up|down|bottom>
launcher item move-to-group <PLAN_ID> <ITEM_ID> <GROUP_ID>
launcher item move-to-root <PLAN_ID> <ITEM_ID>
launcher item delete <PLAN_ID> <ITEM_ID>
```

### CLI 常用示例

```powershell
.\target\debug\launcher.exe --data-dir .\data plan new work "工作方案"
.\target\debug\launcher.exe --data-dir .\data group add work dev "开发环境" --post-delay-ms 1000
.\target\debug\launcher.exe --data-dir .\data item add-path work project "项目目录" "D:\cache\runMain" --group dev
.\target\debug\launcher.exe --data-dir .\data item add-program work editor "编辑器" "C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe" --arg "D:\cache\runMain"
.\target\debug\launcher.exe --data-dir .\data item add-url work docs "参考文档" "https://www.rust-lang.org"
.\target\debug\launcher.exe --data-dir .\data item add-command work dev-server "开发服务器" "npm run dev" --group dev --shell power-shell --working-dir "D:\cache\runMain"
.\target\debug\launcher.exe --data-dir .\data run work --dry-run
.\target\debug\launcher.exe --data-dir .\data run work
```

### JSON 结构示例

`global.json` 保存方案目录、启用状态和启动规则：

```json
{
  "version": 2,
  "globals": {
    "default_pre_delay_ms": 0,
    "default_post_delay_ms": 0,
    "log_retention_days": 14
  },
  "plans": [
    {
      "id": "demo",
      "file": "plans/demo.json",
      "enabled": true,
      "launch": {
        "trigger": "manual",
        "schedules": []
      }
    }
  ]
}
```

单个方案 JSON 保存执行结构：

```json
{
  "version": 2,
  "id": "demo",
  "name": "演示方案",
  "sequence": [
    {
      "kind": "group",
      "id": "dev",
      "name": "开发环境",
      "description": "打开工作目录和工具",
      "pre_delay_ms": 0,
      "post_delay_ms": 1000,
      "on_failure": "continue",
      "items": [
        {
          "id": "project-folder",
          "name": "项目目录",
          "description": "",
          "target": {
            "kind": "path",
            "value": "D:\\cache\\runMain"
          },
          "pre_delay_ms": 0,
          "post_delay_ms": 500,
          "on_failure": "continue"
        }
      ]
    }
  ]
}
```

### 开发验证

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

### 项目结构

```text
crates/
  launcher-core/   JSON 模型、校验、调度、执行、平台适配
  launcher-cli/    命令行入口
  launcher-gui/    Slint 图形界面
data/              示例数据
docs/assets/       README 截图资源
third_party/       GUI 中文文本渲染相关补丁依赖
```

---

## English

### Overview

Rust Launcher is a Windows-first native launcher written in Rust. It turns folders, apps, URLs, and shell commands into ordered launch plans with a clean three-level structure:

```text
Plan
  Group
    Item
  Item
```

Everything is stored as plain JSON on disk. The CLI is great for automation, dry runs, and scripted edits. The Slint GUI is built for daily browsing, editing, and one-click execution.

### Highlights

- Plain JSON storage that is easy to inspect, export, import, and version.
- Native Rust CLI for validation, dry-run preview, and batch editing.
- Slint desktop GUI for plan browsing, editing, launch controls, and logs.
- Ordered execution with groups, delays, and failure policy control.
- Manual launch, auto-on-app-start launch, and `daily` / `weekly` / `once` schedules.
- Windows-oriented launching behavior for paths, URLs, programs, and shell commands.

### Feature Matrix

| Area | What you get |
| --- | --- |
| Plan management | Create, rename, delete, enable, disable, import, export, reorder |
| Group management | Add, edit, remove groups with delay and failure settings |
| Item management | `path`, `program`, `url`, and `command` launch targets |
| Execution | Run a full plan, run a single item, or preview with `--dry-run` |
| Launch trigger | `manual` or `auto` |
| Scheduling | `daily`, `weekly`, `once` |
| Storage | `global.json` for catalog and triggers, one JSON file per plan |

### Screenshots

#### GUI Workbench

![Rust Launcher GUI](docs/assets/readme/gui-workbench.png)

#### CLI Overview

![Rust Launcher CLI Overview](docs/assets/readme/cli-overview.png)

#### CLI Run And Edit Example

![Rust Launcher CLI Run And Edit](docs/assets/readme/cli-run-and-edit.png)

### Data Directory

By default, the binaries read from or create a `data/` folder next to the executable:

```text
launcher.exe
launcher-gui.exe
data/
  global.json
  plans/
    demo.json
```

During development, running from the workspace usually means using the repository-level `data/` directory. Launching `target\debug\launcher-gui.exe` directly typically uses `target\debug\data\`.

Override the CLI data directory with:

```powershell
.\target\debug\launcher.exe --data-dir .\data validate
```

### Quick Start

```powershell
cargo build --workspace
cargo build --workspace --release

# Release binaries:
# .\target\release\launcher.exe
# .\target\release\launcher-gui.exe

.\target\debug\launcher.exe --data-dir .\data validate
.\target\debug\launcher.exe --data-dir .\data list
.\target\debug\launcher.exe --data-dir .\data run demo --dry-run

# Run an exported plan JSON directly, without data/ or global.json:
.\target\debug\launcher.exe run .\huizhou.json --dry-run
.\target\debug\launcher.exe validate .\huizhou.json

.\target\debug\launcher-gui.exe
```

### GUI Guide

The GUI is the best fit for everyday use and visual maintenance:

1. Start `launcher-gui.exe`.
2. Pick a plan from the left sidebar, or filter plans and items with the search box.
3. Review the ordered plan structure in the center panel.
4. Select a plan, group, or item to inspect its properties on the right.
5. Use the top-right run button to execute the currently selected plan.
6. Read success, failure, and summary output in the execution log area.
7. Use the built-in actions for plan creation, import/export, item editing, group editing, and schedule maintenance.

Current GUI coverage includes:

- Plan browsing and search
- Create, rename, delete, import, export, enable, disable, and reorder plans
- Add, edit, delete, and move groups and items
- Launch trigger switching
- Schedule editing
- Execution log viewing

### Complete CLI Reference

All commands accept the global option before the subcommand:

```powershell
.\target\debug\launcher.exe --data-dir .\data <COMMAND>
```

#### Top-Level Commands

```text
launcher validate [PLAN_JSON_PATH]
launcher list
launcher run <PLAN_ID|PLAN_JSON_PATH> [--dry-run]
launcher run-item <PLAN_ID> <ITEM_ID> [--dry-run]
launcher daemon
launcher new-plan <ID> <NAME>
launcher plan <SUBCOMMAND>
launcher launch <SUBCOMMAND>
launcher schedule <SUBCOMMAND>
launcher sequence <SUBCOMMAND>
launcher group <SUBCOMMAND>
launcher item <SUBCOMMAND>
```

Exported plan JSON files can also be validated or run directly without importing them into a workspace:

```powershell
launcher run .\huizhou.json
launcher run .\huizhou.json --dry-run
launcher validate .\huizhou.json
```

#### Plan Commands

```text
launcher plan list
launcher plan new <ID> <NAME> [--file <FILE>]
launcher plan rename <ID> <NAME>
launcher plan delete <ID> [--delete-file]
launcher plan enable <ID>
launcher plan disable <ID>
launcher plan move <ID> <top|up|down|bottom>
launcher plan export <ID> <OUTPUT_PATH>
launcher plan import <SOURCE_PATH> [--overwrite]
```

#### Launch Trigger Commands

```text
launcher launch show <PLAN_ID>
launcher launch set <PLAN_ID> <manual|auto>
```

#### Schedule Commands

```text
launcher schedule list <PLAN_ID>
launcher schedule add-daily <PLAN_ID> <TIME>
launcher schedule add-weekly <PLAN_ID> <monday|tuesday|wednesday|thursday|friday|saturday|sunday> <TIME>
launcher schedule add-once <PLAN_ID> <AT>
launcher schedule remove <PLAN_ID> <INDEX>
```

#### Sequence Commands

```text
launcher sequence list <PLAN_ID>
launcher sequence move <PLAN_ID> <NODE_ID> <top|up|down|bottom>
```

#### Group Commands

```text
launcher group add <PLAN_ID> <ID> <NAME> [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher group edit <PLAN_ID> <GROUP_ID> [--name <NAME>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher group delete <PLAN_ID> <GROUP_ID> [--keep-items]
```

#### Item Commands

Create items:

```text
launcher item add-path <PLAN_ID> <ID> <NAME> <VALUE> [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item add-program <PLAN_ID> <ID> <NAME> <VALUE> [--arg <ARG>]... [--working-dir <DIR>] [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item add-url <PLAN_ID> <ID> <NAME> <VALUE> [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item add-command <PLAN_ID> <ID> <NAME> <VALUE> [--shell <power-shell|cmd|sh>] [--working-dir <DIR>] [--group <GROUP_ID>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
```

Update, move, and delete items:

```text
launcher item edit <PLAN_ID> <ITEM_ID> [--name <NAME>] [--description <TEXT>] [--pre-delay-ms <MS>] [--post-delay-ms <MS>] [--on-failure <continue|stop>]
launcher item target-path <PLAN_ID> <ITEM_ID> <VALUE>
launcher item target-program <PLAN_ID> <ITEM_ID> <VALUE> [--arg <ARG>]... [--working-dir <DIR>]
launcher item target-url <PLAN_ID> <ITEM_ID> <VALUE>
launcher item target-command <PLAN_ID> <ITEM_ID> <VALUE> [--shell <power-shell|cmd|sh>] [--working-dir <DIR>]
launcher item move <PLAN_ID> <ITEM_ID> <top|up|down|bottom>
launcher item move-to-group <PLAN_ID> <ITEM_ID> <GROUP_ID>
launcher item move-to-root <PLAN_ID> <ITEM_ID>
launcher item delete <PLAN_ID> <ITEM_ID>
```

### Common CLI Examples

```powershell
.\target\debug\launcher.exe --data-dir .\data plan new work "Work Setup"
.\target\debug\launcher.exe --data-dir .\data group add work dev "Development Stack" --post-delay-ms 1000
.\target\debug\launcher.exe --data-dir .\data item add-path work project "Project Folder" "D:\cache\runMain" --group dev
.\target\debug\launcher.exe --data-dir .\data item add-program work editor "Editor" "C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe" --arg "D:\cache\runMain"
.\target\debug\launcher.exe --data-dir .\data item add-url work docs "Reference Docs" "https://www.rust-lang.org"
.\target\debug\launcher.exe --data-dir .\data item add-command work dev-server "Dev Server" "npm run dev" --group dev --shell power-shell --working-dir "D:\cache\runMain"
.\target\debug\launcher.exe --data-dir .\data run work --dry-run
.\target\debug\launcher.exe --data-dir .\data run work
```

### JSON Examples

`global.json` stores plan catalog entries and launch behavior:

```json
{
  "version": 2,
  "globals": {
    "default_pre_delay_ms": 0,
    "default_post_delay_ms": 0,
    "log_retention_days": 14
  },
  "plans": [
    {
      "id": "demo",
      "file": "plans/demo.json",
      "enabled": true,
      "launch": {
        "trigger": "manual",
        "schedules": []
      }
    }
  ]
}
```

Each plan JSON stores the ordered execution graph:

```json
{
  "version": 2,
  "id": "demo",
  "name": "Demo Plan",
  "sequence": [
    {
      "kind": "group",
      "id": "dev",
      "name": "Development Stack",
      "description": "Open working directories and tools",
      "pre_delay_ms": 0,
      "post_delay_ms": 1000,
      "on_failure": "continue",
      "items": [
        {
          "id": "project-folder",
          "name": "Project Folder",
          "description": "",
          "target": {
            "kind": "path",
            "value": "D:\\cache\\runMain"
          },
          "pre_delay_ms": 0,
          "post_delay_ms": 500,
          "on_failure": "continue"
        }
      ]
    }
  ]
}
```

### Development Checks

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

### Repository Layout

```text
crates/
  launcher-core/   JSON models, validation, scheduler, execution, platform adapters
  launcher-cli/    Command-line entry point
  launcher-gui/    Slint desktop UI
data/              Example data
docs/assets/       README screenshot assets
third_party/       Local dependency patch for Chinese GUI text rendering
```
