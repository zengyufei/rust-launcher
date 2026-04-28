use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::{
    model::{
        CommandShell, GlobalConfig, Group, LaunchConfig, LaunchItem, LaunchTarget, LaunchTrigger,
        Plan, PlanCatalogEntry, ScheduleRule, SequenceNode, GLOBAL_SCHEMA_VERSION,
        PLAN_SCHEMA_VERSION,
    },
    LauncherError, Result,
};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub data_dir: PathBuf,
    pub global: GlobalConfig,
    pub plans: Vec<Plan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMoveDirection {
    Top,
    Up,
    Down,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeMoveDirection {
    Top,
    Up,
    Down,
    Bottom,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub pre_delay_ms: Option<u64>,
    pub post_delay_ms: Option<u64>,
    pub on_failure: Option<crate::model::FailurePolicy>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub pre_delay_ms: Option<u64>,
    pub post_delay_ms: Option<u64>,
    pub on_failure: Option<crate::model::FailurePolicy>,
    pub target: Option<LaunchTarget>,
}

pub fn default_data_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let dev_data = cwd.join("data");
    if dev_data.exists() || cwd.join("Cargo.toml").exists() {
        return dev_data;
    }

    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("RustLauncher"))
        .unwrap_or(dev_data)
}

pub fn load_workspace(data_dir: &Path) -> Result<Workspace> {
    let global = load_global(data_dir)?;
    let mut plans = Vec::new();
    for entry in &global.plans {
        let plan = load_plan(data_dir, &entry.file)?;
        if plan.id != entry.id {
            return Err(LauncherError::Validation(format!(
                "catalog entry {} points to plan file {} with id {}",
                entry.id, entry.file, plan.id
            )));
        }
        plans.push(plan);
    }
    Ok(Workspace {
        data_dir: data_dir.to_path_buf(),
        global,
        plans,
    })
}

pub fn load_global(data_dir: &Path) -> Result<GlobalConfig> {
    read_json(&data_dir.join("global.json"))
}

pub fn save_global(data_dir: &Path, global: &GlobalConfig) -> Result<()> {
    write_json(&data_dir.join("global.json"), global)
}

pub fn load_plan(data_dir: &Path, relative_file: &str) -> Result<Plan> {
    read_json(&data_dir.join(relative_file))
}

pub fn save_plan(data_dir: &Path, relative_file: &str, plan: &Plan) -> Result<()> {
    write_json(&data_dir.join(relative_file), plan)
}

pub fn create_plan(data_dir: &Path, id: &str, name: &str) -> Result<Plan> {
    create_plan_with_file(data_dir, id, name, &format!("plans/{id}.json"))
}

pub fn create_plan_with_file(
    data_dir: &Path,
    id: &str,
    name: &str,
    relative_file: &str,
) -> Result<Plan> {
    validate_id("plan id", id)?;
    if name.trim().is_empty() {
        return Err(LauncherError::Validation(
            "plan name must not be empty".to_string(),
        ));
    }
    validate_relative_file(relative_file)?;

    let mut global = if data_dir.join("global.json").exists() {
        load_global(data_dir)?
    } else {
        GlobalConfig {
            version: GLOBAL_SCHEMA_VERSION,
            globals: Default::default(),
            plans: Vec::new(),
        }
    };

    if global.plans.iter().any(|plan| plan.id == id) {
        return Err(LauncherError::Validation(format!(
            "plan id already exists: {id}"
        )));
    }
    if data_dir.join(relative_file).exists() {
        return Err(LauncherError::Validation(format!(
            "plan file already exists: {relative_file}"
        )));
    }

    let plan = Plan {
        version: PLAN_SCHEMA_VERSION,
        id: id.to_string(),
        name: name.to_string(),
        sequence: Vec::new(),
    };

    global.plans.push(PlanCatalogEntry {
        id: id.to_string(),
        file: relative_file.to_string(),
        enabled: true,
        launch: LaunchConfig::default(),
    });

    save_plan(data_dir, relative_file, &plan)?;
    save_global(data_dir, &global)?;
    Ok(plan)
}

pub fn export_plan(data_dir: &Path, id: &str, output_path: &Path) -> Result<()> {
    let (_, plan) = load_plan_by_id(data_dir, id)?;
    write_json(output_path, &plan)
}

pub fn import_plan(data_dir: &Path, source_path: &Path, overwrite: bool) -> Result<Plan> {
    let plan: Plan = read_json(source_path)?;
    validate_plan(&plan)?;

    let mut global = if data_dir.join("global.json").exists() {
        load_global(data_dir)?
    } else {
        GlobalConfig {
            version: GLOBAL_SCHEMA_VERSION,
            globals: Default::default(),
            plans: Vec::new(),
        }
    };

    if let Some(entry) = global.plans.iter().find(|entry| entry.id == plan.id) {
        if !overwrite {
            return Err(LauncherError::PlanImportConflict {
                plan_id: plan.id,
                plan_name: plan.name,
                target_file: entry.file.clone(),
                source_path: source_path.to_path_buf(),
            });
        }
        validate_pending_workspace(data_dir, &global, Some(&plan))?;
        save_plan(data_dir, &entry.file, &plan)?;
        return Ok(plan);
    }

    let relative_file = format!("plans/{}.json", plan.id);
    validate_relative_file(&relative_file)?;
    if data_dir.join(&relative_file).exists() {
        return Err(LauncherError::Validation(format!(
            "plan file already exists: {relative_file}"
        )));
    }

    global.plans.push(PlanCatalogEntry {
        id: plan.id.clone(),
        file: relative_file.clone(),
        enabled: true,
        launch: LaunchConfig::default(),
    });
    validate_pending_workspace(data_dir, &global, Some(&plan))?;
    save_plan(data_dir, &relative_file, &plan)?;
    save_global(data_dir, &global)?;
    Ok(plan)
}

pub fn rename_plan(data_dir: &Path, id: &str, name: &str) -> Result<Plan> {
    let (file, mut plan) = load_plan_by_id(data_dir, id)?;
    plan.name = name.to_string();
    save_changed_plan(data_dir, &file, &plan)?;
    Ok(plan)
}

