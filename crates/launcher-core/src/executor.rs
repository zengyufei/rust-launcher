use std::{thread, time::Duration};

use crate::{
    model::{FailurePolicy, LaunchItem, Plan, SequenceNode},
    platform::{LaunchAdapter, SystemLauncher},
    LauncherError, Result,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ExecuteOptions {
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    pub plan_id: String,
    pub dry_run: bool,
    pub items: Vec<ItemExecution>,
    pub stopped: bool,
}

impl ExecutionReport {
    pub fn success_count(&self) -> usize {
        self.items.iter().filter(|item| item.success).count()
    }

    pub fn failure_count(&self) -> usize {
        self.items.iter().filter(|item| !item.success).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemExecution {
    pub item_id: String,
    pub item_name: String,
    pub group_id: Option<String>,
    pub target: String,
    pub pre_delay_ms: u64,
    pub post_delay_ms: u64,
    pub success: bool,
    pub message: String,
}

pub fn execute_plan(plan: &Plan, options: ExecuteOptions) -> ExecutionReport {
    execute_plan_with_progress(plan, options, |_| {})
}

pub fn execute_plan_with_progress<F>(
    plan: &Plan,
    options: ExecuteOptions,
    on_item: F,
) -> ExecutionReport
where
    F: FnMut(&ItemExecution),
{
    execute_plan_with_adapter_and_progress(plan, options, &SystemLauncher, on_item)
}

pub fn execute_plan_with_adapter(
    plan: &Plan,
    options: ExecuteOptions,
    adapter: &dyn LaunchAdapter,
) -> ExecutionReport {
    execute_plan_with_adapter_and_progress(plan, options, adapter, |_| {})
}

pub fn execute_plan_with_adapter_and_progress<F>(
    plan: &Plan,
    options: ExecuteOptions,
    adapter: &dyn LaunchAdapter,
    mut on_item: F,
) -> ExecutionReport
where
    F: FnMut(&ItemExecution),
{
    let mut report = ExecutionReport {
        plan_id: plan.id.clone(),
        dry_run: options.dry_run,
        items: Vec::new(),
        stopped: false,
    };

    for node in &plan.sequence {
        match node {
            SequenceNode::Group(group) => {
                sleep_if_needed(group.pre_delay_ms, options.dry_run);
                for item in &group.items {
                    let execution = execute_item(item, Some(group.id.clone()), options, adapter);
                    on_item(&execution);
                    let should_stop = !execution.success && group.on_failure == FailurePolicy::Stop;
                    report.items.push(execution);
                    if should_stop {
                        report.stopped = true;
                        return report;
                    }
                }
                sleep_if_needed(group.post_delay_ms, options.dry_run);
            }
            SequenceNode::Item(item) => {
                let execution = execute_item(item, None, options, adapter);
                on_item(&execution);
                let should_stop = !execution.success && item.on_failure == FailurePolicy::Stop;
                report.items.push(execution);
                if should_stop {
                    report.stopped = true;
                    return report;
                }
            }
        }
    }

    report
}

pub fn execute_single_item(
    plan: &Plan,
    item_id: &str,
    options: ExecuteOptions,
) -> Result<ExecutionReport> {
    execute_single_item_with_progress(plan, item_id, options, |_| {})
}

pub fn execute_single_item_with_progress<F>(
    plan: &Plan,
    item_id: &str,
    options: ExecuteOptions,
    mut on_item: F,
) -> Result<ExecutionReport>
where
    F: FnMut(&ItemExecution),
{
    let item =
        find_item(plan, item_id).ok_or_else(|| LauncherError::ItemNotFound(item_id.to_string()))?;
    let execution = execute_item(item, None, options, &SystemLauncher);
    on_item(&execution);
    Ok(ExecutionReport {
        plan_id: plan.id.clone(),
        dry_run: options.dry_run,
        stopped: !execution.success && item.on_failure == FailurePolicy::Stop,
        items: vec![execution],
    })
}

fn execute_item(
    item: &LaunchItem,
    group_id: Option<String>,
    options: ExecuteOptions,
    adapter: &dyn LaunchAdapter,
) -> ItemExecution {
    sleep_if_needed(item.pre_delay_ms, options.dry_run);

    let result = if options.dry_run {
        Ok(())
    } else {
        adapter.launch(&item.target)
    };

    sleep_if_needed(item.post_delay_ms, options.dry_run);

    match result {
        Ok(()) => ItemExecution {
            item_id: item.id.clone(),
            item_name: item.name.clone(),
            group_id,
            target: item.target.summary(),
            pre_delay_ms: item.pre_delay_ms,
            post_delay_ms: item.post_delay_ms,
            success: true,
            message: if options.dry_run {
                "dry-run".to_string()
            } else {
                "launched".to_string()
            },
        },
        Err(error) => ItemExecution {
            item_id: item.id.clone(),
            item_name: item.name.clone(),
            group_id,
            target: item.target.summary(),
            pre_delay_ms: item.pre_delay_ms,
            post_delay_ms: item.post_delay_ms,
            success: false,
            message: error.to_string(),
        },
    }
}

fn sleep_if_needed(delay_ms: u64, dry_run: bool) {
    if delay_ms > 0 && !dry_run {
        thread::sleep(Duration::from_millis(delay_ms));
    }
}

fn find_item<'a>(plan: &'a Plan, item_id: &str) -> Option<&'a LaunchItem> {
    plan.sequence.iter().find_map(|node| match node {
        SequenceNode::Group(group) => group.items.iter().find(|item| item.id == item_id),
        SequenceNode::Item(item) if item.id == item_id => Some(item),
        SequenceNode::Item(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        executor::{
            execute_plan_with_adapter, execute_plan_with_adapter_and_progress, ExecuteOptions,
        },
        model::{FailurePolicy, LaunchItem, LaunchTarget, Plan, SequenceNode},
        platform::LaunchAdapter,
        LauncherError, Result,
    };

    #[derive(Default)]
    struct FakeLauncher;

    impl LaunchAdapter for FakeLauncher {
        fn launch(&self, target: &LaunchTarget) -> Result<()> {
            match target {
                LaunchTarget::Command { value, .. } if value == "fail" => {
                    Err(LauncherError::LaunchFailed {
                        item_id: "fake".to_string(),
                        message: "forced failure".to_string(),
                    })
                }
                _ => Ok(()),
            }
        }
    }

    #[test]
    fn dry_run_records_order_without_launching() {
        let plan = Plan {
            version: 2,
            id: "work".to_string(),
            name: "Work".to_string(),
            sequence: vec![SequenceNode::Item(item(
                "one",
                "ok",
                FailurePolicy::Continue,
            ))],
        };

        let report =
            execute_plan_with_adapter(&plan, ExecuteOptions { dry_run: true }, &FakeLauncher);

        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].item_id, "one");
        assert_eq!(report.items[0].message, "dry-run");
    }

    #[test]
    fn stop_policy_stops_after_failed_item() {
        let plan = Plan {
            version: 2,
            id: "work".to_string(),
            name: "Work".to_string(),
            sequence: vec![
                SequenceNode::Item(item("bad", "fail", FailurePolicy::Stop)),
                SequenceNode::Item(item("later", "ok", FailurePolicy::Continue)),
            ],
        };

        let report =
            execute_plan_with_adapter(&plan, ExecuteOptions { dry_run: false }, &FakeLauncher);

        assert!(report.stopped);
        assert_eq!(report.items.len(), 1);
        assert!(!report.items[0].success);
    }

    #[test]
    fn continue_policy_keeps_running_after_failed_item() {
        let plan = Plan {
            version: 2,
            id: "work".to_string(),
            name: "Work".to_string(),
            sequence: vec![
                SequenceNode::Item(item("bad", "fail", FailurePolicy::Continue)),
                SequenceNode::Item(item("later", "ok", FailurePolicy::Continue)),
            ],
        };

        let report =
            execute_plan_with_adapter(&plan, ExecuteOptions { dry_run: false }, &FakeLauncher);

        assert!(!report.stopped);
        assert_eq!(report.items.len(), 2);
        assert_eq!(report.failure_count(), 1);
        assert_eq!(report.success_count(), 1);
    }

    #[test]
    fn progress_callback_runs_in_execution_order() {
        let plan = Plan {
            version: 2,
            id: "work".to_string(),
            name: "Work".to_string(),
            sequence: vec![
                SequenceNode::Item(item("one", "ok", FailurePolicy::Continue)),
                SequenceNode::Item(item("two", "ok", FailurePolicy::Continue)),
            ],
        };
        let mut seen = Vec::new();

        let report = execute_plan_with_adapter_and_progress(
            &plan,
            ExecuteOptions { dry_run: false },
            &FakeLauncher,
            |item| seen.push(item.item_id.clone()),
        );

        assert_eq!(seen, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(report.items.len(), 2);
    }

    fn item(id: &str, command: &str, on_failure: FailurePolicy) -> LaunchItem {
        LaunchItem {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            target: LaunchTarget::Command {
                value: command.to_string(),
                shell: Default::default(),
                working_dir: None,
            },
            pre_delay_ms: 0,
            post_delay_ms: 0,
            on_failure,
        }
    }
}
