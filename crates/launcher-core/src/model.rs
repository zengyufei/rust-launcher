use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const GLOBAL_SCHEMA_VERSION: u32 = 2;
pub const PLAN_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalConfig {
    pub version: u32,
    #[serde(default)]
    pub globals: GlobalDefaults,
    #[serde(default)]
    pub plans: Vec<PlanCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalDefaults {
    #[serde(default)]
    pub default_pre_delay_ms: u64,
    #[serde(default)]
    pub default_post_delay_ms: u64,
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u32,
}

impl Default for GlobalDefaults {
    fn default() -> Self {
        Self {
            default_pre_delay_ms: 0,
            default_post_delay_ms: 0,
            log_retention_days: default_log_retention_days(),
        }
    }
}

fn default_log_retention_days() -> u32 {
    14
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanCatalogEntry {
    pub id: String,
    pub file: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub launch: LaunchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchConfig {
    #[serde(default)]
    pub trigger: LaunchTrigger,
    #[serde(default)]
    pub schedules: Vec<ScheduleRule>,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            trigger: LaunchTrigger::Manual,
            schedules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchTrigger {
    #[default]
    Manual,
    AutoOnAppStart,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleRule {
    Daily { time: String },
    Weekly { weekday: Weekday, time: String },
    Once { at: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sequence: Vec<SequenceNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceNode {
    Group(Group),
    Item(LaunchItem),
}

impl SequenceNode {
    pub fn id(&self) -> &str {
        match self {
            SequenceNode::Group(group) => &group.id,
            SequenceNode::Item(item) => &item.id,
        }
    }
}

impl Serialize for SequenceNode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            SequenceNode::Group(group) => TaggedSequenceNode::Group {
                id: group.id.clone(),
                name: group.name.clone(),
                description: group.description.clone(),
                pre_delay_ms: group.pre_delay_ms,
                post_delay_ms: group.post_delay_ms,
                on_failure: group.on_failure,
                items: group.items.clone(),
            }
            .serialize(serializer),
            SequenceNode::Item(item) => TaggedSequenceNode::Item {
                id: item.id.clone(),
                name: item.name.clone(),
                description: item.description.clone(),
                target: item.target.clone(),
                pre_delay_ms: item.pre_delay_ms,
                post_delay_ms: item.post_delay_ms,
                on_failure: item.on_failure,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for SequenceNode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match TaggedSequenceNode::deserialize(deserializer)? {
            TaggedSequenceNode::Group {
                id,
                name,
                description,
                pre_delay_ms,
                post_delay_ms,
                on_failure,
                items,
            } => Ok(SequenceNode::Group(Group {
                id,
                name,
                description,
                pre_delay_ms,
                post_delay_ms,
                on_failure,
                items,
            })),
            TaggedSequenceNode::Item {
                id,
                name,
                description,
                target,
                pre_delay_ms,
                post_delay_ms,
                on_failure,
            } => Ok(SequenceNode::Item(LaunchItem {
                id,
                name,
                description,
                target,
                pre_delay_ms,
                post_delay_ms,
                on_failure,
            })),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum TaggedSequenceNode {
    Group {
        id: String,
        name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        pre_delay_ms: u64,
        #[serde(default)]
        post_delay_ms: u64,
        #[serde(default)]
        on_failure: FailurePolicy,
        #[serde(default)]
        items: Vec<LaunchItem>,
    },
    Item {
        id: String,
        name: String,
        #[serde(default)]
        description: String,
        target: LaunchTarget,
        #[serde(default)]
        pre_delay_ms: u64,
        #[serde(default)]
        post_delay_ms: u64,
        #[serde(default)]
        on_failure: FailurePolicy,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub pre_delay_ms: u64,
    #[serde(default)]
    pub post_delay_ms: u64,
    #[serde(default)]
    pub on_failure: FailurePolicy,
    #[serde(default)]
    pub items: Vec<LaunchItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub target: LaunchTarget,
    #[serde(default)]
    pub pre_delay_ms: u64,
    #[serde(default)]
    pub post_delay_ms: u64,
    #[serde(default)]
    pub on_failure: FailurePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LaunchTarget {
    Path {
        value: String,
    },
    Program {
        value: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        working_dir: Option<String>,
    },
    Url {
        value: String,
    },
    Command {
        value: String,
        #[serde(default)]
        shell: CommandShell,
        #[serde(default)]
        working_dir: Option<String>,
    },
}

impl LaunchTarget {
    pub fn summary(&self) -> String {
        match self {
            LaunchTarget::Path { value } => format!("path: {value}"),
            LaunchTarget::Program { value, args, .. } if args.is_empty() => {
                format!("program: {value}")
            }
            LaunchTarget::Program { value, args, .. } => {
                format!("program: {value} {}", args.join(" "))
            }
            LaunchTarget::Url { value } => format!("url: {value}"),
            LaunchTarget::Command { value, shell, .. } => {
                format!("command({shell:?}): {value}")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandShell {
    PowerShell,
    Cmd,
    Sh,
}

impl Default for CommandShell {
    fn default() -> Self {
        if cfg!(target_os = "windows") {
            Self::PowerShell
        } else {
            Self::Sh
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FailurePolicy {
    #[default]
    Continue,
    Stop,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_group_and_item_sequence_shape() {
        let plan: Plan = serde_json::from_str(
            r#"{
              "version": 2,
              "id": "work",
              "name": "Work",
              "sequence": [
                {
                  "kind": "group",
                  "id": "dev",
                  "name": "Dev",
                  "description": "Development tools",
                  "on_failure": "continue",
                  "items": [
                    {
                      "id": "folder",
                      "name": "Folder",
                      "description": "Open workspace",
                      "target": { "kind": "path", "value": "D:\\work" }
                    }
                  ]
                },
                {
                  "kind": "item",
                  "id": "notes",
                  "name": "Notes",
                  "description": "Open notes",
                  "target": { "kind": "path", "value": "D:\\notes\\todo.md" },
                  "on_failure": "stop"
                }
              ]
            }"#,
        )
        .unwrap();

        assert!(matches!(plan.sequence[0], SequenceNode::Group(_)));
        assert!(matches!(plan.sequence[1], SequenceNode::Item(_)));
    }

    #[test]
    fn parses_command_target_with_shell_and_working_dir() {
        let item: LaunchItem = serde_json::from_str(
            r#"{
              "id": "dev-server",
              "name": "Dev Server",
              "target": {
                "kind": "command",
                "value": "npm run dev",
                "shell": "power_shell",
                "working_dir": "D:\\cache\\runMain"
              }
            }"#,
        )
        .unwrap();

        assert!(matches!(
            item.target,
            LaunchTarget::Command {
                shell: CommandShell::PowerShell,
                ..
            }
        ));
    }
}
