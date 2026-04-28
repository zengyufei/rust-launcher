use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use launcher_core::{
    add_item, add_plan_schedule, combine_root_items, create_plan_with_file, default_data_dir,
    delete_group, delete_item, delete_plan, delete_plan_schedule, execute_plan, export_plan,
    import_plan, load_workspace, move_plan, move_sequence_node, rename_plan, set_plan_enabled,
    set_plan_launch_trigger, ungroup, update_group, update_item, update_plan_schedule,
    validate_workspace, ExecuteOptions, ExecutionReport, FailurePolicy, Group, GroupUpdate,
    ItemUpdate, LaunchItem, LaunchTarget, LaunchTrigger, LauncherError, NodeMoveDirection, Plan,
    PlanCatalogEntry, PlanMoveDirection, ScheduleRule, Scheduler, SequenceNode, Weekday, Workspace,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

slint::include_modules!();

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
struct GuiState {
    workspace: Option<Workspace>,
    selected_plan_id: Option<String>,
    selected_node_ids: Vec<String>,
    logs: Vec<LogRow>,
    running: bool,
    context_plan_id: Option<String>,
    modal: ModalState,
    modal_plan_id: Option<String>,
    modal_node_id: Option<String>,
    modal_error: Option<String>,
    pending_import_path: Option<PathBuf>,
    pending_import_name: Option<String>,
    pending_import_file: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalState {
    None,
    CreatePlan,
    RenamePlan,
    DeletePlan,
    ImportConflict,
    AddItem,
    EditItem,
    EditGroup,
    CombineGroup,
    Schedule,
}

fn main() -> Result<()> {
    let app = AppWindow::new()?;
    let state = Arc::new(Mutex::new(GuiState {
        workspace: None,
        selected_plan_id: None,
        selected_node_ids: Vec::new(),
        logs: Vec::new(),
        running: false,
        context_plan_id: None,
        modal: ModalState::None,
        modal_plan_id: None,
        modal_node_id: None,
        modal_error: None,
        pending_import_path: None,
        pending_import_name: None,
        pending_import_file: None,
    }));

    load_workspace_into_state(&state);
    render(&app, &state);
    start_scheduler_loop(&app, &state);

    let weak = app.as_weak();
    let state_for_plan = Arc::clone(&state);
    app.on_select_plan(move |plan_id| {
        if let Some(app) = weak.upgrade() {
            let mut state = state_for_plan.lock().expect("GUI state lock poisoned");
            state.selected_plan_id = Some(plan_id.to_string());
            state.selected_node_ids.clear();
            state.context_plan_id = None;
            drop(state);
            render(&app, &state_for_plan);
        }
    });

    let weak = app.as_weak();
    let state_for_node = Arc::clone(&state);
    app.on_select_node(move |node_id| {
        if let Some(app) = weak.upgrade() {
            let mut state = state_for_node.lock().expect("GUI state lock poisoned");
            let node_id = node_id.to_string();
            if let Some(index) = state.selected_node_ids.iter().position(|id| id == &node_id) {
                state.selected_node_ids.remove(index);
            } else {
                state.selected_node_ids.push(node_id);
            }
            state.context_plan_id = None;
            drop(state);
            render(&app, &state_for_node);
        }
    });

    let weak = app.as_weak();
    let state_for_run = Arc::clone(&state);
    app.on_run_selected(move || {
        if let Some(app) = weak.upgrade() {
            start_run(&app, &state_for_run);
        }
    });

    let weak = app.as_weak();
    let state_for_unavailable = Arc::clone(&state);
    app.on_unavailable(move |feature| {
        if let Some(app) = weak.upgrade() {
            append_log(
                &state_for_unavailable,
                "info",
                format!("{feature}: 编辑功能下一阶段接入，本轮不写入 JSON。"),
            );
            render(&app, &state_for_unavailable);
        }
    });

    let weak = app.as_weak();
    let state_for_launch_trigger = Arc::clone(&state);
    app.on_set_launch_trigger(move |label| {
        if let Some(app) = weak.upgrade() {
            handle_launch_trigger_action(&app, &state_for_launch_trigger, &label);
        }
    });

    let weak = app.as_weak();
    let state_for_create_modal = Arc::clone(&state);
    app.on_open_create_plan(move || {
        if let Some(app) = weak.upgrade() {
            let (id, file) = next_plan_defaults(&state_for_create_modal);
            {
                let mut state = state_for_create_modal
                    .lock()
                    .expect("GUI state lock poisoned");
                state.context_plan_id = None;
                state.modal = ModalState::CreatePlan;
                state.modal_plan_id = None;
                state.modal_node_id = None;
                state.modal_error = None;
            }
            app.set_create_plan_name("新方案".into());
            app.set_create_plan_id(id.into());
            app.set_create_plan_file(file.into());
            render(&app, &state_for_create_modal);
        }
    });

    let weak = app.as_weak();
    let state_for_import = Arc::clone(&state);
    app.on_open_import_plan(move || {
        if let Some(app) = weak.upgrade() {
            open_import_plan(&app, &state_for_import);
        }
    });

    let weak = app.as_weak();
    let state_for_menu = Arc::clone(&state);
    app.on_open_plan_menu(move |plan_id| {
        if let Some(app) = weak.upgrade() {
            let mut state = state_for_menu.lock().expect("GUI state lock poisoned");
            state.selected_plan_id = Some(plan_id.to_string());
            state.selected_node_ids.clear();
            state.context_plan_id = Some(plan_id.to_string());
            state.modal = ModalState::None;
            state.modal_plan_id = None;
            state.modal_node_id = None;
            state.modal_error = None;
            drop(state);
            render(&app, &state_for_menu);
        }
    });

    let weak = app.as_weak();
    let state_for_close_menu = Arc::clone(&state);
    app.on_close_plan_menu(move || {
        if let Some(app) = weak.upgrade() {
            state_for_close_menu
                .lock()
                .expect("GUI state lock poisoned")
                .context_plan_id = None;
            render(&app, &state_for_close_menu);
        }
    });

    let weak = app.as_weak();
    let state_for_menu_action = Arc::clone(&state);
    app.on_plan_menu_action(move |plan_id, action| {
        if let Some(app) = weak.upgrade() {
            handle_plan_menu_action(&app, &state_for_menu_action, &plan_id, &action);
        }
    });

    let weak = app.as_weak();
    let state_for_close_modal = Arc::clone(&state);
    app.on_close_modal(move || {
        if let Some(app) = weak.upgrade() {
            close_modal(&state_for_close_modal);
            render(&app, &state_for_close_modal);
        }
    });

    let weak = app.as_weak();
    let state_for_create = Arc::clone(&state);
    app.on_create_plan(move |name, id, file| {
        if let Some(app) = weak.upgrade() {
            mutate_plan_from_modal(&app, &state_for_create, |state| {
                let data_dir = default_data_dir();
                create_plan_with_file(&data_dir, &id, &name, &file)?;
                state.selected_plan_id = Some(id.to_string());
                state.logs.push(log_row(
                    "info",
                    format!("已创建方案: {id} ({name}) -> {file}"),
                ));
                Ok(())
            });
        }
    });

    let weak = app.as_weak();
    let state_for_rename = Arc::clone(&state);
    app.on_rename_plan(move |plan_id, name| {
        if let Some(app) = weak.upgrade() {
            mutate_plan_from_modal(&app, &state_for_rename, |state| {
                let data_dir = default_data_dir();
                rename_plan(&data_dir, &plan_id, &name)?;
                state.selected_plan_id = Some(plan_id.to_string());
                state.logs.push(log_row(
                    "info",
                    format!("已重命名方案: {plan_id} -> {name}"),
                ));
                Ok(())
            });
        }
    });

    let weak = app.as_weak();
    let state_for_delete = Arc::clone(&state);
    app.on_delete_plan(move |plan_id| {
        if let Some(app) = weak.upgrade() {
            mutate_plan_from_modal(&app, &state_for_delete, |state| {
                let data_dir = default_data_dir();
                let entry = delete_plan(&data_dir, &plan_id, true)?;
                if state.selected_plan_id.as_deref() == Some(plan_id.as_str()) {
                    state.selected_plan_id = None;
                }
                state.logs.push(log_row(
                    "info",
                    format!("已删除方案: {} -> {}", entry.id, entry.file),
                ));
                Ok(())
            });
        }
    });

    let weak = app.as_weak();
    let state_for_import_overwrite = Arc::clone(&state);
    app.on_confirm_import_overwrite(move || {
        if let Some(app) = weak.upgrade() {
            confirm_import_overwrite(&app, &state_for_import_overwrite);
        }
    });

    let weak = app.as_weak();
    let state_for_add_item = Arc::clone(&state);
    app.on_open_add_item(move || {
        if let Some(app) = weak.upgrade() {
            open_add_item_modal(&app, &state_for_add_item);
        }
    });

    let weak = app.as_weak();
    let state_for_edit_node = Arc::clone(&state);
    app.on_open_edit_node(move |node_id| {
        if let Some(app) = weak.upgrade() {
            open_edit_node_modal(&app, &state_for_edit_node, &node_id);
        }
    });

    let weak = app.as_weak();
    let state_for_item_kind = Arc::clone(&state);
    app.on_item_kind_changed(move |kind| {
        if let Some(app) = weak.upgrade() {
            app.set_item_form_kind(kind);
            state_for_item_kind
                .lock()
                .expect("GUI state lock poisoned")
                .modal_error = None;
            render(&app, &state_for_item_kind);
        }
    });

    let weak = app.as_weak();
    let state_for_save_item = Arc::clone(&state);
    app.on_save_item(
        move |id, name, description, kind, target, args, working_dir, shell, pre, post, failure| {
            if let Some(app) = weak.upgrade() {
                save_item_from_modal(
                    &app,
                    &state_for_save_item,
                    ItemFormInput {
                        id: id.to_string(),
                        name: name.to_string(),
                        description: description.to_string(),
                        kind: kind.to_string(),
                        target: target.to_string(),
                        args: args.to_string(),
                        working_dir: working_dir.to_string(),
                        shell: shell.to_string(),
                        pre_delay_ms: pre.to_string(),
                        post_delay_ms: post.to_string(),
                        on_failure: failure.to_string(),
                    },
                );
            }
        },
    );

    let weak = app.as_weak();
    let state_for_save_group = Arc::clone(&state);
    app.on_save_group(move |id, name, description, pre, post, failure| {
        if let Some(app) = weak.upgrade() {
            save_group_from_modal(
                &app,
                &state_for_save_group,
                GroupFormInput {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: description.to_string(),
                    pre_delay_ms: pre.to_string(),
                    post_delay_ms: post.to_string(),
                    on_failure: failure.to_string(),
                },
            );
        }
    });

    let weak = app.as_weak();
    let state_for_failure = Arc::clone(&state);
    app.on_set_failure_policy(move |failure| {
        if let Some(app) = weak.upgrade() {
            app.set_form_on_failure(failure);
            state_for_failure
                .lock()
                .expect("GUI state lock poisoned")
                .modal_error = None;
            render(&app, &state_for_failure);
        }
    });

    let weak = app.as_weak();
    let state_for_shell = Arc::clone(&state);
    app.on_set_command_shell(move |shell| {
        if let Some(app) = weak.upgrade() {
            app.set_item_form_shell(shell);
            state_for_shell
                .lock()
                .expect("GUI state lock poisoned")
                .modal_error = None;
            render(&app, &state_for_shell);
        }
    });

    let weak = app.as_weak();
    let state_for_schedule_modal = Arc::clone(&state);
    app.on_open_schedule_modal(move || {
        if let Some(app) = weak.upgrade() {
            open_schedule_modal(&app, &state_for_schedule_modal);
        }
    });

    let weak = app.as_weak();
    let state_for_schedule_kind = Arc::clone(&state);
    app.on_schedule_kind_changed(move |kind| {
        if let Some(app) = weak.upgrade() {
            app.set_schedule_form_kind(kind);
            state_for_schedule_kind
                .lock()
                .expect("GUI state lock poisoned")
                .modal_error = None;
            render(&app, &state_for_schedule_kind);
        }
    });

    let weak = app.as_weak();
    let state_for_schedule_edit = Arc::clone(&state);
    app.on_edit_schedule(move |index| {
        if let Some(app) = weak.upgrade() {
            edit_schedule_in_modal(&app, &state_for_schedule_edit, &index);
        }
    });

    let weak = app.as_weak();
    let state_for_schedule_delete = Arc::clone(&state);
    app.on_delete_schedule(move |index| {
        if let Some(app) = weak.upgrade() {
            delete_schedule_from_modal(&app, &state_for_schedule_delete, &index);
        }
    });

    let weak = app.as_weak();
    let state_for_schedule_save = Arc::clone(&state);
    app.on_save_schedule(move |index, kind, time, weekday, at| {
        if let Some(app) = weak.upgrade() {
            save_schedule_from_modal(
                &app,
                &state_for_schedule_save,
                ScheduleFormInput {
                    index: index.to_string(),
                    kind: kind.to_string(),
                    time: time.to_string(),
                    weekday: weekday.to_string(),
                    at: at.to_string(),
                },
            );
        }
    });

    let weak = app.as_weak();
    let state_for_bulk = Arc::clone(&state);
    app.on_bulk_action(move |action| {
        if let Some(app) = weak.upgrade() {
            handle_bulk_action(&app, &state_for_bulk, &action);
        }
    });

    let weak = app.as_weak();
    let state_for_modal = Arc::clone(&state);
    app.window().on_close_requested(move || {
        if let Some(app) = weak.upgrade() {
            let mut state = state_for_modal.lock().expect("GUI state lock poisoned");
            if state.modal != ModalState::None {
                state.modal = ModalState::None;
                state.modal_plan_id = None;
                state.modal_node_id = None;
                state.modal_error = None;
                state.pending_import_path = None;
                state.pending_import_name = None;
                state.pending_import_file = None;
                drop(state);
                render(&app, &state_for_modal);
                return slint::CloseRequestResponse::KeepWindowShown;
            }
        }
        slint::CloseRequestResponse::HideWindow
    });

    app.run()?;
    Ok(())
}

fn handle_plan_menu_action(
    app: &AppWindow,
    state: &Arc<Mutex<GuiState>>,
    plan_id: &SharedString,
    action: &SharedString,
) {
    match action.as_str() {
        "编辑方案" => {
            let name = selected_plan_name(state, plan_id);
            {
                let mut state = state.lock().expect("GUI state lock poisoned");
                state.context_plan_id = None;
                state.modal = ModalState::RenamePlan;
                state.modal_plan_id = Some(plan_id.to_string());
                state.modal_node_id = None;
                state.modal_error = None;
            }
            app.set_rename_plan_name(name.into());
            render(app, state);
        }
        "删除方案" => {
            {
                let mut state = state.lock().expect("GUI state lock poisoned");
                state.context_plan_id = None;
                state.modal = ModalState::DeletePlan;
                state.modal_plan_id = Some(plan_id.to_string());
                state.modal_node_id = None;
                state.modal_error = None;
            }
            render(app, state);
        }
        "导出方案" => export_plan_from_menu(app, state, plan_id),
        "置顶" => mutate_plan_direct(app, state, plan_id, "置顶", |data_dir, id| {
            move_plan(data_dir, id, PlanMoveDirection::Top)
        }),
        "上移" => mutate_plan_direct(app, state, plan_id, "上移", |data_dir, id| {
            move_plan(data_dir, id, PlanMoveDirection::Up)
        }),
        "下移" => mutate_plan_direct(app, state, plan_id, "下移", |data_dir, id| {
            move_plan(data_dir, id, PlanMoveDirection::Down)
        }),
        "置底" => mutate_plan_direct(app, state, plan_id, "置底", |data_dir, id| {
            move_plan(data_dir, id, PlanMoveDirection::Bottom)
        }),
        "禁用方案" => {
            mutate_plan_direct(app, state, plan_id, "禁用方案", |data_dir, id| {
                set_plan_enabled(data_dir, id, false)
            })
        }
        "启用方案" => {
            mutate_plan_direct(app, state, plan_id, "启用方案", |data_dir, id| {
                set_plan_enabled(data_dir, id, true)
            })
        }
        _ => {
            append_log(state, "error", format!("未知方案菜单动作: {action}"));
            render(app, state);
        }
    }
}

fn open_import_plan(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    if state.lock().expect("GUI state lock poisoned").running {
        append_log(state, "error", "导入方案失败: 方案运行中，暂不能编辑。");
        render(app, state);
        return;
    }

    let Some(path) = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .pick_file()
    else {
        return;
    };

    import_plan_from_path(app, state, path, false);
}

fn confirm_import_overwrite(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    let path = {
        let guard = state.lock().expect("GUI state lock poisoned");
        if guard.running {
            drop(guard);
            state.lock().expect("GUI state lock poisoned").modal_error =
                Some("导入方案失败: 方案运行中，暂不能编辑。".to_string());
            render(app, state);
            return;
        }
        guard.pending_import_path.clone()
    };

    let Some(path) = path else {
        state.lock().expect("GUI state lock poisoned").modal_error =
            Some("导入方案失败: 缺少待导入文件路径。".to_string());
        render(app, state);
        return;
    };

    import_plan_from_path(app, state, path, true);
}

fn import_plan_from_path(
    app: &AppWindow,
    state: &Arc<Mutex<GuiState>>,
    path: PathBuf,
    overwrite: bool,
) {
    match import_plan(&default_data_dir(), &path, overwrite) {
        Ok(plan) => {
            {
                let mut guard = state.lock().expect("GUI state lock poisoned");
                guard.selected_plan_id = Some(plan.id.clone());
                guard.selected_node_ids.clear();
                guard.context_plan_id = None;
                guard.modal = ModalState::None;
                guard.modal_plan_id = None;
                guard.modal_node_id = None;
                guard.modal_error = None;
                guard.pending_import_path = None;
                guard.pending_import_name = None;
                guard.pending_import_file = None;
                guard.logs.push(log_row(
                    "info",
                    format!(
                        "{}方案: {} ({}) <- {}",
                        if overwrite {
                            "已覆盖导入"
                        } else {
                            "已导入"
                        },
                        plan.id,
                        plan.name,
                        path.display()
                    ),
                ));
            }
            load_workspace_into_state(state);
        }
        Err(LauncherError::PlanImportConflict {
            plan_id,
            plan_name,
            target_file,
            source_path,
        }) if !overwrite => {
            let mut guard = state.lock().expect("GUI state lock poisoned");
            guard.context_plan_id = None;
            guard.modal = ModalState::ImportConflict;
            guard.modal_plan_id = Some(plan_id);
            guard.modal_node_id = None;
            guard.modal_error = None;
            guard.pending_import_path = Some(source_path);
            guard.pending_import_name = Some(plan_name);
            guard.pending_import_file = Some(target_file);
        }
        Err(error) => {
            let mut guard = state.lock().expect("GUI state lock poisoned");
            if guard.modal == ModalState::ImportConflict {
                guard.modal_error = Some(format!("导入方案失败: {error}"));
            } else {
                guard
                    .logs
                    .push(log_row("error", format!("导入方案失败: {error}")));
            }
        }
    }
    render(app, state);
}

fn export_plan_from_menu(app: &AppWindow, state: &Arc<Mutex<GuiState>>, plan_id: &SharedString) {
    {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        guard.context_plan_id = None;
        if guard.running {
            guard
                .logs
                .push(log_row("error", "导出方案失败: 方案运行中，暂不能编辑。"));
            drop(guard);
            render(app, state);
            return;
        }
    }

    let default_file = format!("{plan_id}.json");
    let Some(path) = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name(&default_file)
        .save_file()
    else {
        render(app, state);
        return;
    };

    match export_plan(&default_data_dir(), plan_id, &path) {
        Ok(()) => append_log(
            state,
            "info",
            format!("已导出方案: {plan_id} -> {}", path.display()),
        ),
        Err(error) => append_log(state, "error", format!("导出方案失败: {error}")),
    }
    render(app, state);
}

fn handle_launch_trigger_action(
    app: &AppWindow,
    state: &Arc<Mutex<GuiState>>,
    label: &SharedString,
) {
    let trigger = match label.as_str() {
        "手动点击" => LaunchTrigger::Manual,
        "应用启动自动" => LaunchTrigger::AutoOnAppStart,
        other => {
            append_log(state, "error", format!("未知启动方式: {other}"));
            render(app, state);
            return;
        }
    };

    let plan_id = {
        let guard = state.lock().expect("GUI state lock poisoned");
        if guard.running {
            drop(guard);
            append_log(state, "error", "切换启动方式失败: 方案运行中，暂不能编辑。");
            render(app, state);
            return;
        }
        let Some(workspace) = &guard.workspace else {
            drop(guard);
            append_log(state, "error", "切换启动方式失败: 工作区未加载。");
            render(app, state);
            return;
        };
        let Some(plan_id) = selected_plan_id(&guard, workspace).map(str::to_string) else {
            drop(guard);
            append_log(state, "error", "切换启动方式失败: 未选择方案。");
            render(app, state);
            return;
        };
        plan_id
    };

    let data_dir = default_data_dir();
    match set_plan_launch_trigger(&data_dir, &plan_id, trigger) {
        Ok(()) => {
            {
                let mut state = state.lock().expect("GUI state lock poisoned");
                state.selected_plan_id = Some(plan_id.clone());
                state.logs.push(log_row(
                    "info",
                    format!("已切换方案启动方式: {plan_id} -> {}", trigger_text(trigger)),
                ));
            }
            load_workspace_into_state(state);
        }
        Err(error) => {
            state
                .lock()
                .expect("GUI state lock poisoned")
                .logs
                .push(log_row(
                    "error",
                    format!("切换启动方式失败 ({plan_id}): {error}"),
                ));
        }
    }
    render(app, state);
}

fn open_schedule_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    let plan_id = {
        let guard = state.lock().expect("GUI state lock poisoned");
        if guard.running {
            drop(guard);
            append_log(state, "error", "打开定时任务失败: 方案运行中，暂不能编辑。");
            render(app, state);
            return;
        }
        match selected_plan_id_for_mutation(&guard) {
            Ok(plan_id) => plan_id,
            Err(error) => {
                drop(guard);
                append_log(state, "error", format!("打开定时任务失败: {error}"));
                render(app, state);
                return;
            }
        }
    };

    {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        guard.context_plan_id = None;
        guard.modal = ModalState::Schedule;
        guard.modal_plan_id = Some(plan_id);
        guard.modal_node_id = None;
        guard.modal_error = None;
    }
    set_schedule_form_defaults(app);
    render(app, state);
}

fn edit_schedule_in_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>, index: &str) {
    let result = {
        let guard = state.lock().expect("GUI state lock poisoned");
        schedule_at_modal_index(&guard, index)
    };
    match result {
        Ok((index, schedule)) => {
            set_schedule_form(app, Some(index), &schedule);
            state.lock().expect("GUI state lock poisoned").modal_error = None;
            render(app, state);
        }
        Err(error) => {
            state.lock().expect("GUI state lock poisoned").modal_error = Some(error.to_string());
            render(app, state);
        }
    }
}

