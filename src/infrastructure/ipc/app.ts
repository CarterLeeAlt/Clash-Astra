import { invoke } from "./client";

export async function exitApp() {
  return invoke("exit_app");
}

export async function frontendHeartbeat() {
  return invoke<void>("frontend_heartbeat");
}