pub fn delete_plan(data_dir: &Path, id: &str, delete_file: bool) -> Result<PlanCatalogEntry> {
    let mut global = load_global(data_dir)?;
    let index = find_plan_entry_index(&global, id)?;
    let entry = global.plans.remove(index);
    save_changed_global(data_dir, &global, None)?;

    if delete_file {
        let path = data_dir.join(&entry.file);
        if path.exists() {
            fs::remove_file(&path).map_err(|source| LauncherError::Io { path, source })?;
        }
    }

    Ok(entry)
}

pub fn set_plan_enabled(data_dir: &Path, id: &str, enabled: bool) -> Result<()> {
    let mut global = load_global(data_dir)?;
    find_plan_entry_mut(&mut global, id)?.enabled = enabled;
    save_changed_global(data_dir, &global, None)
}

pub fn set_plan_launch_trigger(data_dir: &Path, id: &str, trigger: LaunchTrigger) -> Result<()> {
    let mut global = load_global(data_dir)?;
    find_plan_entry_mut(&mut global, id)?.launch.trigger = trigger;
    save_changed_global(data_dir, &global, None)
}

pub fn add_plan_schedule(data_dir: &Path, id: &str, schedule: ScheduleRule) -> Result<()> {
    let mut global = load_global(data_dir)?;
    find_plan_entry_mut(&mut global, id)?
        .launch
        .schedules
        .push(schedule);
    save_changed_global(data_dir, &global, None)
}

pub fn update_plan_schedule(
    data_dir: &Path,
    id: &str,
    index: usize,
    schedule: ScheduleRule,
) -> Result<()> {
    let mut global = load_global(data_dir)?;
    let schedules = &mut find_plan_entry_mut(&mut global, id)?.launch.schedules;
    let Some(slot) = schedules.get_mut(index) else {
        return Err(validation_error(format!(
            "schedule index {} is out of range",
            index + 1
        )));
    };
    *slot = schedule;
    save_changed_global(data_dir, &global, None)
}

pub fn delete_plan_schedule(data_dir: &Path, id: &str, index: usize) -> Result<()> {
    let mut global = load_global(data_dir)?;
    let schedules = &mut find_plan_entry_mut(&mut global, id)?.launch.schedules;
    if index >= schedules.len() {
        return Err(validation_error(format!(
            "schedule index {} is out of range",
            index + 1
        )));
    }
    schedules.remove(index);
    save_changed_global(data_dir, &global, None)
}

pub fn move_plan(data_dir: &Path, id: &str, direction: PlanMoveDirection) -> Result<()> {
    let mut global = load_global(data_dir)?;
    move_plan_entry(&mut global, id, direction)?;
    save_changed_global(data_dir, &global, None)
}

pub fn add_group(data_dir: &Path, plan_id: &str, group: Group) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    ensure_unique_id(&plan, &group.id)?;
    plan.sequence.push(SequenceNode::Group(group));
    save_changed_plan(data_dir, &file, &plan)
}

pub fn update_group(
    data_dir: &Path,
    plan_id: &str,
    group_id: &str,
    update: GroupUpdate,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    let group = find_group_mut(&mut plan, group_id)?;
    if let Some(name) = update.name {
        group.name = name;
    }
    if let Some(description) = update.description {
        group.description = description;
    }
    if let Some(pre_delay_ms) = update.pre_delay_ms {
        group.pre_delay_ms = pre_delay_ms;
    }
    if let Some(post_delay_ms) = update.post_delay_ms {
        group.post_delay_ms = post_delay_ms;
    }
    if let Some(on_failure) = update.on_failure {
        group.on_failure = on_failure;
    }
    save_changed_plan(data_dir, &file, &plan)
}

pub fn delete_group(
    data_dir: &Path,
    plan_id: &str,
    group_id: &str,
    keep_items: bool,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    delete_group_from_plan(&mut plan, group_id, keep_items)?;
    save_changed_plan(data_dir, &file, &plan)
}

pub fn add_item(
    data_dir: &Path,
    plan_id: &str,
    group_id: Option<&str>,
    item: LaunchItem,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    ensure_unique_id(&plan, &item.id)?;
    if let Some(group_id) = group_id {
        find_group_mut(&mut plan, group_id)?.items.push(item);
    } else {
        plan.sequence.push(SequenceNode::Item(item));
    }
    save_changed_plan(data_dir, &file, &plan)
}

pub fn update_item(
    data_dir: &Path,
    plan_id: &str,
    item_id: &str,
    update: ItemUpdate,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    let item = find_item_mut(&mut plan, item_id)?;
    if let Some(name) = update.name {
        item.name = name;
    }
    if let Some(description) = update.description {
        item.description = description;
    }
    if let Some(pre_delay_ms) = update.pre_delay_ms {
        item.pre_delay_ms = pre_delay_ms;
    }
    if let Some(post_delay_ms) = update.post_delay_ms {
        item.post_delay_ms = post_delay_ms;
    }
    if let Some(on_failure) = update.on_failure {
        item.on_failure = on_failure;
    }
    if let Some(target) = update.target {
        item.target = target;
    }
    save_changed_plan(data_dir, &file, &plan)
}

pub fn delete_item(data_dir: &Path, plan_id: &str, item_id: &str) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    remove_item(&mut plan, item_id)?;
    save_changed_plan(data_dir, &file, &plan)
}

pub fn move_sequence_node(
    data_dir: &Path,
    plan_id: &str,
    node_id: &str,
    direction: NodeMoveDirection,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    move_sequence_node_in_plan(&mut plan, node_id, direction)?;
    save_changed_plan(data_dir, &file, &plan)
}

pub fn move_item(
    data_dir: &Path,
    plan_id: &str,
    item_id: &str,
    direction: NodeMoveDirection,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    move_item_in_plan(&mut plan, item_id, direction)?;
    save_changed_plan(data_dir, &file, &plan)
}

