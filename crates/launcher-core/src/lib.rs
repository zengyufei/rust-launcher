mod error;
pub mod executor;
pub mod model;
pub mod platform;
pub mod scheduler;
pub mod store;

pub use error::{LauncherError, Result};
pub use executor::{
    execute_plan, execute_plan_with_adapter, execute_single_item, ExecuteOptions, ExecutionReport,
    ItemExecution,
};
pub use model::{
    CommandShell, FailurePolicy, GlobalConfig, Group, LaunchConfig, LaunchItem, LaunchTarget,
    LaunchTrigger, Plan, PlanCatalogEntry, ScheduleRule, SequenceNode, Weekday,
    GLOBAL_SCHEMA_VERSION, PLAN_SCHEMA_VERSION,
};
pub use scheduler::{DuePlan, Scheduler};
pub use store::{
    create_plan, create_plan_with_file, default_data_dir, load_global, load_plan, load_workspace,
    save_global, save_plan, validate_workspace, Workspace,
};
