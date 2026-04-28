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
        ScheduleRule::Once { at } => parse_once_datetime(at)
            .map(|at| {
                let age = now.signed_duration_since(at).num_seconds();
                (0..60).contains(&age)
            })
            .unwrap_or(false),
    }
}

fn fired_key(plan_id: &str, index: usize, schedule: &ScheduleRule, now: NaiveDateTime) -> String {
    match schedule {
        ScheduleRule::Daily { time } => format!("{plan_id}:{index}:daily:{time}:{}", now.date()),
        ScheduleRule::Weekly { weekday, time } => {
            format!(
                "{plan_id}:{index}:weekly:{weekday:?}:{time}:{}-week-{}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fired_keys_include_repeating_schedule_identity() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 1)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();

        let daily_nine = ScheduleRule::Daily {
            time: "09:00".to_string(),
        };
        let daily_later = ScheduleRule::Daily {
            time: "09:30".to_string(),
        };
        assert_ne!(
            fired_key("work", 0, &daily_nine, now),
            fired_key("work", 0, &daily_later, now)
        );

        let weekly_nine = ScheduleRule::Weekly {
            weekday: Weekday::Friday,
            time: "09:00".to_string(),
        };
        let weekly_later = ScheduleRule::Weekly {
            weekday: Weekday::Friday,
            time: "09:30".to_string(),
        };
        assert_ne!(
            fired_key("work", 0, &weekly_nine, now),
            fired_key("work", 0, &weekly_later, now)
        );
    }

    #[test]
    fn once_schedule_does_not_run_when_overdue() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 1)
            .unwrap()
            .and_hms_opt(18, 0, 0)
            .unwrap();

        assert!(!is_due(
            &ScheduleRule::Once {
                at: "2026-05-01T17:59:00".to_string()
            },
            now
        ));
        assert!(is_due(
            &ScheduleRule::Once {
                at: "2026-05-01T17:59:30".to_string()
            },
            now
        ));
        assert!(!is_due(
            &ScheduleRule::Once {
                at: "2026-05-01T18:00:30".to_string()
            },
            now
        ));
    }
}