pub fn move_item_to_group(
    data_dir: &Path,
    plan_id: &str,
    item_id: &str,
    group_id: &str,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    let item = take_item(&mut plan, item_id)?;
    find_group_mut(&mut plan, group_id)?.items.push(item);
    save_changed_plan(data_dir, &file, &plan)
}

pub fn move_item_to_root(data_dir: &Path, plan_id: &str, item_id: &str) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    let item = take_item(&mut plan, item_id)?;
    plan.sequence.push(SequenceNode::Item(item));
    save_changed_plan(data_dir, &file, &plan)
}

pub fn combine_root_items(
    data_dir: &Path,
    plan_id: &str,
    item_ids: &[String],
    mut group: Group,
) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    combine_root_items_in_plan(&mut plan, item_ids, &mut group)?;
    save_changed_plan(data_dir, &file, &plan)
}

pub fn ungroup(data_dir: &Path, plan_id: &str, group_ids: &[String]) -> Result<()> {
    let (file, mut plan) = load_plan_by_id(data_dir, plan_id)?;
    ungroup_in_plan(&mut plan, group_ids)?;
    save_changed_plan(data_dir, &file, &plan)
}

pub fn validate_workspace(workspace: &Workspace) -> Result<()> {
    if workspace.global.version != GLOBAL_SCHEMA_VERSION {
        return Err(LauncherError::Validation(format!(
            "global.json version must be {}, got {}",
            GLOBAL_SCHEMA_VERSION, workspace.global.version
        )));
    }

    let mut catalog_ids = HashSet::new();
    for entry in &workspace.global.plans {
        validate_id("plan id", &entry.id)?;
        if !catalog_ids.insert(entry.id.clone()) {
            return Err(LauncherError::Validation(format!(
                "duplicate plan id in global.json: {}",
                entry.id
            )));
        }
        validate_relative_file(&entry.file)?;
        for schedule in &entry.launch.schedules {
            validate_schedule(schedule)?;
        }
    }

    for plan in &workspace.plans {
        validate_plan(plan)?;
        if !catalog_ids.contains(&plan.id) {
            return Err(LauncherError::Validation(format!(
                "plan {} is not listed in global.json",
                plan.id
            )));
        }
    }

    for entry in &workspace.global.plans {
        let plan = workspace.plans.iter().find(|plan| plan.id == entry.id);
        if plan.is_none() {
            return Err(LauncherError::Validation(format!(
                "global plan {} was not loaded",
                entry.id
            )));
        }
    }

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
    save_plan(data_dir, file, plan)
}

fn save_changed_global(
    data_dir: &Path,
    global: &GlobalConfig,
    changed_plan: Option<&Plan>,
) -> Result<()> {
    validate_pending_workspace(data_dir, global, changed_plan)?;
    save_global(data_dir, global)
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
    })
}

fn move_plan_entry(
    global: &mut GlobalConfig,
    id: &str,
    direction: PlanMoveDirection,
) -> Result<()> {
    let index = find_plan_entry_index(global, id)?;
    let new_index = match direction {
        PlanMoveDirection::Top => 0,
        PlanMoveDirection::Up => index.saturating_sub(1),
        PlanMoveDirection::Down => (index + 1).min(global.plans.len() - 1),
        PlanMoveDirection::Bottom => global.plans.len() - 1,
    };
    if index != new_index {
        let entry = global.plans.remove(index);
        global.plans.insert(new_index, entry);
    }
    Ok(())
}

fn move_sequence_node_in_plan(
    plan: &mut Plan,
    node_id: &str,
    direction: NodeMoveDirection,
) -> Result<()> {
    let index = plan
        .sequence
        .iter()
        .position(|node| node.id() == node_id)
        .ok_or_else(|| validation_error(format!("top-level node not found: {node_id}")))?;
    move_sequence_index(&mut plan.sequence, index, direction);
    Ok(())
}

fn move_item_in_plan(plan: &mut Plan, item_id: &str, direction: NodeMoveDirection) -> Result<()> {
    if let Some(index) = plan
        .sequence
        .iter()
        .position(|node| matches!(node, SequenceNode::Item(item) if item.id == item_id))
    {
        move_sequence_index(&mut plan.sequence, index, direction);
        return Ok(());
    }

    for node in &mut plan.sequence {
        if let SequenceNode::Group(group) = node {
            if let Some(index) = group.items.iter().position(|item| item.id == item_id) {
                move_item_index(&mut group.items, index, direction);
                return Ok(());
            }
        }
    }

    Err(LauncherError::ItemNotFound(item_id.to_string()))
}

fn move_sequence_index(nodes: &mut Vec<SequenceNode>, index: usize, direction: NodeMoveDirection) {
    let new_index = moved_index(index, nodes.len(), direction);
    if index != new_index {
        let node = nodes.remove(index);
        nodes.insert(new_index, node);
    }
}

fn move_item_index(items: &mut Vec<LaunchItem>, index: usize, direction: NodeMoveDirection) {
    let new_index = moved_index(index, items.len(), direction);
    if index != new_index {
        let item = items.remove(index);
        items.insert(new_index, item);
    }
}

fn moved_index(index: usize, len: usize, direction: NodeMoveDirection) -> usize {
    match direction {
        NodeMoveDirection::Top => 0,
        NodeMoveDirection::Up => index.saturating_sub(1),
        NodeMoveDirection::Down => (index + 1).min(len - 1),
        NodeMoveDirection::Bottom => len - 1,
    }
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

fn find_item_mut<'a>(plan: &'a mut Plan, item_id: &str) -> Result<&'a mut LaunchItem> {
    for node in &mut plan.sequence {
        match node {
            SequenceNode::Item(item) if item.id == item_id => return Ok(item),
            SequenceNode::Group(group) => {
                if let Some(item) = group.items.iter_mut().find(|item| item.id == item_id) {
                    return Ok(item);
                }
            }
            _ => {}
        }
    }

    Err(LauncherError::ItemNotFound(item_id.to_string()))
}

fn ensure_unique_id(plan: &Plan, id: &str) -> Result<()> {
    validate_id("id", id)?;
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

    Err(LauncherError::ItemNotFound(item_id.to_string()))
}