fn delete_schedule_from_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>, index: &str) {
    let result = {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        (|| -> launcher_core::Result<()> {
            if guard.running {
                return Err(launcher_core::LauncherError::Validation(
                    "方案运行中，暂不能编辑。".to_string(),
                ));
            }
            let plan_id = current_modal_plan_id(&guard)?;
            let index = parse_schedule_index(index)?;
            delete_plan_schedule(&default_data_dir(), &plan_id, index)?;
            guard.logs.push(log_row(
                "info",
                format!("已删除定时任务: {plan_id} #{}", index + 1),
            ));
            guard.modal_error = None;
            guard.modal_node_id = None;
            Ok(())
        })()
    };

    match result {
        Ok(()) => {
            set_schedule_form_defaults(app);
            load_workspace_into_state(state);
        }
        Err(error) => {
            state.lock().expect("GUI state lock poisoned").modal_error = Some(error.to_string());
        }
    }
    render(app, state);
}

fn save_schedule_from_modal(
    app: &AppWindow,
    state: &Arc<Mutex<GuiState>>,
    input: ScheduleFormInput,
) {
    let result = {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        (|| -> launcher_core::Result<()> {
            if guard.running {
                return Err(launcher_core::LauncherError::Validation(
                    "方案运行中，暂不能编辑。".to_string(),
                ));
            }
            let plan_id = current_modal_plan_id(&guard)?;
            let schedule = build_schedule(&input)?;
            if input.index.trim().is_empty() {
                add_plan_schedule(&default_data_dir(), &plan_id, schedule)?;
                guard
                    .logs
                    .push(log_row("info", format!("已新增定时任务: {plan_id}")));
            } else {
                let index = parse_schedule_index(&input.index)?;
                update_plan_schedule(&default_data_dir(), &plan_id, index, schedule)?;
                guard.logs.push(log_row(
                    "info",
                    format!("已更新定时任务: {plan_id} #{}", index + 1),
                ));
            }
            guard.modal_error = None;
            guard.modal_node_id = None;
            Ok(())
        })()
    };

    match result {
        Ok(()) => {
            set_schedule_form_defaults(app);
            load_workspace_into_state(state);
        }
        Err(error) => {
            state.lock().expect("GUI state lock poisoned").modal_error = Some(error.to_string());
        }
    }
    render(app, state);
}

