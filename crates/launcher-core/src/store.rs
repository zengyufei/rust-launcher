use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use crate::{
    model::{
        CommandShell, GlobalConfig, Group, LaunchConfig, LaunchItem, LaunchTarget, Plan,
        PlanCatalogEntry, ScheduleRule, SequenceNode, GLOBAL_SCHEMA_VERSION, PLAN_SCHEMA_VERSION,
    },
    LauncherError, Result,
};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub data_dir: PathBuf,
    pub global: GlobalConfig,
    pub plans: Vec<Plan>,
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
    serde_json::from_str(&text).map_err(|source| LauncherError::Json {
        path: path.to_path_buf(),
        source,
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
}