fn take_item(plan: &mut Plan, item_id: &str) -> Result<LaunchItem> {
    if let Some(index) = plan
        .sequence
        .iter()
        .position(|node| matches!(node, SequenceNode::Item(item) if item.id == item_id))
    {
        let SequenceNode::Item(item) = plan.sequence.remove(index) else {
            unreachable!();
        };
        return Ok(item);
    }

    for node in &mut plan.sequence {
        if let SequenceNode::Group(group) = node {
            if let Some(index) = group.items.iter().position(|item| item.id == item_id) {
                return Ok(group.items.remove(index));
            }
        }
    }

    Err(LauncherError::ItemNotFound(item_id.to_string()))
}

fn delete_group_from_plan(plan: &mut Plan, group_id: &str, keep_items: bool) -> Result<()> {
    let index = find_group_index(plan, group_id)?;
    let SequenceNode::Group(group) = plan.sequence.remove(index) else {
        unreachable!();
    };
    if keep_items {
        for (offset, item) in group.items.into_iter().enumerate() {
            plan.sequence
                .insert(index + offset, SequenceNode::Item(item));
        }
    }
    Ok(())
}

fn combine_root_items_in_plan(
    plan: &mut Plan,
    item_ids: &[String],
    group: &mut Group,
) -> Result<()> {
    if item_ids.len() < 2 {
        return Err(validation_error("combine requires at least two items"));
    }
    ensure_unique_id(plan, &group.id)?;
    if !group.items.is_empty() {
        return Err(validation_error(
            "new combined group must not already contain items",
        ));
    }

    let selected_ids = item_ids.iter().collect::<HashSet<_>>();
    let mut selected_indexes = Vec::new();
    for (index, node) in plan.sequence.iter().enumerate() {
        if let SequenceNode::Item(item) = node {
            if selected_ids.contains(&item.id) {
                selected_indexes.push(index);
            }
        }
    }
    if selected_indexes.len() != item_ids.len() {
        return Err(validation_error(
            "combine only supports existing root-level items",
        ));
    }

    let insert_index = selected_indexes[0];
    for index in selected_indexes.into_iter().rev() {
        let SequenceNode::Item(item) = plan.sequence.remove(index) else {
            unreachable!();
        };
        group.items.insert(0, item);
    }
    plan.sequence
        .insert(insert_index, SequenceNode::Group(group.clone()));
    Ok(())
}

fn ungroup_in_plan(plan: &mut Plan, group_ids: &[String]) -> Result<()> {
    if group_ids.is_empty() {
        return Err(validation_error("ungroup requires at least one group"));
    }
    let selected_ids = group_ids.iter().collect::<HashSet<_>>();
    let found = plan
        .sequence
        .iter()
        .filter(
            |node| matches!(node, SequenceNode::Group(group) if selected_ids.contains(&group.id)),
        )
        .count();
    if found != group_ids.len() {
        return Err(validation_error(
            "ungroup only supports existing root-level groups",
        ));
    }

    let mut index = 0;
    while index < plan.sequence.len() {
        let should_ungroup = matches!(
            &plan.sequence[index],
            SequenceNode::Group(group) if selected_ids.contains(&group.id)
        );
        if should_ungroup {
            let SequenceNode::Group(group) = plan.sequence.remove(index) else {
                unreachable!();
            };
            for item in group.items.into_iter().rev() {
                plan.sequence.insert(index, SequenceNode::Item(item));
            }
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn validation_error(message: impl Into<String>) -> LauncherError {
    LauncherError::Validation(message.into())
}

fn find_plan_entry<'a>(global: &'a GlobalConfig, plan_id: &str) -> Result<&'a PlanCatalogEntry> {
    global
        .plans
        .iter()
        .find(|entry| entry.id == plan_id)
        .ok_or_else(|| LauncherError::PlanNotFound(plan_id.to_string()))
}

fn find_plan_entry_mut<'a>(
    global: &'a mut GlobalConfig,
    plan_id: &str,
) -> Result<&'a mut PlanCatalogEntry> {
    global
        .plans
        .iter_mut()
        .find(|entry| entry.id == plan_id)
        .ok_or_else(|| LauncherError::PlanNotFound(plan_id.to_string()))
}

fn find_plan_entry_index(global: &GlobalConfig, plan_id: &str) -> Result<usize> {
    global
        .plans
        .iter()
        .position(|entry| entry.id == plan_id)
        .ok_or_else(|| LauncherError::PlanNotFound(plan_id.to_string()))
}

fn validate_plan(plan: &Plan) -> Result<()> {
    if plan.version != PLAN_SCHEMA_VERSION {
        return Err(LauncherError::Validation(format!(
            "plan {} version must be {}, got {}",
            plan.id, PLAN_SCHEMA_VERSION, plan.version
        )));
    }
    validate_id("plan id", &plan.id)?;
    if plan.name.trim().is_empty() {
        return Err(LauncherError::Validation(format!(
            "plan {} has an empty name",
            plan.id
        )));
    }

    let mut ids = HashSet::new();
    for node in &plan.sequence {
        match node {
            SequenceNode::Group(group) => validate_group(group, &mut ids)?,
            SequenceNode::Item(item) => validate_item(item, &mut ids)?,
        }
    }
    Ok(())
}

fn validate_group(group: &Group, ids: &mut HashSet<String>) -> Result<()> {
    validate_id("group id", &group.id)?;
    if !ids.insert(group.id.clone()) {
        return Err(LauncherError::Validation(format!(
            "duplicate id in plan: {}",
            group.id
        )));
    }
    if group.name.trim().is_empty() {
        return Err(LauncherError::Validation(format!(
            "group {} has an empty name",
            group.id
        )));
    }
    for item in &group.items {
        validate_item(item, ids)?;
    }
    Ok(())
}