fn mutate_plan_direct<F>(
    app: &AppWindow,
    state: &Arc<Mutex<GuiState>>,
    plan_id: &SharedString,
    action_name: &str,
    action: F,
) where
    F: FnOnce(&Path, &str) -> launcher_core::Result<()>,
{
    let plan_id = plan_id.to_string();
    {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        guard.context_plan_id = None;
        if guard.running {
            guard.logs.push(log_row(
                "error",
                format!("{action_name} ({plan_id}) 失败: 方案运行中，暂不能编辑。"),
            ));
            drop(guard);
            render(app, state);
            return;
        }
    }

    let data_dir = default_data_dir();
    match action(&data_dir, &plan_id) {
        Ok(()) => {
            {
                let mut state = state.lock().expect("GUI state lock poisoned");
                state.selected_plan_id = Some(plan_id.clone());
                state.logs.push(log_row(
                    "info",
                    format!("已执行方案操作: {action_name} ({plan_id})"),
                ));
            }
            load_workspace_into_state(state);
        }
        Err(error) => {
            state
                .lock()
                .expect("GUI state lock poisoned")
                .logs
                .push(log_row(
                    "error",
                    format!("{action_name} ({plan_id}) 失败: {error}"),
                ));
        }
    }
    render(app, state);
}

#[derive(Debug, Clone)]
struct ItemFormInput {
    id: String,
    name: String,
    description: String,
    kind: String,
    target: String,
    args: String,
    working_dir: String,
    shell: String,
    pre_delay_ms: String,
    post_delay_ms: String,
    on_failure: String,
}

#[derive(Debug, Clone)]
struct GroupFormInput {
    id: String,
    name: String,
    description: String,
    pre_delay_ms: String,
    post_delay_ms: String,
    on_failure: String,
}

#[derive(Debug, Clone)]
struct ScheduleFormInput {
    index: String,
    kind: String,
    time: String,
    weekday: String,
    at: String,
}

fn open_add_item_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    let (plan_id, item_id) = {
        let guard = state.lock().expect("GUI state lock poisoned");
        let Some(workspace) = &guard.workspace else {
            drop(guard);
            append_log(state, "error", "没有可编辑的方案");
            render(app, state);
            return;
        };
        let Some(plan_id) = selected_plan_id(&guard, workspace).map(str::to_string) else {
            drop(guard);
            append_log(state, "error", "没有可编辑的方案");
            render(app, state);
            return;
        };
        let item_id = next_node_id(
            workspace.plans.iter().find(|plan| plan.id == plan_id),
            "new-item",
        );
        (plan_id, item_id)
    };

    {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        guard.context_plan_id = None;
        guard.modal = ModalState::AddItem;
        guard.modal_plan_id = Some(plan_id);
        guard.modal_node_id = None;
        guard.modal_error = None;
    }
    set_item_form(
        app,
        &ItemFormInput {
            id: item_id,
            name: "新启动项".to_string(),
            description: String::new(),
            kind: "path".to_string(),
            target: String::new(),
            args: String::new(),
            working_dir: String::new(),
            shell: "PowerShell".to_string(),
            pre_delay_ms: "0".to_string(),
            post_delay_ms: "0".to_string(),
            on_failure: "continue".to_string(),
        },
    );
    render(app, state);
}

