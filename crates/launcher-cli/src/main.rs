use std::{
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use launcher_core::{
    add_group, add_item as store_add_item, add_plan_schedule, create_plan, create_plan_with_file,
    default_data_dir, delete_group, delete_item, delete_plan, delete_plan_schedule, execute_plan,
    execute_single_item, export_plan, import_plan, load_global, load_plan, load_workspace,
    move_item, move_item_to_group, move_item_to_root, move_plan, move_sequence_node, rename_plan,
    set_plan_enabled, set_plan_launch_trigger, update_group, update_item, validate_workspace,
    CommandShell, ExecuteOptions, ExecutionReport, FailurePolicy, GlobalConfig, Group, GroupUpdate,
    ItemUpdate, LaunchItem, LaunchTarget, LaunchTrigger, LauncherError, NodeMoveDirection, Plan,
    PlanCatalogEntry, PlanMoveDirection, ScheduleRule, Scheduler, SequenceNode, Weekday,
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
    Export {
        id: String,
        output_path: PathBuf,
    },
    Import {
        source_path: PathBuf,
        #[arg(long)]
        overwrite: bool,
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
    List {
        plan_id: String,
    },
    Move {
        plan_id: String,
        node_id: String,
        #[arg(value_enum)]
        direction: MoveDirection,
    },
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
    Edit {
        plan_id: String,
        item_id: String,
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
    TargetPath {
        plan_id: String,
        item_id: String,
        value: String,
    },
    TargetProgram {
        plan_id: String,
        item_id: String,
        value: String,
        #[arg(long = "arg")]
        args: Vec<String>,
        #[arg(long)]
        working_dir: Option<String>,
    },
    TargetUrl {
        plan_id: String,
        item_id: String,
        value: String,
    },
    TargetCommand {
        plan_id: String,
        item_id: String,
        value: String,
        #[arg(long, value_enum, default_value = "power-shell")]
        shell: CliShell,
        #[arg(long)]
        working_dir: Option<String>,
    },
    Move {
        plan_id: String,
        item_id: String,
        #[arg(value_enum)]
        direction: MoveDirection,
    },
    MoveToGroup {
        plan_id: String,
        item_id: String,
        group_id: String,
    },
    MoveToRoot {
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

impl From<MoveDirection> for PlanMoveDirection {
    fn from(value: MoveDirection) -> Self {
        match value {
            MoveDirection::Top => Self::Top,
            MoveDirection::Up => Self::Up,
            MoveDirection::Down => Self::Down,
            MoveDirection::Bottom => Self::Bottom,
        }
    }
}

impl From<MoveDirection> for NodeMoveDirection {
    fn from(value: MoveDirection) -> Self {
        match value {
            MoveDirection::Top => Self::Top,
            MoveDirection::Up => Self::Up,
            MoveDirection::Down => Self::Down,
            MoveDirection::Bottom => Self::Bottom,
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
            let plan = rename_plan(data_dir, &id, &name)?;
            println!("renamed plan {}", plan.id);
            Ok(())
        }
        PlanCommand::Delete { id, delete_file } => {
            delete_plan(data_dir, &id, delete_file)?;
            println!("deleted plan catalog entry {id}");
            Ok(())
        }
        PlanCommand::Enable { id } => {
            set_plan_enabled(data_dir, &id, true)?;
            println!("enabled plan {id}");
            Ok(())
        }
        PlanCommand::Disable { id } => {
            set_plan_enabled(data_dir, &id, false)?;
            println!("disabled plan {id}");
            Ok(())
        }
        PlanCommand::Move { id, direction } => {
            move_plan(data_dir, &id, direction.into())?;
            println!("moved plan {id} {direction:?}");
            Ok(())
        }
        PlanCommand::Export { id, output_path } => {
            export_plan(data_dir, &id, &output_path)?;
            println!("exported plan {id} to {}", output_path.display());
            Ok(())
        }
        PlanCommand::Import {
            source_path,
            overwrite,
        } => match import_plan(data_dir, &source_path, overwrite) {
            Ok(plan) => {
                if overwrite {
                    println!("overwrote plan {} ({})", plan.id, plan.name);
                } else {
                    println!("imported plan {} ({})", plan.id, plan.name);
                }
                Ok(())
            }
            Err(LauncherError::PlanImportConflict { plan_id, .. }) => Err(Box::new(
                LauncherError::Validation(format!(
                    "plan import conflict: {plan_id}; rerun with --overwrite to replace the existing plan JSON"
                )),
            )),
            Err(error) => Err(Box::new(error)),
        },
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
            set_plan_launch_trigger(data_dir, &plan_id, trigger.into())?;
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
            add_schedule(data_dir, &plan_id, ScheduleRule::Daily { time })
        }
        ScheduleCommand::AddWeekly {
            plan_id,
            weekday,
            time,
        } => add_schedule(
            data_dir,
            &plan_id,
            ScheduleRule::Weekly {
                weekday: weekday.into(),
                time,
            },
        ),
        ScheduleCommand::AddOnce { plan_id, at } => {
            add_schedule(data_dir, &plan_id, ScheduleRule::Once { at })
        }
        ScheduleCommand::Remove { plan_id, index } => {
            if index == 0 {
                return Err(validation_error("schedule index starts at 1"));
            }
            delete_plan_schedule(data_dir, &plan_id, index - 1)?;
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
        SequenceCommand::Move {
            plan_id,
            node_id,
            direction,
        } => {
            move_sequence_node(data_dir, &plan_id, &node_id, direction.into())?;
            println!("moved node {node_id} {direction:?}");
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
            add_group(
                data_dir,
                &plan_id,
                Group {
                    id: id.clone(),
                    name,
                    description: options.description,
                    pre_delay_ms: options.pre_delay_ms,
                    post_delay_ms: options.post_delay_ms,
                    on_failure: options.on_failure.into(),
                    items: Vec::new(),
                },
            )?;
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
            update_group(
                data_dir,
                &plan_id,
                &group_id,
                GroupUpdate {
                    name,
                    description,
                    pre_delay_ms,
                    post_delay_ms,
                    on_failure: on_failure.map(Into::into),
                },
            )?;
            println!("edited group {group_id}");
            Ok(())
        }
        GroupCommand::Delete {
            plan_id,
            group_id,
            keep_items,
        } => {
            delete_group(data_dir, &plan_id, &group_id, keep_items)?;
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
            delete_item(data_dir, &plan_id, &item_id)?;
            println!("deleted item {item_id}");
            Ok(())
        }
        ItemCommand::Edit {
            plan_id,
            item_id,
            name,
            description,
            pre_delay_ms,
            post_delay_ms,
            on_failure,
        } => {
            update_item(
                data_dir,
                &plan_id,
                &item_id,
                ItemUpdate {
                    name,
                    description,
                    pre_delay_ms,
                    post_delay_ms,
                    on_failure: on_failure.map(Into::into),
                    target: None,
                },
            )?;
            println!("edited item {item_id}");
            Ok(())
        }
        ItemCommand::TargetPath {
            plan_id,
            item_id,
            value,
        } => set_item_target(data_dir, &plan_id, &item_id, LaunchTarget::Path { value }),
        ItemCommand::TargetProgram {
            plan_id,
            item_id,
            value,
            args,
            working_dir,
        } => set_item_target(
            data_dir,
            &plan_id,
            &item_id,
            LaunchTarget::Program {
                value,
                args,
                working_dir,
            },
        ),
        ItemCommand::TargetUrl {
            plan_id,
            item_id,
            value,
        } => set_item_target(data_dir, &plan_id, &item_id, LaunchTarget::Url { value }),
        ItemCommand::TargetCommand {
            plan_id,
            item_id,
            value,
            shell,
            working_dir,
        } => set_item_target(
            data_dir,
            &plan_id,
            &item_id,
            LaunchTarget::Command {
                value,
                shell: shell.into(),
                working_dir,
            },
        ),
        ItemCommand::Move {
            plan_id,
            item_id,
            direction,
        } => {
            move_item(data_dir, &plan_id, &item_id, direction.into())?;
            println!("moved item {item_id} {direction:?}");
            Ok(())
        }
        ItemCommand::MoveToGroup {
            plan_id,
            item_id,
            group_id,
        } => {
            move_item_to_group(data_dir, &plan_id, &item_id, &group_id)?;
            println!("moved item {item_id} to group {group_id}");
            Ok(())
        }
        ItemCommand::MoveToRoot { plan_id, item_id } => {
            move_item_to_root(data_dir, &plan_id, &item_id)?;
            println!("moved item {item_id} to root");
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
    let item_id = item.id.clone();
    store_add_item(data_dir, plan_id, group_id, item)?;
    println!("added item {item_id} to {plan_id}");
    Ok(())
}

fn set_item_target(
    data_dir: &Path,
    plan_id: &str,
    item_id: &str,
    target: LaunchTarget,
) -> Result<()> {
    update_item(
        data_dir,
        plan_id,
        item_id,
        ItemUpdate {
            target: Some(target),
            ..Default::default()
        },
    )?;
    println!("changed target for item {item_id}");
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

fn add_schedule(data_dir: &Path, plan_id: &str, schedule: ScheduleRule) -> Result<()> {
    add_plan_schedule(data_dir, plan_id, schedule)?;
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