fn validate_item(item: &LaunchItem, ids: &mut HashSet<String>) -> Result<()> {
    validate_id("item id", &item.id)?;
    if !ids.insert(item.id.clone()) {
        return Err(LauncherError::Validation(format!(
            "duplicate id in plan: {}",
            item.id
        )));
    }
    if item.name.trim().is_empty() {
        return Err(LauncherError::Validation(format!(
            "item {} has an empty name",
            item.id
        )));
    }
    match &item.target {
        LaunchTarget::Path { value } | LaunchTarget::Url { value } => {
            validate_non_empty_target(&item.id, value)
        }
        LaunchTarget::Program { value, .. } => validate_non_empty_target(&item.id, value),
        LaunchTarget::Command { value, shell, .. } => {
            validate_command_shell(&item.id, *shell)?;
            validate_non_empty_target(&item.id, value)
        }
    }
}

fn validate_command_shell(item_id: &str, shell: CommandShell) -> Result<()> {
    if !cfg!(target_os = "windows") && matches!(shell, CommandShell::PowerShell | CommandShell::Cmd)
    {
        return Err(LauncherError::Validation(format!(
            "item {item_id} uses a Windows-only shell on this platform"
        )));
    }
    Ok(())
}

fn validate_non_empty_target(item_id: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(LauncherError::Validation(format!(
            "item {item_id} has an empty target"
        )))
    } else {
        Ok(())
    }
}

fn validate_schedule(schedule: &ScheduleRule) -> Result<()> {
    match schedule {
        ScheduleRule::Daily { time } | ScheduleRule::Weekly { time, .. } => {
            parse_hhmm(time).map(|_| ())
        }
        ScheduleRule::Once { at } => parse_once_datetime(at).map(|_| ()),
    }
}

pub fn parse_once_datetime(value: &str) -> Result<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M"))
        .map_err(|_| {
            LauncherError::Validation(format!(
                "invalid once schedule datetime, expected YYYY-MM-DDTHH:MM:SS or YYYY-MM-DD HH:MM: {value}"
            ))
        })
}

pub fn parse_hhmm(value: &str) -> Result<(u32, u32)> {
    let (hour, minute) = value.split_once(':').ok_or_else(|| {
        LauncherError::Validation(format!("invalid time, expected HH:MM: {value}"))
    })?;
    let hour = hour
        .parse::<u32>()
        .map_err(|_| LauncherError::Validation(format!("invalid hour in time: {value}")))?;
    let minute = minute
        .parse::<u32>()
        .map_err(|_| LauncherError::Validation(format!("invalid minute in time: {value}")))?;
    if hour > 23 || minute > 59 {
        return Err(LauncherError::Validation(format!(
            "time is out of range: {value}"
        )));
    }
    Ok((hour, minute))
}

fn validate_id(label: &str, id: &str) -> Result<()> {
    if id.trim().is_empty() {
        return Err(LauncherError::Validation(format!("{label} is empty")));
    }
    if !id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(LauncherError::Validation(format!(
            "{label} contains unsupported characters: {id}"
        )));
    }
    Ok(())
}