fn open_edit_node_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>, node_id: &str) {
    let mut modal_data = None;
    {
        let guard = state.lock().expect("GUI state lock poisoned");
        if let Some(workspace) = &guard.workspace {
            if let Some(plan_id) = selected_plan_id(&guard, workspace) {
                if let Some(plan) = workspace.plans.iter().find(|plan| plan.id == plan_id) {
                    modal_data =
                        node_for_modal(plan, node_id).map(|node| (plan_id.to_string(), node));
                }
            }
        }
    }

    match modal_data {
        Some((plan_id, NodeModalData::Group(group))) => {
            {
                let mut guard = state.lock().expect("GUI state lock poisoned");
                guard.context_plan_id = None;
                guard.modal = ModalState::EditGroup;
                guard.modal_plan_id = Some(plan_id);
                guard.modal_node_id = Some(group.id.clone());
                guard.modal_error = None;
            }
            set_group_form(
                app,
                &group.id,
                &group.name,
                &group.description,
                group.pre_delay_ms,
                group.post_delay_ms,
                group.on_failure,
            );
            render(app, state);
        }
        Some((plan_id, NodeModalData::Item(item))) => {
            {
                let mut guard = state.lock().expect("GUI state lock poisoned");
                guard.context_plan_id = None;
                guard.modal = ModalState::EditItem;
                guard.modal_plan_id = Some(plan_id);
                guard.modal_node_id = Some(item.id.clone());
                guard.modal_error = None;
            }
            set_item_form_from_item(app, &item);
            render(app, state);
        }
        None => {
            append_log(state, "error", format!("找不到可编辑节点: {node_id}"));
            render(app, state);
        }
    }
}

enum NodeModalData {
    Group(Group),
    Item(LaunchItem),
}

fn node_for_modal(plan: &Plan, node_id: &str) -> Option<NodeModalData> {
    plan.sequence.iter().find_map(|node| match node {
        SequenceNode::Group(group) if group.id == node_id => {
            Some(NodeModalData::Group(group.clone()))
        }
        SequenceNode::Item(item) if item.id == node_id => Some(NodeModalData::Item(item.clone())),
        _ => None,
    })
}

fn save_item_from_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>, input: ItemFormInput) {
    mutate_structure_from_modal(app, state, |guard| {
        let plan_id = current_modal_plan_id(guard)?;
        let target = build_target(&input)?;
        let pre_delay_ms = parse_delay("执行前延迟", &input.pre_delay_ms)?;
        let post_delay_ms = parse_delay("执行后延迟", &input.post_delay_ms)?;
        let on_failure = failure_from_text(&input.on_failure)?;
        let data_dir = default_data_dir();

        match guard.modal {
            ModalState::AddItem => {
                add_item(
                    &data_dir,
                    &plan_id,
                    None,
                    LaunchItem {
                        id: input.id.clone(),
                        name: input.name.clone(),
                        description: input.description.clone(),
                        target,
                        pre_delay_ms,
                        post_delay_ms,
                        on_failure,
                    },
                )?;
                guard.selected_node_ids = vec![input.id.clone()];
                guard
                    .logs
                    .push(log_row("info", format!("已新增启动项: {}", input.id)));
            }
            ModalState::EditItem => {
                let item_id = guard.modal_node_id.clone().ok_or_else(|| {
                    launcher_core::LauncherError::Validation("缺少启动项标识".to_string())
                })?;
                update_item(
                    &data_dir,
                    &plan_id,
                    &item_id,
                    ItemUpdate {
                        name: Some(input.name.clone()),
                        description: Some(input.description.clone()),
                        pre_delay_ms: Some(pre_delay_ms),
                        post_delay_ms: Some(post_delay_ms),
                        on_failure: Some(on_failure),
                        target: Some(target),
                    },
                )?;
                guard.selected_node_ids = vec![item_id.clone()];
                guard
                    .logs
                    .push(log_row("info", format!("已编辑启动项: {item_id}")));
            }
            _ => {
                return Err(launcher_core::LauncherError::Validation(
                    "当前弹窗不是启动项编辑".to_string(),
                ));
            }
        }
        Ok(())
    });
}

fn save_group_from_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>, input: GroupFormInput) {
    mutate_structure_from_modal(app, state, |guard| {
        let plan_id = current_modal_plan_id(guard)?;
        let pre_delay_ms = parse_delay("执行前延迟", &input.pre_delay_ms)?;
        let post_delay_ms = parse_delay("执行后延迟", &input.post_delay_ms)?;
        let on_failure = failure_from_text(&input.on_failure)?;
        let data_dir = default_data_dir();

        match guard.modal {
            ModalState::EditGroup => {
                let group_id = guard.modal_node_id.clone().ok_or_else(|| {
                    launcher_core::LauncherError::Validation("缺少组标识".to_string())
                })?;
                update_group(
                    &data_dir,
                    &plan_id,
                    &group_id,
                    GroupUpdate {
                        name: Some(input.name.clone()),
                        description: Some(input.description.clone()),
                        pre_delay_ms: Some(pre_delay_ms),
                        post_delay_ms: Some(post_delay_ms),
                        on_failure: Some(on_failure),
                    },
                )?;
                guard.selected_node_ids = vec![group_id.clone()];
                guard
                    .logs
                    .push(log_row("info", format!("已编辑组: {group_id}")));
            }
            ModalState::CombineGroup => {
                let selected_ids = selected_root_ids_in_plan_order(guard)?;
                combine_root_items(
                    &data_dir,
                    &plan_id,
                    &selected_ids,
                    Group {
                        id: input.id.clone(),
                        name: input.name.clone(),
                        description: input.description.clone(),
                        pre_delay_ms,
                        post_delay_ms,
                        on_failure,
                        items: Vec::new(),
                    },
                )?;
                guard.selected_node_ids = vec![input.id.clone()];
                guard.logs.push(log_row(
                    "info",
                    format!("已组合 {} 个启动项为组: {}", selected_ids.len(), input.id),
                ));
            }
            _ => {
                return Err(launcher_core::LauncherError::Validation(
                    "当前弹窗不是组编辑".to_string(),
                ));
            }
        }
        Ok(())
    });
}

fn handle_bulk_action(app: &AppWindow, state: &Arc<Mutex<GuiState>>, action: &SharedString) {
    if action.as_str() == "组合" {
        open_combine_group_modal(app, state);
        return;
    }

    mutate_structure_direct(app, state, action.as_str(), |guard| {
        let plan_id = selected_plan_id_for_mutation(guard)?;
        let data_dir = default_data_dir();
        let selected_ids = selected_root_ids_in_plan_order(guard)?;
        match action.as_str() {
            "删除" => {
                for node_id in &selected_ids {
                    match root_node_kind(guard, node_id)? {
                        "group" => delete_group(&data_dir, &plan_id, node_id, false)?,
                        "item" => delete_item(&data_dir, &plan_id, node_id)?,
                        _ => unreachable!(),
                    }
                }
                guard.logs.push(log_row(
                    "info",
                    format!("已删除 {} 个节点", selected_ids.len()),
                ));
                guard.selected_node_ids.clear();
            }
            "取消组合" => {
                ensure_all_groups(guard, &selected_ids)?;
                ungroup(&data_dir, &plan_id, &selected_ids)?;
                guard.logs.push(log_row(
                    "info",
                    format!("已取消组合 {} 个组", selected_ids.len()),
                ));
                guard.selected_node_ids.clear();
            }
            "置顶" | "上移" | "下移" | "置底" => {
                let direction = direction_from_action(action.as_str())?;
                let ordered = ordered_ids_for_direction(&selected_ids, direction);
                for node_id in &ordered {
                    move_sequence_node(&data_dir, &plan_id, node_id, direction)?;
                }
                guard.logs.push(log_row(
                    "info",
                    format!("已{} {} 个节点", action, selected_ids.len()),
                ));
                guard.selected_node_ids = selected_ids;
            }
            _ => {
                return Err(launcher_core::LauncherError::Validation(format!(
                    "未知批量动作: {action}"
                )));
            }
        }
        Ok(())
    });
}

fn open_combine_group_modal(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    let result = {
        let guard = state.lock().expect("GUI state lock poisoned");
        let plan_id = selected_plan_id_for_mutation(&guard);
        let selected = selected_root_ids_in_plan_order(&guard);
        plan_id.and_then(|plan_id| {
            let selected = selected?;
            ensure_all_items(&guard, &selected)?;
            if selected.len() < 2 {
                return Err(launcher_core::LauncherError::Validation(
                    "组合至少需要选择两个根层级启动项".to_string(),
                ));
            }
            let id = guard
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.plans.iter().find(|plan| plan.id == plan_id))
                .map(|plan| next_node_id(Some(plan), "new-group"))
                .unwrap_or_else(|| "new-group".to_string());
            Ok((plan_id, id))
        })
    };

    match result {
        Ok((plan_id, group_id)) => {
            {
                let mut guard = state.lock().expect("GUI state lock poisoned");
                guard.context_plan_id = None;
                guard.modal = ModalState::CombineGroup;
                guard.modal_plan_id = Some(plan_id);
                guard.modal_node_id = None;
                guard.modal_error = None;
            }
            set_group_form(app, &group_id, "新组合", "", 0, 0, FailurePolicy::Continue);
            render(app, state);
        }
        Err(error) => {
            append_log(state, "error", format!("组合失败: {error}"));
            render(app, state);
        }
    }
}

fn mutate_structure_from_modal<F>(app: &AppWindow, state: &Arc<Mutex<GuiState>>, action: F)
where
    F: FnOnce(&mut GuiState) -> launcher_core::Result<()>,
{
    let success = {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        if guard.running {
            guard.modal_error = Some("方案运行中，暂不能编辑。".to_string());
            false
        } else {
            match action(&mut guard) {
                Ok(()) => {
                    guard.modal = ModalState::None;
                    guard.modal_plan_id = None;
                    guard.modal_node_id = None;
                    guard.modal_error = None;
                    true
                }
                Err(error) => {
                    guard.modal_error = Some(error.to_string());
                    false
                }
            }
        }
    };

    if success {
        load_workspace_into_state(state);
    }
    render(app, state);
}

