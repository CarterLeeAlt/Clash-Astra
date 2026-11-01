import { invoke } from "./client";

export async function getClashSettings() {
  return invoke<IClashSettings>("get_clash_settings");
}

export async function patchClashSettings(payload: IClashSettings) {
  return invoke<void>("patch_clash_settings", { payload });
}

interface SchedulerOneShotTaskInfo {
  key: string;
  remainingMs: number;
  generation: number;
}

interface SchedulerRunningTaskInfo {
  key: string;
  generation: number;
  cancellable: boolean;
}

interface SchedulerState {
  periodicTasks: unknown[];
  oneShotTasks: SchedulerOneShotTaskInfo[];
  runningTasks: SchedulerRunningTaskInfo[];
  pendingLimit: number;
}

export async function getSchedulerState() {
  return invoke<SchedulerState>("get_scheduler_state");
}
