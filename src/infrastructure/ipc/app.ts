import { invoke } from "./client";

export async function frontendHeartbeat() {
  return invoke<void>("frontend_heartbeat");
}
