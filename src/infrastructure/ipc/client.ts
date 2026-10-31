import { invoke as tauriInvoke } from "@tauri-apps/api/tauri";

interface CommandFailure {
  code: string;
  message: string;
  stage?: string;
  recoveryError?: string;
}

class CommandError extends Error {
  readonly code: string;
  readonly stage?: string;
  readonly recoveryError?: string;

  constructor(failure: CommandFailure) {
    super(failure.message);
    this.name = "CommandError";
    this.code = failure.code;
    this.stage = failure.stage;
    this.recoveryError = failure.recoveryError;
  }
}

/** 所有 IPC 调用在边界统一解码错误，业务组件只处理 Error 和错误码。 */
export async function invoke<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    if (
      error &&
      typeof error === "object" &&
      "message" in error &&
      "code" in error
    ) {
      throw new CommandError(error as CommandFailure);
    }
    throw error instanceof Error ? error : new Error(String(error));
  }
}