fn mutate_structure_direct<F>(
    app: &AppWindow,
    state: &Arc<Mutex<GuiState>>,
    action_name: &str,
    action: F,
) where
    F: FnOnce(&mut GuiState) -> launcher_core::Result<()>,
{
    let success = {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        guard.context_plan_id = None;
        if guard.running {
            guard.logs.push(log_row(
                "error",
                format!("{action_name} 失败: 方案运行中，暂不能编辑。"),
            ));
            false
        } else {
            match action(&mut guard) {
                Ok(()) => true,
                Err(error) => {
                    guard
                        .logs
                        .push(log_row("error", format!("{action_name} 失败: {error}")));
                    false
                }
            }
        }
    };

    if success {
        load_workspace_into_state(state);
    }
    render(app, state);
}

fn mutate_plan_from_modal<F>(app: &AppWindow, state: &Arc<Mutex<GuiState>>, action: F)
where
    F: FnOnce(&mut GuiState) -> launcher_core::Result<()>,
{
    let success = {
        let mut guard = state.lock().expect("GUI state lock poisoned");
        if guard.running {
            guard.modal_error = Some("方案运行中，暂不能编辑。".to_string());
            false
        } else {
            match action(&mut guard) {
                Ok(()) => {
                    guard.modal = ModalState::None;
                    guard.modal_plan_id = None;
                    guard.modal_node_id = None;
                    guard.modal_error = None;
                    true
                }
                Err(error) => {
                    guard.modal_error = Some(error.to_string());
                    false
                }
            }
        }
    };

    if success {
        load_workspace_into_state(state);
    }
    render(app, state);
}

fn close_modal(state: &Arc<Mutex<GuiState>>) {
    let mut state = state.lock().expect("GUI state lock poisoned");
    state.modal = ModalState::None;
    state.modal_plan_id = None;
    state.modal_node_id = None;
    state.modal_error = None;
    state.pending_import_path = None;
    state.pending_import_name = None;
    state.pending_import_file = None;
}

fn current_modal_plan_id(state: &GuiState) -> launcher_core::Result<String> {
    state
        .modal_plan_id
        .clone()
        .ok_or_else(|| launcher_core::LauncherError::Validation("缺少方案标识".to_string()))
}

fn schedule_at_modal_index(
    state: &GuiState,
    index: &str,
) -> launcher_core::Result<(usize, ScheduleRule)> {
    let plan_id = current_modal_plan_id(state)?;
    let index = parse_schedule_index(index)?;
    let workspace = state.workspace.as_ref().ok_or_else(|| {
        launcher_core::LauncherError::Validation("没有已加载的工作区".to_string())
    })?;
    let entry = workspace
        .global
        .plans
        .iter()
        .find(|entry| entry.id == plan_id)
        .ok_or_else(|| launcher_core::LauncherError::PlanNotFound(plan_id.clone()))?;
    entry
        .launch
        .schedules
        .get(index)
        .cloned()
        .map(|schedule| (index, schedule))
        .ok_or_else(|| {
            launcher_core::LauncherError::Validation(format!("定时任务索引 {} 超出范围", index + 1))
        })
}

fn parse_schedule_index(index: &str) -> launcher_core::Result<usize> {
    index
        .trim()
        .parse::<usize>()
        .map_err(|_| launcher_core::LauncherError::Validation(format!("无效定时任务索引: {index}")))
}

fn build_schedule(input: &ScheduleFormInput) -> launcher_core::Result<ScheduleRule> {
    match input.kind.as_str() {
        "daily" => Ok(ScheduleRule::Daily {
            time: input.time.trim().to_string(),
        }),
        "weekly" => Ok(ScheduleRule::Weekly {
            weekday: weekday_from_text(&input.weekday)?,
            time: input.time.trim().to_string(),
        }),
        "once" => Ok(ScheduleRule::Once {
            at: input.at.trim().to_string(),
        }),
        _ => Err(launcher_core::LauncherError::Validation(format!(
            "未知定时类型: {}",
            input.kind
        ))),
    }
}

fn weekday_from_text(value: &str) -> launcher_core::Result<Weekday> {
    match value {
        "monday" | "星期一" => Ok(Weekday::Monday),
        "tuesday" | "星期二" => Ok(Weekday::Tuesday),
        "wednesday" | "星期三" => Ok(Weekday::Wednesday),
        "thursday" | "星期四" => Ok(Weekday::Thursday),
        "friday" | "星期五" => Ok(Weekday::Friday),
        "saturday" | "星期六" => Ok(Weekday::Saturday),
        "sunday" | "星期日" => Ok(Weekday::Sunday),
        _ => Err(launcher_core::LauncherError::Validation(format!(
            "未知星期: {value}"
        ))),
    }
}

fn selected_plan_id_for_mutation(state: &GuiState) -> launcher_core::Result<String> {
    let workspace = state.workspace.as_ref().ok_or_else(|| {
        launcher_core::LauncherError::Validation("没有已加载的工作区".to_string())
    })?;
    selected_plan_id(state, workspace)
        .map(str::to_string)
        .ok_or_else(|| launcher_core::LauncherError::Validation("没有选中的方案".to_string()))
}

fn selected_root_ids_in_plan_order(state: &GuiState) -> launcher_core::Result<Vec<String>> {
    if state.selected_node_ids.is_empty() {
        return Err(launcher_core::LauncherError::Validation(
            "没有选中的启动项".to_string(),
        ));
    }
    let workspace = state.workspace.as_ref().ok_or_else(|| {
        launcher_core::LauncherError::Validation("没有已加载的工作区".to_string())
    })?;
    let plan_id = selected_plan_id(state, workspace)
        .ok_or_else(|| launcher_core::LauncherError::Validation("没有选中的方案".to_string()))?;
    let plan = workspace
        .plans
        .iter()
        .find(|plan| plan.id == plan_id)
        .ok_or_else(|| launcher_core::LauncherError::Validation("方案不存在".to_string()))?;
    let selected = state
        .selected_node_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let ids = plan
        .sequence
        .iter()
        .filter(|node| selected.contains(&node.id().to_string()))
        .map(|node| node.id().to_string())
        .collect::<Vec<_>>();
    if ids.len() != state.selected_node_ids.len() {
        return Err(launcher_core::LauncherError::Validation(
            "当前批量操作只支持根层级节点".to_string(),
        ));
    }
    Ok(ids)
}

fn root_node_kind<'a>(state: &'a GuiState, node_id: &str) -> launcher_core::Result<&'a str> {
    let workspace = state.workspace.as_ref().ok_or_else(|| {
        launcher_core::LauncherError::Validation("没有已加载的工作区".to_string())
    })?;
    let plan_id = selected_plan_id(state, workspace)
        .ok_or_else(|| launcher_core::LauncherError::Validation("没有选中的方案".to_string()))?;
    let plan = workspace
        .plans
        .iter()
        .find(|plan| plan.id == plan_id)
        .ok_or_else(|| launcher_core::LauncherError::Validation("方案不存在".to_string()))?;
    plan.sequence
        .iter()
        .find_map(|node| match node {
            SequenceNode::Group(group) if group.id == node_id => Some("group"),
            SequenceNode::Item(item) if item.id == node_id => Some("item"),
            _ => None,
        })
        .ok_or_else(|| launcher_core::LauncherError::Validation(format!("节点不存在: {node_id}")))
}

fn ensure_all_items(state: &GuiState, ids: &[String]) -> launcher_core::Result<()> {
    for id in ids {
        if root_node_kind(state, id)? != "item" {
            return Err(launcher_core::LauncherError::Validation(
                "组合只支持根层级启动项，不支持组".to_string(),
            ));
        }
    }
    Ok(())
}

fn ensure_all_groups(state: &GuiState, ids: &[String]) -> launcher_core::Result<()> {
    for id in ids {
        if root_node_kind(state, id)? != "group" {
            return Err(launcher_core::LauncherError::Validation(
                "取消组合只支持组".to_string(),
            ));
        }
    }
    Ok(())
}

fn direction_from_action(action: &str) -> launcher_core::Result<NodeMoveDirection> {
    match action {
        "置顶" => Ok(NodeMoveDirection::Top),
        "上移" => Ok(NodeMoveDirection::Up),
        "下移" => Ok(NodeMoveDirection::Down),
        "置底" => Ok(NodeMoveDirection::Bottom),
        _ => Err(launcher_core::LauncherError::Validation(format!(
            "未知排序动作: {action}"
        ))),
    }
}

fn ordered_ids_for_direction(ids: &[String], direction: NodeMoveDirection) -> Vec<String> {
    let mut ids = ids.to_vec();
    if matches!(direction, NodeMoveDirection::Top | NodeMoveDirection::Down) {
        ids.reverse();
    }
    ids
}

fn next_node_id(plan: Option<&Plan>, base: &str) -> String {
    let Some(plan) = plan else {
        return base.to_string();
    };
    let mut index = 1;
    loop {
        let id = if index == 1 {
            base.to_string()
        } else {
            format!("{base}-{index}")
        };
        let exists = plan.sequence.iter().any(|node| node.id() == id)
            || plan.sequence.iter().any(|node| match node {
                SequenceNode::Group(group) => group.items.iter().any(|item| item.id == id),
                SequenceNode::Item(_) => false,
            });
        if !exists {
            return id;
        }
        index += 1;
    }
}

