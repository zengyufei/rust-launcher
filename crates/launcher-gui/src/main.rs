use std::rc::Rc;

use launcher_core::{
    default_data_dir, execute_plan, load_workspace, validate_workspace, ExecuteOptions, Plan,
    SequenceNode,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = AppWindow::new()?;
    refresh_view(&app);

    let weak = app.as_weak();
    app.on_refresh(move || {
        if let Some(app) = weak.upgrade() {
            refresh_view(&app);
        }
    });

    let weak = app.as_weak();
    app.on_run_dry(move || {
        if let Some(app) = weak.upgrade() {
            run_first_plan_dry(&app);
        }
    });

    app.run()?;
    Ok(())
}

fn refresh_view(app: &AppWindow) {
    let data_dir = default_data_dir();
    match load_workspace(&data_dir).and_then(|workspace| {
        validate_workspace(&workspace)?;
        Ok(workspace)
    }) {
        Ok(workspace) => {
            let plan_names = workspace
                .plans
                .iter()
                .map(|plan| format!("{}  {}", plan.id, plan.name))
                .collect::<Vec<_>>();
            let details = workspace
                .plans
                .first()
                .map(plan_details)
                .unwrap_or_else(|| "暂无方案".to_string());
            let launch_modes = workspace
                .global
                .plans
                .iter()
                .map(|entry| {
                    format!(
                        "{}: enabled={} trigger={:?} schedules={}",
                        entry.id,
                        entry.enabled,
                        entry.launch.trigger,
                        entry.launch.schedules.len()
                    )
                })
                .collect::<Vec<_>>();

            app.set_data_dir(data_dir.display().to_string().into());
            app.set_plan_names(model_from_strings(plan_names));
            app.set_launch_modes(model_from_strings(launch_modes));
            app.set_plan_details(details.into());
            app.set_status("已加载 JSON。GUI 当前是审核稿，支持刷新和 dry-run 预览。".into());
        }
        Err(error) => {
            app.set_data_dir(data_dir.display().to_string().into());
            app.set_plan_names(model_from_strings(Vec::new()));
            app.set_launch_modes(model_from_strings(Vec::new()));
            app.set_plan_details("无法加载方案".into());
            app.set_status(format!("错误: {error}").into());
        }
    }
}

fn run_first_plan_dry(app: &AppWindow) {
    let data_dir = default_data_dir();
    match load_workspace(&data_dir).and_then(|workspace| {
        validate_workspace(&workspace)?;
        Ok(workspace)
    }) {
        Ok(workspace) => {
            if let Some(plan) = workspace.plans.first() {
                let report = execute_plan(plan, ExecuteOptions { dry_run: true });
                app.set_status(
                    format!(
                        "dry-run: plan={} success={} failure={} stopped={}",
                        report.plan_id,
                        report.success_count(),
                        report.failure_count(),
                        report.stopped
                    )
                    .into(),
                );
            } else {
                app.set_status("没有可运行的方案".into());
            }
        }
        Err(error) => app.set_status(format!("错误: {error}").into()),
    }
}

fn plan_details(plan: &Plan) -> String {
    let mut lines = vec![format!("{} ({})", plan.name, plan.id)];
    for node in &plan.sequence {
        match node {
            SequenceNode::Group(group) => {
                lines.push(format!(
                    "组: {} [{}] pre={}ms post={}ms",
                    group.name, group.id, group.pre_delay_ms, group.post_delay_ms
                ));
                for item in &group.items {
                    lines.push(format!(
                        "  - {} [{}] {} · {} on_failure={:?}",
                        item.name,
                        item.id,
                        item.description,
                        item.target.summary(),
                        item.on_failure
                    ));
                }
            }
            SequenceNode::Item(item) => {
                lines.push(format!(
                    "单个: {} [{}] {} on_failure={:?}",
                    item.name,
                    item.id,
                    item.target.summary(),
                    item.on_failure
                ));
            }
        }
    }
    lines.join("\n")
}

fn model_from_strings(values: Vec<String>) -> ModelRc<SharedString> {
    let values = values
        .into_iter()
        .map(SharedString::from)
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(values)))
}
