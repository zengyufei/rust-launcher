use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use launcher_core::{
    create_plan, create_plan_with_file, default_data_dir, execute_plan, execute_single_item,
    load_global, load_plan, load_workspace, save_global, save_plan, validate_workspace,
    CommandShell, ExecuteOptions, ExecutionReport, FailurePolicy, GlobalConfig, Group, LaunchItem,
    LaunchTarget, LaunchTrigger, LauncherError, Plan, PlanCatalogEntry, ScheduleRule, Scheduler,
    SequenceNode, Weekday, Workspace,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Parser)]
#[command(name = "launcher", version, about = "JSON driven Rust launcher")]
struct Cli {
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate,
    List,
    Run {
        plan_id: String,
        #[arg(long)]
        dry_run: bool,
    },
    RunItem {
        plan_id: String,
        item_id: String,
        #[arg(long)]
        dry_run: bool,
    },
    Daemon,
    NewPlan {
        id: String,
        name: String,
    },
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    Launch {
        #[command(subcommand)]
        command: LaunchCommand,
    },
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommand,
    },
    Sequence {
        #[command(subcommand)]
        command: SequenceCommand,
    },
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    Item {
        #[command(subcommand)]
        command: ItemCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PlanCommand {
    List,
    New {
        id: String,
        name: String,
        #[arg(long)]
        file: Option<String>,
    },
    Rename {
        id: String,
        name: String,
    },
    Delete {
        id: String,
        #[arg(long)]
        delete_file: bool,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Move {
        id: String,
        #[arg(value_enum)]
        direction: MoveDirection,
    },
}

#[derive(Debug, Subcommand)]
enum LaunchCommand {
    Show {
        plan_id: String,
    },
    Set {
        plan_id: String,
        #[arg(value_enum)]
        trigger: CliTrigger,
    },
}

#[derive(Debug, Subcommand)]
enum ScheduleCommand {
    List {
        plan_id: String,
    },
    AddDaily {
        plan_id: String,
        time: String,
    },
    AddWeekly {
        plan_id: String,
        #[arg(value_enum)]
        weekday: CliWeekday,
        time: String,
    },
    AddOnce {
        plan_id: String,
        at: String,
    },
    Remove {
        plan_id: String,
        index: usize,
    },
}

#[derive(Debug, Subcommand)]
enum SequenceCommand {
    List { plan_id: String },
}

#[derive(Debug, Subcommand)]
enum GroupCommand {
    Add {
        plan_id: String,
        id: String,
        name: String,
        #[command(flatten)]
        options: NodeOptions,
    },
    Edit {
        plan_id: String,
        group_id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        pre_delay_ms: Option<u64>,
        #[arg(long)]
        post_delay_ms: Option<u64>,
        #[arg(long, value_enum)]
        on_failure: Option<CliFailurePolicy>,
    },
    Delete {
        plan_id: String,
        group_id: String,
        #[arg(long)]
        keep_items: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ItemCommand {
    AddPath {
        plan_id: String,
        id: String,
        name: String,
        value: String,
        #[command(flatten)]
        options: ItemOptions,
    },
    AddProgram {
        plan_id: String,
        id: String,
        name: String,
        value: String,
        #[arg(long = "arg")]
        args: Vec<String>,
        #[arg(long)]
        working_dir: Option<String>,
        #[command(flatten)]
        options: ItemOptions,
    },
    AddUrl {
        plan_id: String,
        id: String,
        name: String,
        value: String,
        #[command(flatten)]
        options: ItemOptions,
    },
    AddCommand {
        plan_id: String,
        id: String,
        name: String,
        value: String,
        #[arg(long, value_enum, default_value = "power-shell")]
        shell: CliShell,
        #[arg(long)]
        working_dir: Option<String>,
        #[command(flatten)]
        options: ItemOptions,
    },
    Delete {
        plan_id: String,
        item_id: String,
    },
}

#[derive(Debug, Clone, Args)]
struct NodeOptions {
    #[arg(long, default_value = "")]
    description: String,
    #[arg(long, default_value_t = 0)]
    pre_delay_ms: u64,
    #[arg(long, default_value_t = 0)]
    post_delay_ms: u64,
    #[arg(long, value_enum, default_value = "continue")]
    on_failure: CliFailurePolicy,
}

#[derive(Debug, Clone, Args)]
struct ItemOptions {
    #[command(flatten)]
    node: NodeOptions,
    #[arg(long)]
    group: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MoveDirection {
    Top,
    Up,
    Down,
    Bottom,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliTrigger {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliFailurePolicy {
    Continue,
    Stop,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliWeekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliShell {
    #[value(name = "power-shell", alias = "powershell")]
    PowerShell,
    Cmd,
    Sh,
}

impl From<CliTrigger> for LaunchTrigger {
    fn from(value: CliTrigger) -> Self {
        match value {
            CliTrigger::Manual => Self::Manual,
            CliTrigger::Auto => Self::AutoOnAppStart,
        }
    }
}

impl From<CliFailurePolicy> for FailurePolicy {
    fn from(value: CliFailurePolicy) -> Self {
        match value {
            CliFailurePolicy::Continue => Self::Continue,
            CliFailurePolicy::Stop => Self::Stop,
        }
    }
}

impl From<CliWeekday> for Weekday {
    fn from(value: CliWeekday) -> Self {
        match value {
            CliWeekday::Monday => Self::Monday,
            CliWeekday::Tuesday => Self::Tuesday,
            CliWeekday::Wednesday => Self::Wednesday,
            CliWeekday::Thursday => Self::Thursday,
            CliWeekday::Friday => Self::Friday,
            CliWeekday::Saturday => Self::Saturday,
            CliWeekday::Sunday => Self::Sunday,
        }
    }
}

impl From<CliShell> for CommandShell {
    fn from(value: CliShell) -> Self {
        match value {
            CliShell::PowerShell => Self::PowerShell,
            CliShell::Cmd => Self::Cmd,
            CliShell::Sh => Self::Sh,
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "launcher=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let data_dir = cli.data_dir.unwrap_or_else(default_data_dir);

    match cli.command {
        Command::Validate => {
            let workspace = load_workspace(&data_dir)?;
            validate_workspace(&workspace)?;
            println!("OK: {}", workspace.data_dir.display());
        }
        Command::List => list_plans(&data_dir)?,
        Command::Run { plan_id, dry_run } => {
            let workspace = load_workspace(&data_dir)?;
            validate_workspace(&workspace)?;
            let plan = find_plan(&workspace.plans, &plan_id)?;
            let report = execute_plan(plan, ExecuteOptions { dry_run });
            print_report(&report);
        }
        Command::RunItem {
            plan_id,
            item_id,
            dry_run,
        } => {
            let workspace = load_workspace(&data_dir)?;
            validate_workspace(&workspace)?;
            let plan = find_plan(&workspace.plans, &plan_id)?;
            let report = execute_single_item(plan, &item_id, ExecuteOptions { dry_run })?;
            print_report(&report);
        }
        Command::Daemon => run_daemon(&data_dir)?,
        Command::NewPlan { id, name } => {
            let plan = create_plan(&data_dir, &id, &name)?;
            println!("created plan {} ({})", plan.id, plan.name);
        }
        Command::Plan { command } => run_plan_command(&data_dir, command)?,
        Command::Launch { command } => run_launch_command(&data_dir, command)?,
        Command::Schedule { command } => run_schedule_command(&data_dir, command)?,
        Command::Sequence { command } => run_sequence_command(&data_dir, command)?,
        Command::Group { command } => run_group_command(&data_dir, command)?,
        Command::Item { command } => run_item_command(&data_dir, command)?,
    }

    Ok(())
}

fn run_plan_command(data_dir: &Path, command: PlanCommand) -> Result<()> {
    match command {
        PlanCommand::List => list_plans(data_dir),
        PlanCommand::New { id, name, file } => {
            let plan = match file {
                Some(file) => create_plan_with_file(data_dir, &id, &name, &file)?,
                None => create_plan(data_dir, &id, &name)?,
            };
            println!("created plan {} ({})", plan.id, plan.name);
            Ok(())
        }
        PlanCommand::Rename { id, name } => {
            let (file, mut plan) = load_plan_by_id(data_dir, &id)?;
            plan.name = name;
            save_changed_plan(data_dir, &file, &plan)?;
            println!("renamed plan {}", plan.id);
            Ok(())
        }
        PlanCommand::Delete { id, delete_file } => {
            let mut global = load_global(data_dir)?;
            let index = find_plan_entry_index(&global, &id)?;
            let entry = global.plans.remove(index);
            save_changed_global(data_dir, &global, None)?;
            if delete_file {
                let path = data_dir.join(&entry.file);
                if path.exists() {
                    fs::remove_file(&path).map_err(|source| LauncherError::Io { path, source })?;
                }
            }
            println!("deleted plan catalog entry {id}");
            Ok(())
        }
        PlanCommand::Enable { id } => {
            let mut global = load_global(data_dir)?;
            find_plan_entry_mut(&mut global, &id)?.enabled = true;
            save_changed_global(data_dir, &global, None)?;
            println!("enabled plan {id}");
            Ok(())
        }
        PlanCommand::Disable { id } => {
            let mut global = load_global(data_dir)?;
            find_plan_entry_mut(&mut global, &id)?.enabled = false;
            save_changed_global(data_dir, &global, None)?;
            println!("disabled plan {id}");
            Ok(())
        }
        PlanCommand::Move { id, direction } => {
            let mut global = load_global(data_dir)?;
            move_plan_entry(&mut global, &id, direction)?;
            save_changed_global(data_dir, &global, None)?;
            println!("moved plan {id} {direction:?}");
            Ok(())
        }
    }
}

fn run_launch_command(data_dir: &Path, command: LaunchCommand) -> Result<()> {
    match command {
        LaunchCommand::Show { plan_id } => {
            let global = load_global(data_dir)?;
            let entry = find_plan_entry(&global, &plan_id)?;
            println!(
                "{}\tenabled={}\ttrigger={:?}\tschedules={}",
                entry.id,
                entry.enabled,
                entry.launch.trigger,
                entry.launch.schedules.len()
            );
            Ok(())
        }
        LaunchCommand::Set { plan_id, trigger } => {
            let mut global = load_global(data_dir)?;
            find_plan_entry_mut(&mut global, &plan_id)?.launch.trigger = trigger.into();
            save_changed_global(data_dir, &global, None)?;
            println!("set launch trigger for {plan_id}");
            Ok(())
        }
    }
}

fn run_schedule_command(data_dir: &Path, command: ScheduleCommand) -> Result<()> {
    match command {
        ScheduleCommand::List { plan_id } => {
            let global = load_global(data_dir)?;
            let entry = find_plan_entry(&global, &plan_id)?;
            print_schedules(&entry.launch.schedules);
            Ok(())
        }
        ScheduleCommand::AddDaily { plan_id, time } => {
            push_schedule(data_dir, &plan_id, ScheduleRule::Daily { time })
        }
        ScheduleCommand::AddWeekly {
            plan_id,
            weekday,
            time,
        } => push_schedule(
            data_dir,
            &plan_id,
            ScheduleRule::Weekly {
                weekday: weekday.into(),
                time,
            },
        ),
        ScheduleCommand::AddOnce { plan_id, at } => {
            push_schedule(data_dir, &plan_id, ScheduleRule::Once { at })
        }
        ScheduleCommand::Remove { plan_id, index } => {
            if index == 0 {
                return Err(validation_error("schedule index starts at 1"));
            }
            let mut global = load_global(data_dir)?;
            let entry = find_plan_entry_mut(&mut global, &plan_id)?;
            if index > entry.launch.schedules.len() {
                return Err(validation_error(format!(
                    "schedule index {index} is out of range"
                )));
            }
            entry.launch.schedules.remove(index - 1);
            save_changed_global(data_dir, &global, None)?;
            println!("removed schedule {index} from {plan_id}");
            Ok(())
        }
    }
}

fn run_sequence_command(data_dir: &Path, command: SequenceCommand) -> Result<()> {
    match command {
        SequenceCommand::List { plan_id } => {
            let (_, plan) = load_plan_by_id(data_dir, &plan_id)?;
            print_sequence(&plan);
            Ok(())
        }
    }
}

fn run_group_command(data_dir: &Path, command: GroupCommand) -> Result<()> {
    match command {
        GroupCommand::Add {
            plan_id,
            id,
            name,
            options,
        } => {
            let (file, mut plan) = load_plan_by_id(data_dir, &plan_id)?;
            ensure_unique_id(&plan, &id)?;
            plan.sequence.push(SequenceNode::Group(Group {
                id: id.clone(),
                name,
                description: options.description,
                pre_delay_ms: options.pre_delay_ms,
                post_delay_ms: options.post_delay_ms,
                on_failure: options.on_failure.into(),
                items: Vec::new(),
            }));
            save_changed_plan(data_dir, &file, &plan)?;
            println!("added group {id} to {plan_id}");
            Ok(())
        }
        GroupCommand::Edit {
            plan_id,
            group_id,
            name,
            description,
            pre_delay_ms,
            post_delay_ms,
            on_failure,
        } => {
            let (file, mut plan) = load_plan_by_id(data_dir, &plan_id)?;
            let group = find_group_mut(&mut plan, &group_id)?;
            if let Some(name) = name {
                group.name = name;
            }
            if let Some(description) = description {
                group.description = description;
            }
            if let Some(pre_delay_ms) = pre_delay_ms {
                group.pre_delay_ms = pre_delay_ms;
            }
            if let Some(post_delay_ms) = post_delay_ms {
                group.post_delay_ms = post_delay_ms;
            }
            if let Some(on_failure) = on_failure {
                group.on_failure = on_failure.into();
            }
            save_changed_plan(data_dir, &file, &plan)?;
            println!("edited group {group_id}");
            Ok(())
        }
        GroupCommand::Delete {
            plan_id,
            group_id,
            keep_items,
        } => {
            let (file, mut plan) = load_plan_by_id(data_dir, &plan_id)?;
            let index = find_group_index(&plan, &group_id)?;
            let SequenceNode::Group(group) = plan.sequence.remove(index) else {
                unreachable!();
            };
            if keep_items {
                for (offset, item) in group.items.into_iter().enumerate() {
                    plan.sequence
                        .insert(index + offset, SequenceNode::Item(item));
                }
            }
            save_changed_plan(data_dir, &file, &plan)?;
            println!("deleted group {group_id}");
            Ok(())
        }
    }
}

fn run_item_command(data_dir: &Path, command: ItemCommand) -> Result<()> {
    match command {
        ItemCommand::AddPath {
            plan_id,
            id,
            name,
            value,
            options,
        } => add_item(
            data_dir,
            &plan_id,
            options.group.as_deref(),
            build_item(id, name, LaunchTarget::Path { value }, options.node),
        ),
        ItemCommand::AddProgram {
            plan_id,
            id,
            name,
            value,
            args,
            working_dir,
            options,
        } => add_item(
            data_dir,
            &plan_id,
            options.group.as_deref(),
            build_item(
                id,
                name,
                LaunchTarget::Program {
                    value,
                    args,
                    working_dir,
                },
                options.node,
            ),
        ),
        ItemCommand::AddUrl {
            plan_id,
            id,
            name,
            value,
            options,
        } => add_item(
            data_dir,
            &plan_id,
            options.group.as_deref(),
            build_item(id, name, LaunchTarget::Url { value }, options.node),
        ),
        ItemCommand::AddCommand {
            plan_id,
            id,
            name,
            value,
            shell,
            working_dir,
            options,
        } => add_item(
            data_dir,
            &plan_id,
            options.group.as_deref(),
            build_item(
                id,
                name,
                LaunchTarget::Command {
                    value,
                    shell: shell.into(),
                    working_dir,
                },
                options.node,
            ),
        ),
        ItemCommand::Delete { plan_id, item_id } => {
            let (file, mut plan) = load_plan_by_id(data_dir, &plan_id)?;
            remove_item(&mut plan, &item_id)?;
            save_changed_plan(data_dir, &file, &plan)?;
            println!("deleted item {item_id}");
            Ok(())
        }
    }
}

fn add_item(
    data_dir: &Path,
    plan_id: &str,
    group_id: Option<&str>,
    item: LaunchItem,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    ensure_unique_id(&plan, &item.id)?;
    let item_id = item.id.clone();
    if let Some(group_id) = group_id {
        find_group_mut(&mut plan, group_id)?.items.push(item);
    } else {
        plan.sequence.push(SequenceNode::Item(item));
    }
    save_changed_plan(data_dir, &file, &plan)?;
    println!("added item {item_id} to {plan_id}");
    Ok(())
}

fn build_item(id: String, name: String, target: LaunchTarget, options: NodeOptions) -> LaunchItem {
    LaunchItem {
        id,
        name,
        description: options.description,
        target,
        pre_delay_ms: options.pre_delay_ms,
        post_delay_ms: options.post_delay_ms,
        on_failure: options.on_failure.into(),
    }
}

fn push_schedule(data_dir: &Path, plan_id: &str, schedule: ScheduleRule) -> Result<()> {
    let mut global = load_global(data_dir)?;
    find_plan_entry_mut(&mut global, plan_id)?
        .launch
        .schedules
        .push(schedule);
    save_changed_global(data_dir, &global, None)?;
    println!("added schedule to {plan_id}");
    Ok(())
}

fn run_daemon(data_dir: &Path) -> Result<()> {
    let mut scheduler = Scheduler::new();
    let mut workspace = load_workspace(data_dir)?;
    validate_workspace(&workspace)?;

    println!("daemon started: {}", data_dir.display());
    for due in Scheduler::auto_on_app_start(&workspace.global) {
        if let Some(plan) = workspace.plans.iter().find(|plan| plan.id == due.plan_id) {
            println!("auto start: {} ({})", due.plan_id, due.reason);
            let report = execute_plan(plan, ExecuteOptions { dry_run: false });
            print_report(&report);
        }
    }

    loop {
        workspace = load_workspace(data_dir)?;
        validate_workspace(&workspace)?;
        for due in scheduler.due_now(&workspace.global) {
            if let Some(plan) = workspace.plans.iter().find(|plan| plan.id == due.plan_id) {
                println!("scheduled start: {} ({})", due.plan_id, due.reason);
                let report = execute_plan(plan, ExecuteOptions { dry_run: false });
                print_report(&report);
            }
        }
        thread::sleep(Duration::from_secs(30));
    }
}

fn list_plans(data_dir: &Path) -> Result<()> {
    let workspace = load_workspace(data_dir)?;
    validate_workspace(&workspace)?;
    print_plan_list(&workspace.global.plans, &workspace.plans);
    Ok(())
}

fn load_plan_by_id(data_dir: &Path, plan_id: &str) -> Result<(String, Plan)> {
    let global = load_global(data_dir)?;
    let entry = find_plan_entry(&global, plan_id)?;
    Ok((entry.file.clone(), load_plan(data_dir, &entry.file)?))
}

fn save_changed_plan(data_dir: &Path, file: &str, plan: &Plan) -> Result<()> {
    let global = load_global(data_dir)?;
    validate_pending_workspace(data_dir, &global, Some(plan))?;
    save_plan(data_dir, file, plan)?;
    Ok(())
}

fn save_changed_global(
    data_dir: &Path,
    global: &GlobalConfig,
    changed_plan: Option<&Plan>,
) -> Result<()> {
    validate_pending_workspace(data_dir, global, changed_plan)?;
    save_global(data_dir, global)?;
    Ok(())
}

fn validate_pending_workspace(
    data_dir: &Path,
    global: &GlobalConfig,
    changed_plan: Option<&Plan>,
) -> Result<()> {
    let mut plans = Vec::new();
    for entry in &global.plans {
        if let Some(plan) = changed_plan.filter(|plan| plan.id == entry.id) {
            plans.push(plan.clone());
        } else {
            plans.push(load_plan(data_dir, &entry.file)?);
        }
    }
    validate_workspace(&Workspace {
        data_dir: data_dir.to_path_buf(),
        global: global.clone(),
        plans,
    })?;
    Ok(())
}

fn move_plan_entry(global: &mut GlobalConfig, id: &str, direction: MoveDirection) -> Result<()> {
    let index = find_plan_entry_index(global, id)?;
    let new_index = match direction {
        MoveDirection::Top => 0,
        MoveDirection::Up => index.saturating_sub(1),
        MoveDirection::Down => (index + 1).min(global.plans.len() - 1),
        MoveDirection::Bottom => global.plans.len() - 1,
    };
    if index != new_index {
        let entry = global.plans.remove(index);
        global.plans.insert(new_index, entry);
    }
    Ok(())
}

fn find_plan<'a>(plans: &'a [Plan], plan_id: &str) -> Result<&'a Plan> {
    plans
        .iter()
        .find(|plan| plan.id == plan_id)
        .ok_or_else(|| LauncherError::PlanNotFound(plan_id.to_string()).into())
}

fn find_plan_entry<'a>(global: &'a GlobalConfig, plan_id: &str) -> Result<&'a PlanCatalogEntry> {
    global
        .plans
        .iter()
        .find(|entry| entry.id == plan_id)
        .ok_or_else(|| LauncherError::PlanNotFound(plan_id.to_string()).into())
}

fn find_plan_entry_mut<'a>(
    global: &'a mut GlobalConfig,
    plan_id: &str,
) -> Result<&'a mut PlanCatalogEntry> {
    global
        .plans
        .iter_mut()
        .find(|entry| entry.id == plan_id)
        .ok_or_else(|| LauncherError::PlanNotFound(plan_id.to_string()).into())
}

fn find_plan_entry_index(global: &GlobalConfig, plan_id: &str) -> Result<usize> {
    global
        .plans
        .iter()
        .position(|entry| entry.id == plan_id)
        .ok_or_else(|| LauncherError::PlanNotFound(plan_id.to_string()).into())
}

fn find_group_mut<'a>(plan: &'a mut Plan, group_id: &str) -> Result<&'a mut Group> {
    plan.sequence
        .iter_mut()
        .find_map(|node| match node {
            SequenceNode::Group(group) if group.id == group_id => Some(group),
            _ => None,
        })
        .ok_or_else(|| validation_error(format!("group not found: {group_id}")))
}

fn find_group_index(plan: &Plan, group_id: &str) -> Result<usize> {
    plan.sequence
        .iter()
        .position(|node| matches!(node, SequenceNode::Group(group) if group.id == group_id))
        .ok_or_else(|| validation_error(format!("group not found: {group_id}")))
}

fn ensure_unique_id(plan: &Plan, id: &str) -> Result<()> {
    if plan.sequence.iter().any(|node| node.id() == id)
        || plan.sequence.iter().any(|node| match node {
            SequenceNode::Group(group) => group.items.iter().any(|item| item.id == id),
            SequenceNode::Item(_) => false,
        })
    {
        return Err(validation_error(format!("duplicate id in plan: {id}")));
    }
    Ok(())
}

fn remove_item(plan: &mut Plan, item_id: &str) -> Result<()> {
    if let Some(index) = plan
        .sequence
        .iter()
        .position(|node| matches!(node, SequenceNode::Item(item) if item.id == item_id))
    {
        plan.sequence.remove(index);
        return Ok(());
    }

    for node in &mut plan.sequence {
        if let SequenceNode::Group(group) = node {
            if let Some(index) = group.items.iter().position(|item| item.id == item_id) {
                group.items.remove(index);
                return Ok(());
            }
        }
    }

    Err(validation_error(format!("item not found: {item_id}")))
}

fn print_plan_list(entries: &[PlanCatalogEntry], plans: &[Plan]) {
    for entry in entries {
        let name = plans
            .iter()
            .find(|plan| plan.id == entry.id)
            .map(|plan| plan.name.as_str())
            .unwrap_or("<missing>");
        let schedule_count = entry.launch.schedules.len();
        println!(
            "{}\t{}\tenabled={}\ttrigger={:?}\tschedules={}\tfile={}",
            entry.id, name, entry.enabled, entry.launch.trigger, schedule_count, entry.file
        );
    }
}

fn print_schedules(schedules: &[ScheduleRule]) {
    for (index, schedule) in schedules.iter().enumerate() {
        match schedule {
            ScheduleRule::Daily { time } => {
                println!("{}\tdaily\t{}", index + 1, time);
            }
            ScheduleRule::Weekly { weekday, time } => {
                println!("{}\tweekly\t{:?}\t{}", index + 1, weekday, time);
            }
            ScheduleRule::Once { at } => {
                println!("{}\tonce\t{}", index + 1, at);
            }
        }
    }
}

fn print_sequence(plan: &Plan) {
    println!("{} ({})", plan.name, plan.id);
    for (index, node) in plan.sequence.iter().enumerate() {
        match node {
            SequenceNode::Group(group) => {
                println!(
                    "{}. [group] {} ({}) pre={}ms post={}ms failure={:?}",
                    index + 1,
                    group.name,
                    group.id,
                    group.pre_delay_ms,
                    group.post_delay_ms,
                    group.on_failure
                );
                for item in &group.items {
                    println!(
                        "   - [item] {} ({}) -> {}",
                        item.name,
                        item.id,
                        item.target.summary()
                    );
                }
            }
            SequenceNode::Item(item) => {
                println!(
                    "{}. [item] {} ({}) pre={}ms post={}ms failure={:?} -> {}",
                    index + 1,
                    item.name,
                    item.id,
                    item.pre_delay_ms,
                    item.post_delay_ms,
                    item.on_failure,
                    item.target.summary()
                );
            }
        }
    }
}

fn print_report(report: &ExecutionReport) {
    println!(
        "plan={} dry_run={} success={} failure={} stopped={}",
        report.plan_id,
        report.dry_run,
        report.success_count(),
        report.failure_count(),
        report.stopped
    );
    for item in &report.items {
        let scope = item
            .group_id
            .as_ref()
            .map(|group| format!("{group}/{}", item.item_id))
            .unwrap_or_else(|| item.item_id.clone());
        println!(
            "- [{}] {} -> {} ({})",
            if item.success { "ok" } else { "failed" },
            scope,
            item.target,
            item.message
        );
    }
}

fn validation_error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    LauncherError::Validation(message.into()).into()
}
