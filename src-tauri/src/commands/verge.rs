use super::CmdResult;
use crate::{config::*, core::scheduler, feat, wrap_err};

#[tauri::command]
pub fn get_verge_config() -> CmdResult<IVerge> {
    Ok(Config::verge().data().clone())
}

#[tauri::command]
pub async fn patch_verge_config(payload: IVerge) -> CmdResult {
    wrap_err!(feat::patch_verge(payload).await)
}

#[tauri::command]
pub async fn test_delay(url: String) -> CmdResult<feat::TestDelayResult> {
    Ok(feat::test_delay(url)
        .await
        .unwrap_or(feat::TestDelayResult {
            delay: 10000u32,
            proxy: None,
        }))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerState {
    pub periodic_tasks: Vec<scheduler::PeriodicTaskInfo>,
    pub one_shot_tasks: Vec<scheduler::OneShotTaskInfo>,
    pub running_tasks: Vec<scheduler::RunningTaskInfo>,
    pub pending_limit: usize,
}

#[tauri::command]
pub fn get_scheduler_state() -> CmdResult<SchedulerState> {
    Ok(SchedulerState {
        periodic_tasks: scheduler::Scheduler::global().tasks(),
        one_shot_tasks: scheduler::OneShotScheduler::global().pending_tasks(),
        running_tasks: scheduler::OneShotScheduler::global().running_tasks(),
        pending_limit: scheduler::MAX_PENDING_ONE_SHOT_TASKS,
    })
}