fn parse_delay(label: &str, value: &str) -> launcher_core::Result<u64> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(0);
    }
    value.parse::<u64>().map_err(|_| {
        launcher_core::LauncherError::Validation(format!("{label} 必须是非负整数毫秒"))
    })
}

fn failure_from_text(value: &str) -> launcher_core::Result<FailurePolicy> {
    match value {
        "continue" => Ok(FailurePolicy::Continue),
        "stop" => Ok(FailurePolicy::Stop),
        _ => Err(launcher_core::LauncherError::Validation(format!(
            "未知失败策略: {value}"
        ))),
    }
}

fn shell_from_text(value: &str) -> launcher_core::Result<launcher_core::CommandShell> {
    match value {
        "power_shell" | "powershell" | "PowerShell" => Ok(launcher_core::CommandShell::PowerShell),
        "cmd" | "命令提示符" => Ok(launcher_core::CommandShell::Cmd),
        "sh" | "Sh" => Ok(launcher_core::CommandShell::Sh),
        _ => Err(launcher_core::LauncherError::Validation(format!(
            "未知 shell: {value}"
        ))),
    }
}

fn build_target(input: &ItemFormInput) -> launcher_core::Result<LaunchTarget> {
    let value = input.target.trim().to_string();
    if value.is_empty() {
        return Err(launcher_core::LauncherError::Validation(
            "目标不能为空".to_string(),
        ));
    }
    let working_dir = optional_string(&input.working_dir);
    match input.kind.as_str() {
        "path" => Ok(LaunchTarget::Path { value }),
        "program" => Ok(LaunchTarget::Program {
            value,
            args: parse_args(&input.args),
            working_dir,
        }),
        "url" => Ok(LaunchTarget::Url { value }),
        "command" => Ok(LaunchTarget::Command {
            value,
            shell: shell_from_text(&input.shell)?,
            working_dir,
        }),
        _ => Err(launcher_core::LauncherError::Validation(format!(
            "未知目标类型: {}",
            input.kind
        ))),
    }
}

fn optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_args(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn set_item_form_from_item(app: &AppWindow, item: &LaunchItem) {
    let (kind, target, args, working_dir, shell) = match &item.target {
        LaunchTarget::Path { value } => (
            "path",
            value.clone(),
            String::new(),
            String::new(),
            "PowerShell",
        ),
        LaunchTarget::Program {
            value,
            args,
            working_dir,
        } => (
            "program",
            value.clone(),
            args.join(" "),
            working_dir.clone().unwrap_or_default(),
            "PowerShell",
        ),
        LaunchTarget::Url { value } => (
            "url",
            value.clone(),
            String::new(),
            String::new(),
            "PowerShell",
        ),
        LaunchTarget::Command {
            value,
            shell,
            working_dir,
        } => (
            "command",
            value.clone(),
            String::new(),
            working_dir.clone().unwrap_or_default(),
            shell_text(*shell),
        ),
    };
    set_item_form(
        app,
        &ItemFormInput {
            id: item.id.clone(),
            name: item.name.clone(),
            description: item.description.clone(),
            kind: kind.to_string(),
            target,
            args,
            working_dir,
            shell: shell.to_string(),
            pre_delay_ms: item.pre_delay_ms.to_string(),
            post_delay_ms: item.post_delay_ms.to_string(),
            on_failure: failure_text(item.on_failure),
        },
    );
}

fn set_item_form(app: &AppWindow, input: &ItemFormInput) {
    app.set_item_form_kind(input.kind.as_str().into());
    app.set_item_form_id(input.id.as_str().into());
    app.set_item_form_name(input.name.as_str().into());
    app.set_item_form_description(input.description.as_str().into());
    app.set_item_form_target(input.target.as_str().into());
    app.set_item_form_args(input.args.as_str().into());
    app.set_item_form_working_dir(input.working_dir.as_str().into());
    app.set_item_form_shell(input.shell.as_str().into());
    app.set_form_pre_delay_ms(input.pre_delay_ms.as_str().into());
    app.set_form_post_delay_ms(input.post_delay_ms.as_str().into());
    app.set_form_on_failure(input.on_failure.as_str().into());
}

fn set_group_form(
    app: &AppWindow,
    id: &str,
    name: &str,
    description: &str,
    pre_delay_ms: u64,
    post_delay_ms: u64,
    on_failure: FailurePolicy,
) {
    app.set_group_form_id(id.into());
    app.set_group_form_name(name.into());
    app.set_group_form_description(description.into());
    app.set_form_pre_delay_ms(pre_delay_ms.to_string().into());
    app.set_form_post_delay_ms(post_delay_ms.to_string().into());
    app.set_form_on_failure(failure_text(on_failure).into());
}

fn set_schedule_form_defaults(app: &AppWindow) {
    app.set_schedule_form_index("".into());
    app.set_schedule_form_kind("daily".into());
    app.set_schedule_form_time("09:00".into());
    app.set_schedule_form_weekday("星期一".into());
    app.set_schedule_form_at("2026-05-01T10:00:00".into());
}

fn set_schedule_form(app: &AppWindow, index: Option<usize>, schedule: &ScheduleRule) {
    app.set_schedule_form_index(
        index
            .map(|index| index.to_string())
            .unwrap_or_default()
            .into(),
    );
    match schedule {
        ScheduleRule::Daily { time } => {
            app.set_schedule_form_kind("daily".into());
            app.set_schedule_form_time(time.as_str().into());
            app.set_schedule_form_weekday("星期一".into());
            app.set_schedule_form_at("2026-05-01T10:00:00".into());
        }
        ScheduleRule::Weekly { weekday, time } => {
            app.set_schedule_form_kind("weekly".into());
            app.set_schedule_form_time(time.as_str().into());
            app.set_schedule_form_weekday(weekday_value(*weekday).into());
            app.set_schedule_form_at("2026-05-01T10:00:00".into());
        }
        ScheduleRule::Once { at } => {
            app.set_schedule_form_kind("once".into());
            app.set_schedule_form_time("09:00".into());
            app.set_schedule_form_weekday("星期一".into());
            app.set_schedule_form_at(at.as_str().into());
        }
    }
}

fn shell_text(shell: launcher_core::CommandShell) -> &'static str {
    match shell {
        launcher_core::CommandShell::PowerShell => "PowerShell",
        launcher_core::CommandShell::Cmd => "命令提示符",
        launcher_core::CommandShell::Sh => "Sh",
    }
}

fn next_plan_defaults(state: &Arc<Mutex<GuiState>>) -> (String, String) {
    let state = state.lock().expect("GUI state lock poisoned");
    let mut index = 1;
    loop {
        let id = if index == 1 {
            "new-plan".to_string()
        } else {
            format!("new-plan-{index}")
        };
        let exists = state
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.global.plans.iter().any(|plan| plan.id == id));
        if !exists {
            return (id.clone(), format!("plans/{id}.json"));
        }
        index += 1;
    }
}

fn selected_plan_name(state: &Arc<Mutex<GuiState>>, plan_id: &str) -> String {
    let state = state.lock().expect("GUI state lock poisoned");
    state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.plans.iter().find(|plan| plan.id == plan_id))
        .map(|plan| plan.name.clone())
        .unwrap_or_default()
}

#[derive(Default)]
struct ModalPlanInfo {
    id: String,
    name: String,
    file: String,
}

fn modal_plan_info(state: &GuiState) -> ModalPlanInfo {
    let Some(plan_id) = &state.modal_plan_id else {
        return ModalPlanInfo::default();
    };
    let Some(workspace) = &state.workspace else {
        return ModalPlanInfo {
            id: plan_id.clone(),
            ..Default::default()
        };
    };

    let name = workspace
        .plans
        .iter()
        .find(|plan| &plan.id == plan_id)
        .map(|plan| plan.name.clone())
        .unwrap_or_default();
    let file = workspace
        .global
        .plans
        .iter()
        .find(|plan| &plan.id == plan_id)
        .map(|plan| plan.file.clone())
        .unwrap_or_default();

    ModalPlanInfo {
        id: plan_id.clone(),
        name,
        file,
    }
}

fn context_plan_enabled(state: &GuiState) -> bool {
    let Some(plan_id) = &state.context_plan_id else {
        return true;
    };
    state
        .workspace
        .as_ref()
        .and_then(|workspace| {
            workspace
                .global
                .plans
                .iter()
                .find(|plan| &plan.id == plan_id)
        })
        .is_none_or(|plan| plan.enabled)
}

fn modal_kind_text(modal: ModalState) -> &'static str {
    match modal {
        ModalState::None => "",
        ModalState::CreatePlan => "create",
        ModalState::RenamePlan => "rename",
        ModalState::DeletePlan => "delete",
        ModalState::ImportConflict => "import-conflict",
        ModalState::AddItem => "add-item",
        ModalState::EditItem => "edit-item",
        ModalState::EditGroup => "edit-group",
        ModalState::CombineGroup => "combine-group",
        ModalState::Schedule => "schedule",
    }
}

fn load_workspace_into_state(state: &Arc<Mutex<GuiState>>) {
    let data_dir = default_data_dir();
    let result = load_workspace(&data_dir).and_then(|workspace| {
        validate_workspace(&workspace)?;
        Ok(workspace)
    });

    let mut state = state.lock().expect("GUI state lock poisoned");
    match result {
        Ok(workspace) => {
            let first_plan_id = workspace.plans.first().map(|plan| plan.id.clone());
            let selected_plan_exists = state
                .selected_plan_id
                .as_ref()
                .is_some_and(|id| workspace.plans.iter().any(|plan| &plan.id == id));
            if !selected_plan_exists {
                state.selected_plan_id = first_plan_id;
                state.selected_node_ids.clear();
            }
            state.workspace = Some(workspace);
            if state.logs.is_empty() {
                state.logs.push(log_row(
                    "info",
                    format!("已加载 JSON: {}", data_dir.display()),
                ));
            }
        }
        Err(error) => {
            state.workspace = None;
            state.selected_plan_id = None;
            state.selected_node_ids.clear();
            state.context_plan_id = None;
            state.modal = ModalState::None;
            state.modal_plan_id = None;
            state.modal_node_id = None;
            state.modal_error = None;
            state.pending_import_path = None;
            state.pending_import_name = None;
            state.pending_import_file = None;
            state
                .logs
                .push(log_row("error", format!("加载失败: {error}")));
        }
    }
}

