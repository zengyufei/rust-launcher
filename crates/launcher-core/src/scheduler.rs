use std::collections::HashSet;

use chrono::{Datelike, Local, NaiveDateTime, Timelike};

use crate::{
    model::{GlobalConfig, LaunchTrigger, ScheduleRule, Weekday},
    store::{parse_hhmm, parse_once_datetime},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuePlan {
    pub plan_id: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Scheduler {
    fired_keys: HashSet<String>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn auto_on_app_start(global: &GlobalConfig) -> Vec<DuePlan> {
        global
            .plans
            .iter()
            .filter(|entry| entry.enabled && entry.launch.trigger == LaunchTrigger::AutoOnAppStart)
            .map(|entry| DuePlan {
                plan_id: entry.id.clone(),
                reason: "auto_on_app_start".to_string(),
            })
            .collect()
    }

    pub fn due_now(&mut self, global: &GlobalConfig) -> Vec<DuePlan> {
        let now = Local::now().naive_local();
        let mut due = Vec::new();

        for entry in &global.plans {
            if !entry.enabled || entry.launch.trigger != LaunchTrigger::AutoOnAppStart {
                continue;
            }

            for (index, schedule) in entry.launch.schedules.iter().enumerate() {
                if is_due(schedule, now) {
                    let key = fired_key(&entry.id, index, schedule, now);
                    if self.fired_keys.insert(key) {
                        due.push(DuePlan {
                            plan_id: entry.id.clone(),
                            reason: schedule_reason(schedule),
                        });
                    }
                }
            }
        }

        due
    }
}

fn is_due(schedule: &ScheduleRule, now: NaiveDateTime) -> bool {
    match schedule {
        ScheduleRule::Daily { time } => match parse_hhmm(time) {
            Ok((hour, minute)) => now.hour() == hour && now.minute() == minute,
            Err(_) => false,
        },
        ScheduleRule::Weekly { weekday, time } => match parse_hhmm(time) {
            Ok((hour, minute)) => {
                rust_weekday(now.weekday()) == *weekday
                    && now.hour() == hour
                    && now.minute() == minute
            }
            Err(_) => false,
        },
        ScheduleRule::Once { at } => parse_once_datetime(at).map(|at| now >= at).unwrap_or(false),
    }
}

fn fired_key(plan_id: &str, index: usize, schedule: &ScheduleRule, now: NaiveDateTime) -> String {
    match schedule {
        ScheduleRule::Daily { .. } => format!("{plan_id}:{index}:{}", now.date()),
        ScheduleRule::Weekly { .. } => {
            format!(
                "{plan_id}:{index}:{}-week-{}",
                now.year(),
                now.iso_week().week()
            )
        }
        ScheduleRule::Once { at } => format!("{plan_id}:{index}:{at}"),
    }
}

fn schedule_reason(schedule: &ScheduleRule) -> String {
    match schedule {
        ScheduleRule::Daily { time } => format!("daily {time}"),
        ScheduleRule::Weekly { weekday, time } => format!("weekly {weekday:?} {time}"),
        ScheduleRule::Once { at } => format!("once {at}"),
    }
}

fn rust_weekday(weekday: chrono::Weekday) -> Weekday {
    match weekday {
        chrono::Weekday::Mon => Weekday::Monday,
        chrono::Weekday::Tue => Weekday::Tuesday,
        chrono::Weekday::Wed => Weekday::Wednesday,
        chrono::Weekday::Thu => Weekday::Thursday,
        chrono::Weekday::Fri => Weekday::Friday,
        chrono::Weekday::Sat => Weekday::Saturday,
        chrono::Weekday::Sun => Weekday::Sunday,
    }
}