fn validate_relative_file(file: &str) -> Result<()> {
    let path = Path::new(file);
    if file.trim().is_empty() {
        return Err(LauncherError::Validation(
            "plan file path must not be empty".to_string(),
        ));
    }
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(LauncherError::Validation(format!(
            "plan file path must stay inside data dir: {file}"
        )));
    }
    Ok(())
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let text = fs::read_to_string(path).map_err(|source| LauncherError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|source| {
        LauncherError::Json {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn write_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: serde::Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| LauncherError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let text = serde_json::to_string_pretty(value).map_err(|source| LauncherError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, format!("{text}\n")).map_err(|source| LauncherError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_workspace(data_dir: &Path) {
        fs::create_dir_all(data_dir.join("plans")).unwrap();
        fs::write(
            data_dir.join("global.json"),
            r#"{
              "version": 2,
              "plans": [
                { "id": "work", "file": "plans/work.json", "enabled": true },
                { "id": "music", "file": "plans/music.json", "enabled": true },
                { "id": "notes", "file": "plans/notes.json", "enabled": true }
              ]
            }"#,
        )
        .unwrap();
        for id in ["work", "music", "notes"] {
            fs::write(
                data_dir.join(format!("plans/{id}.json")),
                format!(
                    r#"{{
                      "version": 2,
                      "id": "{id}",
                      "name": "{id}",
                      "sequence": []
                    }}"#
                ),
            )
            .unwrap();
        }
    }

    fn write_sequence_workspace(data_dir: &Path) {
        fs::create_dir_all(data_dir.join("plans")).unwrap();
        fs::write(
            data_dir.join("global.json"),
            r#"{
              "version": 2,
              "plans": [
                { "id": "work", "file": "plans/work.json", "enabled": true }
              ]
            }"#,
        )
        .unwrap();
        fs::write(
            data_dir.join("plans/work.json"),
            r#"{
              "version": 2,
              "id": "work",
              "name": "Work",
              "sequence": [
                {
                  "kind": "item",
                  "id": "notes",
                  "name": "Notes",
                  "target": { "kind": "path", "value": "D:\\notes.md" }
                },
                {
                  "kind": "group",
                  "id": "dev",
                  "name": "Dev",
                  "items": [
                    {
                      "id": "repo",
                      "name": "Repo",
                      "target": { "kind": "path", "value": "D:\\repo" }
                    }
                  ]
                },
                {
                  "kind": "item",
                  "id": "music",
                  "name": "Music",
                  "target": { "kind": "url", "value": "https://example.com" }
                }
              ]
            }"#,
        )
        .unwrap();
    }

    #[test]
    fn parses_valid_workspace() {
        let data_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(data_dir.path().join("plans")).unwrap();
        fs::write(
            data_dir.path().join("global.json"),
            r#"{
              "version": 2,
              "plans": [
                {
                  "id": "work",
                  "file": "plans/work.json",
                  "enabled": true,
                  "launch": {
                    "trigger": "auto_on_app_start",
                    "schedules": [{ "kind": "daily", "time": "09:00" }]
                  }
                }
              ]
            }"#,
        )
        .unwrap();
        fs::write(
            data_dir.path().join("plans/work.json"),
            r#"{
              "version": 2,
              "id": "work",
              "name": "Work",
              "sequence": [
                {
                  "kind": "item",
                  "id": "notes",
                  "name": "Notes",
                  "target": { "kind": "path", "value": "D:\\notes\\todo.md" },
                  "on_failure": "continue"
                }
              ]
            }"#,
        )
        .unwrap();

        let workspace = load_workspace(data_dir.path()).unwrap();
        validate_workspace(&workspace).unwrap();
        assert_eq!(workspace.plans[0].id, "work");
    }

    #[test]
    fn rejects_invalid_time() {
        let error = validate_schedule(&ScheduleRule::Daily {
            time: "25:00".to_string(),
        })
        .unwrap_err();

        assert!(error.to_string().contains("out of range"));
    }

    #[test]
    fn creates_plan_and_catalog_entry() {
        let data_dir = tempfile::tempdir().unwrap();

        let plan = create_plan(data_dir.path(), "work", "Work").unwrap();
        let workspace = load_workspace(data_dir.path()).unwrap();

        assert_eq!(plan.id, "work");
        assert_eq!(workspace.global.plans[0].file, "plans/work.json");
        assert!(data_dir.path().join("plans/work.json").exists());
        validate_workspace(&workspace).unwrap();
    }

    #[test]
    fn rejects_duplicate_plan_id_without_writing() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        let error = create_plan(data_dir.path(), "work", "Duplicate").unwrap_err();
        let workspace = load_workspace(data_dir.path()).unwrap();

        assert!(error.to_string().contains("already exists"));
        assert_eq!(workspace.global.plans.len(), 3);
    }

    #[test]
    fn rejects_invalid_plan_file_without_writing() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        let error =
            create_plan_with_file(data_dir.path(), "bad", "Bad", "../bad.json").unwrap_err();
        let workspace = load_workspace(data_dir.path()).unwrap();

        assert!(error.to_string().contains("must stay inside data dir"));
        assert_eq!(workspace.global.plans.len(), 3);
    }

    #[test]
    fn rejects_existing_plan_file_without_overwriting() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        fs::write(data_dir.path().join("plans/custom.json"), "keep me").unwrap();

        let error = create_plan_with_file(data_dir.path(), "custom", "Custom", "plans/custom.json")
            .unwrap_err();
        let text = fs::read_to_string(data_dir.path().join("plans/custom.json")).unwrap();
        let workspace = load_workspace(data_dir.path()).unwrap();

        assert!(error.to_string().contains("already exists"));
        assert_eq!(text, "keep me");
        assert_eq!(workspace.global.plans.len(), 3);
    }

    #[test]
    fn renames_plan_without_changing_id_or_file() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        let plan = rename_plan(data_dir.path(), "work", "Work Renamed").unwrap();
        let workspace = load_workspace(data_dir.path()).unwrap();

        assert_eq!(plan.id, "work");
        assert_eq!(plan.name, "Work Renamed");
        assert_eq!(workspace.global.plans[0].file, "plans/work.json");
        assert_eq!(workspace.plans[0].name, "Work Renamed");
    }

    #[test]
    fn rejects_empty_rename_without_overwriting_existing_plan() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        let error = rename_plan(data_dir.path(), "work", "").unwrap_err();
        let plan = load_plan(data_dir.path(), "plans/work.json").unwrap();

        assert!(error.to_string().contains("empty name"));
        assert_eq!(plan.name, "work");
    }

    #[test]
    fn deletes_plan_entry_and_file() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        let entry = delete_plan(data_dir.path(), "music", true).unwrap();
        let workspace = load_workspace(data_dir.path()).unwrap();

        assert_eq!(entry.id, "music");
        assert_eq!(workspace.global.plans.len(), 2);
        assert!(!data_dir.path().join("plans/music.json").exists());
        assert!(workspace.global.plans.iter().all(|plan| plan.id != "music"));
    }

    #[test]
    fn exports_plan_json() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        let output = data_dir.path().join("exported-work.json");

        export_plan(data_dir.path(), "work", &output).unwrap();

        let exported: Plan = read_json(&output).unwrap();
        assert_eq!(exported.id, "work");
        assert_eq!(exported.name, "work");
    }

    #[test]
    fn imports_new_plan_and_catalog_entry() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        let source = data_dir.path().join("imported.json");
        fs::write(
            &source,
            r#"{
              "version": 2,
              "id": "imported",
              "name": "Imported",
              "sequence": [
                {
                  "kind": "item",
                  "id": "site",
                  "name": "Site",
                  "target": { "kind": "url", "value": "https://example.com" }
                }
              ]
            }"#,
        )
        .unwrap();

        let plan = import_plan(data_dir.path(), &source, false).unwrap();
        let workspace = load_workspace(data_dir.path()).unwrap();

        assert_eq!(plan.id, "imported");
        assert!(data_dir.path().join("plans/imported.json").exists());
        assert!(workspace.global.plans.iter().any(|entry| {
            entry.id == "imported"
                && entry.file == "plans/imported.json"
                && entry.enabled
                && entry.launch == LaunchConfig::default()
        }));
        validate_workspace(&workspace).unwrap();
    }

    #[test]
    fn imports_plan_json_with_utf8_bom() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        let source = data_dir.path().join("bom.json");
        fs::write(
            &source,
            "\u{feff}{\"version\":2,\"id\":\"bom\",\"name\":\"BOM\",\"sequence\":[]}",
        )
        .unwrap();

        let plan = import_plan(data_dir.path(), &source, false).unwrap();

        assert_eq!(plan.id, "bom");
        assert!(data_dir.path().join("plans/bom.json").exists());
    }

    #[test]
    fn import_conflict_without_overwrite_preserves_existing_plan() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        let source = data_dir.path().join("work-import.json");
        fs::write(
            &source,
            r#"{
              "version": 2,
              "id": "work",
              "name": "Imported Work",
              "sequence": []
            }"#,
        )
        .unwrap();
        let before = fs::read_to_string(data_dir.path().join("plans/work.json")).unwrap();

        let error = import_plan(data_dir.path(), &source, false).unwrap_err();
        let after = fs::read_to_string(data_dir.path().join("plans/work.json")).unwrap();

        assert!(matches!(
            error,
            LauncherError::PlanImportConflict { plan_id, .. } if plan_id == "work"
        ));
        assert_eq!(before, after);
    }

    #[test]
    fn import_overwrite_preserves_catalog_file_and_launch() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        set_plan_launch_trigger(data_dir.path(), "work", LaunchTrigger::AutoOnAppStart).unwrap();
        add_plan_schedule(
            data_dir.path(),
            "work",
            ScheduleRule::Daily {
                time: "09:00".to_string(),
            },
        )
        .unwrap();
        let source = data_dir.path().join("work-import.json");
        fs::write(
            &source,
            r#"{
              "version": 2,
              "id": "work",
              "name": "Imported Work",
              "sequence": []
            }"#,
        )
        .unwrap();

        import_plan(data_dir.path(), &source, true).unwrap();
        let workspace = load_workspace(data_dir.path()).unwrap();
        let entry = workspace
            .global
            .plans
            .iter()
            .find(|entry| entry.id == "work")
            .unwrap();
        let plan = workspace
            .plans
            .iter()
            .find(|plan| plan.id == "work")
            .unwrap();

        assert_eq!(entry.file, "plans/work.json");
        assert_eq!(entry.launch.trigger, LaunchTrigger::AutoOnAppStart);
        assert_eq!(entry.launch.schedules.len(), 1);
        assert_eq!(plan.name, "Imported Work");
        validate_workspace(&workspace).unwrap();
    }

    #[test]
    fn rejects_bad_import_without_writing() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        let source = data_dir.path().join("bad-import.json");
        fs::write(
            &source,
            r#"{
              "version": 2,
              "id": "bad",
              "name": "Bad",
              "sequence": [
                {
                  "kind": "item",
                  "id": "dup",
                  "name": "One",
                  "target": { "kind": "url", "value": "https://example.com" }
                },
                {
                  "kind": "item",
                  "id": "dup",
                  "name": "Two",
                  "target": { "kind": "url", "value": "https://example.com" }
                }
              ]
            }"#,
        )
        .unwrap();
        let before = fs::read_to_string(data_dir.path().join("global.json")).unwrap();

        let error = import_plan(data_dir.path(), &source, false).unwrap_err();
        let after = fs::read_to_string(data_dir.path().join("global.json")).unwrap();

        assert!(error.to_string().contains("duplicate id"));
        assert_eq!(before, after);
        assert!(!data_dir.path().join("plans/bad.json").exists());
    }

    #[test]
    fn moving_plan_respects_order_and_boundaries() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        move_plan(data_dir.path(), "music", PlanMoveDirection::Top).unwrap();
        move_plan(data_dir.path(), "music", PlanMoveDirection::Up).unwrap();
        move_plan(data_dir.path(), "work", PlanMoveDirection::Bottom).unwrap();
        move_plan(data_dir.path(), "work", PlanMoveDirection::Down).unwrap();
        let global = load_global(data_dir.path()).unwrap();
        let ids = global
            .plans
            .iter()
            .map(|plan| plan.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["music", "notes", "work"]);
    }

    #[test]
    fn toggles_plan_enabled() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        set_plan_enabled(data_dir.path(), "work", false).unwrap();
        assert!(!load_global(data_dir.path()).unwrap().plans[0].enabled);
        set_plan_enabled(data_dir.path(), "work", true).unwrap();
        assert!(load_global(data_dir.path()).unwrap().plans[0].enabled);
    }

    #[test]
    fn updates_plan_launch_trigger_without_touching_schedule() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        set_plan_launch_trigger(data_dir.path(), "work", LaunchTrigger::AutoOnAppStart).unwrap();
        let global = load_global(data_dir.path()).unwrap();
        assert_eq!(
            global.plans[0].launch.trigger,
            LaunchTrigger::AutoOnAppStart
        );
        assert!(global.plans[0].launch.schedules.is_empty());

        set_plan_launch_trigger(data_dir.path(), "work", LaunchTrigger::Manual).unwrap();
        assert_eq!(
            load_global(data_dir.path()).unwrap().plans[0]
                .launch
                .trigger,
            LaunchTrigger::Manual
        );
    }

    #[test]
    fn adds_updates_and_deletes_plan_schedules() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        add_plan_schedule(
            data_dir.path(),
            "work",
            ScheduleRule::Daily {
                time: "09:00".to_string(),
            },
        )
        .unwrap();
        add_plan_schedule(
            data_dir.path(),
            "work",
            ScheduleRule::Weekly {
                weekday: crate::model::Weekday::Monday,
                time: "09:30".to_string(),
            },
        )
        .unwrap();
        add_plan_schedule(
            data_dir.path(),
            "work",
            ScheduleRule::Once {
                at: "2026-05-01T10:00:00".to_string(),
            },
        )
        .unwrap();
        update_plan_schedule(
            data_dir.path(),
            "work",
            1,
            ScheduleRule::Weekly {
                weekday: crate::model::Weekday::Friday,
                time: "18:00".to_string(),
            },
        )
        .unwrap();
        delete_plan_schedule(data_dir.path(), "work", 0).unwrap();

        let global = load_global(data_dir.path()).unwrap();
        let schedules = &global.plans[0].launch.schedules;
        assert_eq!(schedules.len(), 2);
        assert!(matches!(
            schedules[0],
            ScheduleRule::Weekly {
                weekday: crate::model::Weekday::Friday,
                ..
            }
        ));
        assert!(matches!(schedules[1], ScheduleRule::Once { .. }));
        validate_workspace(&load_workspace(data_dir.path()).unwrap()).unwrap();
    }

    #[test]
    fn rejects_bad_schedule_mutations_without_overwriting_global() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());
        add_plan_schedule(
            data_dir.path(),
            "work",
            ScheduleRule::Daily {
                time: "09:00".to_string(),
            },
        )
        .unwrap();
        let before = fs::read_to_string(data_dir.path().join("global.json")).unwrap();

        let bad_time = add_plan_schedule(
            data_dir.path(),
            "work",
            ScheduleRule::Daily {
                time: "25:00".to_string(),
            },
        )
        .unwrap_err();
        let bad_index = update_plan_schedule(
            data_dir.path(),
            "work",
            4,
            ScheduleRule::Daily {
                time: "10:00".to_string(),
            },
        )
        .unwrap_err();
        let missing = delete_plan_schedule(data_dir.path(), "missing", 0).unwrap_err();
        let after = fs::read_to_string(data_dir.path().join("global.json")).unwrap();

        assert!(bad_time.to_string().contains("out of range"));
        assert!(bad_index.to_string().contains("out of range"));
        assert!(missing.to_string().contains("plan not found"));
        assert_eq!(before, after);
    }

    #[test]
    fn missing_plan_mutation_returns_error() {
        let data_dir = tempfile::tempdir().unwrap();
        write_workspace(data_dir.path());

        let error = delete_plan(data_dir.path(), "missing", true).unwrap_err();

        assert!(error.to_string().contains("plan not found"));
    }

    #[test]
    fn edits_group_and_can_expand_its_items_on_delete() {
        let data_dir = tempfile::tempdir().unwrap();
        write_sequence_workspace(data_dir.path());

        update_group(
            data_dir.path(),
            "work",
            "dev",
            GroupUpdate {
                name: Some("Development".to_string()),
                description: Some("Tools".to_string()),
                pre_delay_ms: Some(10),
                post_delay_ms: Some(20),
                on_failure: Some(crate::model::FailurePolicy::Stop),
            },
        )
        .unwrap();
        delete_group(data_dir.path(), "work", "dev", true).unwrap();
        let plan = load_plan(data_dir.path(), "plans/work.json").unwrap();
        let ids = plan
            .sequence
            .iter()
            .map(SequenceNode::id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["notes", "repo", "music"]);
        validate_workspace(&load_workspace(data_dir.path()).unwrap()).unwrap();
    }

    #[test]
    fn adds_and_updates_all_item_target_shapes() {
        let data_dir = tempfile::tempdir().unwrap();
        write_sequence_workspace(data_dir.path());

        add_item(
            data_dir.path(),
            "work",
            None,
            LaunchItem {
                id: "script".to_string(),
                name: "Script".to_string(),
                description: "Run script".to_string(),
                target: LaunchTarget::Command {
                    value: "cargo test".to_string(),
                    shell: CommandShell::PowerShell,
                    working_dir: Some("D:\\cache\\runMain".to_string()),
                },
                pre_delay_ms: 1,
                post_delay_ms: 2,
                on_failure: crate::model::FailurePolicy::Continue,
            },
        )
        .unwrap();
        update_item(
            data_dir.path(),
            "work",
            "script",
            ItemUpdate {
                name: Some("Program".to_string()),
                target: Some(LaunchTarget::Program {
                    value: "notepad.exe".to_string(),
                    args: vec!["readme.txt".to_string()],
                    working_dir: None,
                }),
                ..Default::default()
            },
        )
        .unwrap();

        let plan = load_plan(data_dir.path(), "plans/work.json").unwrap();
        let SequenceNode::Item(item) = plan.sequence.last().unwrap() else {
            panic!("last node should be item");
        };
        assert_eq!(item.name, "Program");
        assert!(matches!(item.target, LaunchTarget::Program { .. }));
        validate_workspace(&load_workspace(data_dir.path()).unwrap()).unwrap();
    }

    #[test]
    fn rejects_bad_item_without_overwriting_plan() {
        let data_dir = tempfile::tempdir().unwrap();
        write_sequence_workspace(data_dir.path());
        let before = fs::read_to_string(data_dir.path().join("plans/work.json")).unwrap();

        let error = add_item(
            data_dir.path(),
            "work",
            None,
            LaunchItem {
                id: "notes".to_string(),
                name: "Duplicate".to_string(),
                description: String::new(),
                target: LaunchTarget::Path {
                    value: "D:\\x".to_string(),
                },
                pre_delay_ms: 0,
                post_delay_ms: 0,
                on_failure: crate::model::FailurePolicy::Continue,
            },
        )
        .unwrap_err();
        let after = fs::read_to_string(data_dir.path().join("plans/work.json")).unwrap();

        assert!(error.to_string().contains("duplicate id"));
        assert_eq!(before, after);
    }

    #[test]
    fn moves_root_nodes_and_group_items_with_boundaries() {
        let data_dir = tempfile::tempdir().unwrap();
        write_sequence_workspace(data_dir.path());

        move_sequence_node(data_dir.path(), "work", "music", NodeMoveDirection::Top).unwrap();
        move_sequence_node(data_dir.path(), "work", "music", NodeMoveDirection::Up).unwrap();
        move_item(data_dir.path(), "work", "repo", NodeMoveDirection::Bottom).unwrap();
        let plan = load_plan(data_dir.path(), "plans/work.json").unwrap();
        let ids = plan
            .sequence
            .iter()
            .map(SequenceNode::id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["music", "notes", "dev"]);
    }

    #[test]
    fn combines_root_items_and_ungroups_in_plan_order() {
        let data_dir = tempfile::tempdir().unwrap();
        write_sequence_workspace(data_dir.path());

        combine_root_items(
            data_dir.path(),
            "work",
            &["music".to_string(), "notes".to_string()],
            Group {
                id: "combo".to_string(),
                name: "Combo".to_string(),
                description: String::new(),
                pre_delay_ms: 0,
                post_delay_ms: 0,
                on_failure: crate::model::FailurePolicy::Continue,
                items: Vec::new(),
            },
        )
        .unwrap();
        let plan = load_plan(data_dir.path(), "plans/work.json").unwrap();
        let SequenceNode::Group(group) = &plan.sequence[0] else {
            panic!("first node should be combined group");
        };
        assert_eq!(group.id, "combo");
        assert_eq!(
            group
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["notes", "music"]
        );

        ungroup(data_dir.path(), "work", &["combo".to_string()]).unwrap();
        let plan = load_plan(data_dir.path(), "plans/work.json").unwrap();
        let ids = plan
            .sequence
            .iter()
            .map(SequenceNode::id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["notes", "music", "dev"]);
    }
}