fn render(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    let state = state.lock().expect("GUI state lock poisoned");
    let data_dir = default_data_dir();
    app.set_data_dir(data_dir.display().to_string().into());
    app.set_running(state.running);
    app.set_context_menu_visible(state.context_plan_id.is_some());
    app.set_context_plan_id(state.context_plan_id.clone().unwrap_or_default().into());
    app.set_context_plan_enabled(context_plan_enabled(&state));
    app.set_modal_kind(modal_kind_text(state.modal).into());
    app.set_modal_error(state.modal_error.clone().unwrap_or_default().into());
    app.set_modal_node_id(state.modal_node_id.clone().unwrap_or_default().into());
    app.set_import_conflict_name(state.pending_import_name.clone().unwrap_or_default().into());
    app.set_import_conflict_file(state.pending_import_file.clone().unwrap_or_default().into());
    app.set_import_conflict_source(
        state
            .pending_import_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
            .into(),
    );
    let modal_plan = modal_plan_info(&state);
    app.set_modal_plan_id(modal_plan.id.into());
    app.set_modal_plan_name(modal_plan.name.into());
    app.set_modal_plan_file(modal_plan.file.into());

    let Some(workspace) = &state.workspace else {
        app.set_selected_plan_title("未加载方案".into());
        app.set_selected_plan_file("请检查 data/global.json 与 plans/*.json".into());
        app.set_plan_rows(model_from(Vec::new()));
        app.set_sequence_rows(model_from(Vec::new()));
        app.set_has_selection(false);
        app.set_selection_summary("未选择启动项".into());
        app.set_launch_rows(model_from(Vec::new()));
        app.set_schedule_visible(false);
        app.set_schedule_rows(model_from(Vec::new()));
        app.set_log_rows(model_from(state.logs.clone()));
        return;
    };

    let selected_plan_id = selected_plan_id(&state, workspace);
    let selected_plan = selected_plan_id.and_then(|id| workspace.plans.iter().find(|p| p.id == id));
    let selected_entry =
        selected_plan_id.and_then(|id| workspace.global.plans.iter().find(|p| p.id == id));

    app.set_plan_rows(model_from(plan_rows(workspace, selected_plan_id)));
    app.set_sequence_rows(model_from(sequence_rows(
        selected_plan,
        &state.selected_node_ids,
    )));
    app.set_has_selection(!state.selected_node_ids.is_empty());
    app.set_selection_summary(selection_summary(&state.selected_node_ids));
    app.set_launch_rows(model_from(launch_rows(selected_entry)));
    app.set_schedule_visible(
        selected_entry.is_some_and(|entry| entry.launch.trigger == LaunchTrigger::AutoOnAppStart),
    );
    app.set_schedule_rows(model_from(schedule_rows(selected_entry)));
    app.set_log_rows(model_from(state.logs.clone()));

    app.set_selected_plan_title(
        selected_plan
            .map(|plan| format!("{} ({})", plan.name, plan.id))
            .unwrap_or_else(|| "暂无方案".to_string())
            .into(),
    );
    app.set_selected_plan_file(
        selected_entry
            .map(|entry| format!("data/{}", entry.file))
            .unwrap_or_else(|| "未选择方案文件".to_string())
            .into(),
    );
}

fn start_run(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    let plan_id = {
        let state = state.lock().expect("GUI state lock poisoned");
        state
            .workspace
            .as_ref()
            .and_then(|workspace| selected_plan_id(&state, workspace))
            .map(str::to_string)
    };
    let Some(plan_id) = plan_id else {
        append_log(state, "error", "没有可运行的方案");
        render(app, state);
        return;
    };

    if state.lock().expect("GUI state lock poisoned").running {
        return;
    }

    {
        let mut state = state.lock().expect("GUI state lock poisoned");
        state.running = true;
        state
            .logs
            .push(log_row("info", format!("开始运行方案: {plan_id}")));
    }
    render(app, state);

    let weak = app.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let result = run_plan_by_id(&plan_id);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                {
                    let mut state = state.lock().expect("GUI state lock poisoned");
                    state.running = false;
                    state.selected_plan_id = Some(plan_id.clone());
                    state.logs.extend(report_logs(result));
                }
                load_workspace_into_state(&state);
                render(&app, &state);
            }
        });
    });
}

fn run_plan_by_id(plan_id: &str) -> std::result::Result<ExecutionReport, String> {
    let data_dir = default_data_dir();
    let workspace = load_workspace(&data_dir)
        .and_then(|workspace| {
            validate_workspace(&workspace)?;
            Ok(workspace)
        })
        .map_err(|error| error.to_string())?;
    let plan = workspace
        .plans
        .iter()
        .find(|plan| plan.id == plan_id)
        .ok_or_else(|| format!("方案不存在: {plan_id}"))?;
    Ok(execute_plan(plan, ExecuteOptions { dry_run: false }))
}

