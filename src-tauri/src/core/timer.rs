//! 订阅自动更新任务。
//!
//! 每个开启了更新间隔的订阅都会在 [`crate::core::scheduler`] 中注册一个
//! `profile.update:<uid>` 周期任务。配置变化时调用 [`sync`] 同步任务集合。

use crate::config::Config;
use crate::core::scheduler::{JobFuture, Schedule, Scheduler};
use crate::feat;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

const TASK_PREFIX: &str = "profile.update:";

/// 按当前订阅配置注册、更新或移除任务。
pub fn sync() -> Result<()> {
    let desired = gen_map();
    let scheduler = Scheduler::global();

    for key in scheduler.keys_with_prefix(TASK_PREFIX) {
        let Some(uid) = key.strip_prefix(TASK_PREFIX) else {
            continue;
        };
        if !desired.contains_key(uid) {
            scheduler.remove(&key)?;
        }
    }

    let desired_count = desired.len();
    for (uid, minutes) in desired {
        let key = format!("{TASK_PREFIX}{uid}");
        let run = {
            let uid = uid.clone();
            Arc::new(move || {
                let uid = uid.clone();
                let future: JobFuture = Box::pin(async move {
                    log::info!(target: "app", "running scheduled task `profile.update` uid={uid}");
                    feat::update_profile(uid, None).await
                });
                future
            })
        };

        scheduler.upsert(key, Schedule::Minutes(minutes), run)?;
    }

    log::debug!(
        target: "app",
        "profile update schedules synced: {desired_count} task(s)"
    );
    Ok(())
}

/// 启动时补跑已经超过更新间隔的订阅。
pub fn catch_up() -> Result<()> {
    let cur_timestamp = chrono::Local::now().timestamp();
    let profiles = Config::profiles();
    let profiles = profiles.latest();

    let Some(items) = profiles.get_items() else {
        return Ok(());
    };

    for item in items {
        let Some(uid) = item.uid.as_ref() else {
            continue;
        };
        let Some(minutes) = item
            .option
            .as_ref()
            .and_then(|option| option.update_interval)
        else {
            continue;
        };
        let Some(updated) = item.updated else {
            continue;
        };
        if minutes == 0 {
            continue;
        }

        let interval = i64::try_from(minutes)
            .unwrap_or(i64::MAX)
            .saturating_mul(60);
        let updated = i64::try_from(updated).unwrap_or(i64::MAX);

        if cur_timestamp.saturating_sub(updated) >= interval {
            let key = format!("{TASK_PREFIX}{uid}");
            if !Scheduler::global().trigger(&key)? {
                log::warn!(target: "app", "scheduled task `{key}` is not registered");
            }
        }
    }

    Ok(())
}

fn gen_map() -> HashMap<String, u64> {
    let mut map = HashMap::new();

    let profiles = Config::profiles();
    let profiles = profiles.latest();

    let Some(items) = profiles.get_items() else {
        return map;
    };

    for item in items {
        let Some(option) = item.option.as_ref() else {
            continue;
        };
        let interval = option.update_interval.unwrap_or(0);
        let Some(uid) = item.uid.clone() else {
            continue;
        };

        if interval > 0 {
            map.insert(uid, interval);
        }
    }

    map
}
