import { invoke } from "@tauri-apps/api/tauri";

export async function getVergeConfig() {
  return invoke<IVergeConfig>("get_verge_config");
}

export async function patchVergeConfig(payload: IVergeConfig) {
  return invoke<void>("patch_verge_config", { payload });
}

export interface SchedulerOneShotTaskInfo {
  key: string;
  remainingMs: number;
  generation: number;
}

export interface SchedulerRunningTaskInfo {
  key: string;
  generation: number;
  cancellable: boolean;
}

export interface SchedulerState {
  periodicTasks: unknown[];
  oneShotTasks: SchedulerOneShotTaskInfo[];
  runningTasks: SchedulerRunningTaskInfo[];
  pendingLimit: number;
}

export async function getSchedulerState() {
  return invoke<SchedulerState>("get_scheduler_state");
}