fn start_scheduler_loop(app: &AppWindow, state: &Arc<Mutex<GuiState>>) {
    let weak = app.as_weak();
    let state = Arc::clone(state);
    thread::spawn(move || {
        let mut scheduler = Scheduler::new();
        let mut last_load_error: Option<String> = None;

        loop {
            if is_running(&state) {
                thread::sleep(Duration::from_secs(10));
                continue;
            }

            let due_plans = scheduled_due_plans(&mut scheduler);

            match due_plans {
                Ok(due_plans) => {
                    last_load_error = None;
                    for due in due_plans {
                        if !begin_scheduled_run(&state, &due.plan_id, &due.reason) {
                            continue;
                        }

                        let plan_id = due.plan_id.clone();
                        let before_state = Arc::clone(&state);
                        let before_weak = weak.clone();
                        if slint::invoke_from_event_loop(move || {
                            if let Some(app) = before_weak.upgrade() {
                                render(&app, &before_state);
                            }
                        })
                        .is_err()
                        {
                            return;
                        }

                        let result = run_plan_by_id(&plan_id);
                        {
                            let mut state = state.lock().expect("GUI state lock poisoned");
                            state.running = false;
                            state.selected_plan_id = Some(plan_id.clone());
                            state.logs.extend(report_logs(result));
                        }
                        let complete_state = Arc::clone(&state);
                        let complete_weak = weak.clone();
                        if slint::invoke_from_event_loop(move || {
                            if let Some(app) = complete_weak.upgrade() {
                                load_workspace_into_state(&complete_state);
                                render(&app, &complete_state);
                            }
                        })
                        .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let error = error.to_string();
                    if last_load_error.as_deref() != Some(error.as_str()) {
                        last_load_error = Some(error.clone());
                        let error_state = Arc::clone(&state);
                        let error_weak = weak.clone();
                        if slint::invoke_from_event_loop(move || {
                            if let Some(app) = error_weak.upgrade() {
                                append_log(
                                    &error_state,
                                    "error",
                                    format!("定时调度加载失败: {error}"),
                                );
                                render(&app, &error_state);
                            }
                        })
                        .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            thread::sleep(Duration::from_secs(10));
        }
    });
}

fn is_running(state: &Arc<Mutex<GuiState>>) -> bool {
    state.lock().expect("GUI state lock poisoned").running
}

fn scheduled_due_plans(
    scheduler: &mut Scheduler,
) -> launcher_core::Result<Vec<launcher_core::DuePlan>> {
    let data_dir = default_data_dir();
    let workspace = load_workspace(&data_dir)?;
    validate_workspace(&workspace)?;
    Ok(scheduler.due_now(&workspace.global))
}

fn begin_scheduled_run(state: &Arc<Mutex<GuiState>>, plan_id: &str, reason: &str) -> bool {
    let mut state = state.lock().expect("GUI state lock poisoned");
    if state.running {
        state.logs.push(log_row(
            "info",
            format!("跳过定时触发: {plan_id} ({reason})，当前已有方案运行中"),
        ));
        return false;
    }
    state.running = true;
    state.selected_plan_id = Some(plan_id.to_string());
    state.logs.push(log_row(
        "info",
        format!("定时触发运行方案: {plan_id} ({reason})"),
    ));
    true
}

fn report_logs(result: std::result::Result<ExecutionReport, String>) -> Vec<LogRow> {
    match result {
        Ok(report) => {
            let mut rows = vec![log_row(
                if report.failure_count() == 0 {
                    "info"
                } else {
                    "error"
                },
                format!(
                    "运行完成: 方案={} 成功={} 失败={} 已停止={}",
                    report.plan_id,
                    report.success_count(),
                    report.failure_count(),
                    report.stopped
                ),
            )];
            for item in report.items {
                let scope = item
                    .group_id
                    .map(|group_id| format!("{group_id}/{}", item.item_id))
                    .unwrap_or(item.item_id);
                rows.push(log_row(
                    if item.success { "info" } else { "error" },
                    format!(
                        "{}  {}  {}",
                        if item.success { "成功" } else { "失败" },
                        scope,
                        item.message
                    ),
                ));
            }
            rows
        }
        Err(error) => vec![log_row("error", format!("运行失败: {error}"))],
    }
}

fn selected_plan_id<'a>(state: &'a GuiState, workspace: &'a Workspace) -> Option<&'a str> {
    state
        .selected_plan_id
        .as_deref()
        .or_else(|| workspace.plans.first().map(|plan| plan.id.as_str()))
}

fn plan_rows(workspace: &Workspace, selected_plan_id: Option<&str>) -> Vec<PlanRow> {
    workspace
        .global
        .plans
        .iter()
        .map(|entry| {
            let name = workspace
                .plans
                .iter()
                .find(|plan| plan.id == entry.id)
                .map(|plan| plan.name.clone())
                .unwrap_or_else(|| "<缺失>".to_string());
            PlanRow {
                id: entry.id.clone().into(),
                name: name.into(),
                meta: plan_meta(entry).into(),
                selected: selected_plan_id == Some(entry.id.as_str()),
                enabled: entry.enabled,
            }
        })
        .collect()
}

fn sequence_rows(plan: Option<&Plan>, selected_node_ids: &[String]) -> Vec<SequenceRow> {
    let Some(plan) = plan else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    for node in &plan.sequence {
        match node {
            SequenceNode::Group(group) => {
                rows.push(group_sequence_row(group, selected_node_ids));
            }
            SequenceNode::Item(item) => {
                rows.push(item_sequence_row(item, 0, selected_node_ids));
            }
        }
    }
    rows
}

fn group_sequence_row(group: &Group, selected_node_ids: &[String]) -> SequenceRow {
    let mut child_rows = group.items.iter().map(child_row);
    let (child_primary_title, child_primary_meta) = child_rows
        .next()
        .unwrap_or_else(|| ("无内部启动项".to_string(), String::new()));
    let (child_secondary_title, child_secondary_meta) = child_rows
        .next()
        .unwrap_or_else(|| (String::new(), String::new()));
    let child_more = if group.items.len() > 2 {
        format!("另有 {} 项内部启动项", group.items.len() - 2)
    } else {
        String::new()
    };

    SequenceRow {
        id: group.id.clone().into(),
        kind: "group".into(),
        kind_label: "组".into(),
        title: format!("{} · {}", group.name, group.id).into(),
        description: group_description(group).into(),
        delay: format!(
            "组延迟 {}",
            delay_text(group.pre_delay_ms, group.post_delay_ms)
        )
        .into(),
        failure: failure_text(group.on_failure).into(),
        target: "内部失败策略各自生效".into(),
        child_primary_title: child_primary_title.into(),
        child_primary_meta: child_primary_meta.into(),
        child_secondary_title: child_secondary_title.into(),
        child_secondary_meta: child_secondary_meta.into(),
        child_more: child_more.into(),
        depth: 0,
        selected: selected_node_ids.iter().any(|id| id == &group.id),
    }
}

fn item_sequence_row(item: &LaunchItem, depth: i32, selected_node_ids: &[String]) -> SequenceRow {
    SequenceRow {
        id: item.id.clone().into(),
        kind: "item".into(),
        kind_label: target_kind(&item.target).into(),
        title: format!("{} · {}", item.name, item.id).into(),
        description: item_description(item).into(),
        delay: delay_text(item.pre_delay_ms, item.post_delay_ms).into(),
        failure: failure_text(item.on_failure).into(),
        target: target_summary(&item.target).into(),
        child_primary_title: "".into(),
        child_primary_meta: "".into(),
        child_secondary_title: "".into(),
        child_secondary_meta: "".into(),
        child_more: "".into(),
        depth,
        selected: selected_node_ids.iter().any(|id| id == &item.id),
    }
}

fn group_description(group: &Group) -> String {
    if group.description.is_empty() {
        format!(
            "描述: 组合了 {} 个单个启动项；只选择组合本身，内部子项不显示复选框。",
            group.items.len()
        )
    } else {
        format!("描述: {}", group.description)
    }
}

fn item_description(item: &LaunchItem) -> String {
    if item.description.is_empty() {
        "描述: 无".to_string()
    } else {
        format!("描述: {}", item.description)
    }
}

fn child_row(item: &LaunchItem) -> (String, String) {
    (
        format!(
            "{} · {} · {}",
            target_kind(&item.target),
            item.name,
            compact_target_text(&item.target)
        ),
        format!(
            "{} · 延迟 {} · {}",
            item_description(item),
            compact_delay_text(item.pre_delay_ms, item.post_delay_ms),
            failure_label(item.on_failure)
        ),
    )
}

fn compact_target_text(target: &LaunchTarget) -> String {
    match target {
        LaunchTarget::Path { value } => format!("目标 {value}"),
        LaunchTarget::Program { value, .. } => format!("目标 {value}"),
        LaunchTarget::Url { value } => format!("目标 {value}"),
        LaunchTarget::Command { value, .. } => format!("目标 {value}"),
    }
}

fn compact_delay_text(pre_delay_ms: u64, post_delay_ms: u64) -> String {
    match (pre_delay_ms, post_delay_ms) {
        (0, 0) => "0ms".to_string(),
        (0, post) => format!("后 {post}ms"),
        (pre, 0) => format!("前 {pre}ms"),
        (pre, post) => format!("前 {pre}ms / 后 {post}ms"),
    }
}

fn delay_text(pre_delay_ms: u64, post_delay_ms: u64) -> String {
    format!("前 {pre_delay_ms}ms / 后 {post_delay_ms}ms")
}

fn target_summary(target: &LaunchTarget) -> String {
    match target {
        LaunchTarget::Path { value } => format!("目标路径: {value}"),
        LaunchTarget::Program { value, .. } => format!("目标程序: {value}"),
        LaunchTarget::Url { value } => format!("目标链接: {value}"),
        LaunchTarget::Command { value, .. } => format!("目标命令: {value}"),
    }
}

fn target_kind(target: &LaunchTarget) -> &'static str {
    match target {
        LaunchTarget::Path { .. } => "路径",
        LaunchTarget::Program { .. } => "程序",
        LaunchTarget::Url { .. } => "链接",
        LaunchTarget::Command { .. } => "命令",
    }
}

fn selection_summary(selected_node_ids: &[String]) -> SharedString {
    if selected_node_ids.is_empty() {
        return "未选择启动项".into();
    }

    format!("已选择 {} 项", selected_node_ids.len()).into()
}

fn launch_rows(entry: Option<&PlanCatalogEntry>) -> Vec<InspectorRow> {
    let Some(entry) = entry else {
        return Vec::new();
    };

    vec![
        inspector_row(
            "手动点击",
            if matches!(entry.launch.trigger, LaunchTrigger::Manual) {
                "selected"
            } else {
                ""
            },
        ),
        inspector_row(
            "应用启动自动",
            if matches!(entry.launch.trigger, LaunchTrigger::AutoOnAppStart) {
                "selected"
            } else {
                ""
            },
        ),
    ]
}

fn schedule_rows(entry: Option<&PlanCatalogEntry>) -> Vec<InspectorRow> {
    let Some(entry) = entry else {
        return Vec::new();
    };

    if entry.launch.schedules.is_empty() {
        return vec![inspector_row("", "无定时配置")];
    }

    entry
        .launch
        .schedules
        .iter()
        .enumerate()
        .map(|(index, schedule)| inspector_row(index.to_string(), schedule_text(schedule)))
        .collect()
}

fn plan_meta(entry: &PlanCatalogEntry) -> String {
    format!(
        "状态={} · 启动={} · 定时={} 条",
        if entry.enabled { "启用" } else { "禁用" },
        trigger_text(entry.launch.trigger),
        entry.launch.schedules.len()
    )
}

fn trigger_text(trigger: LaunchTrigger) -> &'static str {
    match trigger {
        LaunchTrigger::Manual => "手动点击",
        LaunchTrigger::AutoOnAppStart => "应用启动自动",
    }
}

fn failure_text(policy: FailurePolicy) -> String {
    match policy {
        FailurePolicy::Continue => "continue".to_string(),
        FailurePolicy::Stop => "stop".to_string(),
    }
}

fn failure_label(policy: FailurePolicy) -> &'static str {
    match policy {
        FailurePolicy::Continue => "继续",
        FailurePolicy::Stop => "停止",
    }
}

fn schedule_text(schedule: &ScheduleRule) -> String {
    match schedule {
        ScheduleRule::Daily { time } => format!("每天  {time}"),
        ScheduleRule::Weekly { weekday, time } => {
            format!("每周{}  {time}", weekday_text(*weekday))
        }
        ScheduleRule::Once { at } => format!("单次  {at}"),
    }
}

fn weekday_text(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "一",
        Weekday::Tuesday => "二",
        Weekday::Wednesday => "三",
        Weekday::Thursday => "四",
        Weekday::Friday => "五",
        Weekday::Saturday => "六",
        Weekday::Sunday => "日",
    }
}

fn weekday_value(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "星期一",
        Weekday::Tuesday => "星期二",
        Weekday::Wednesday => "星期三",
        Weekday::Thursday => "星期四",
        Weekday::Friday => "星期五",
        Weekday::Saturday => "星期六",
        Weekday::Sunday => "星期日",
    }
}

fn append_log(
    state: &Arc<Mutex<GuiState>>,
    level: impl Into<SharedString>,
    message: impl Into<SharedString>,
) {
    state
        .lock()
        .expect("GUI state lock poisoned")
        .logs
        .push(LogRow {
            level: level.into(),
            message: message.into(),
        });
}

fn log_row(level: impl Into<SharedString>, message: impl Into<SharedString>) -> LogRow {
    LogRow {
        level: level.into(),
        message: message.into(),
    }
}

fn inspector_row(label: impl Into<SharedString>, value: impl Into<SharedString>) -> InspectorRow {
    InspectorRow {
        label: label.into(),
        value: value.into(),
    }
}

fn model_from<T: Clone + 'static>(values: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(values)))
}
