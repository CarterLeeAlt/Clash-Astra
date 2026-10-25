import { ReactNode, useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { ErrorBoundary, FallbackProps } from "react-error-boundary";

const AUTO_RELOAD_KEY = "clash-astra:error-auto-reloads";
const AUTO_RELOAD_WINDOW_MS = 10 * 60 * 1000;
const AUTO_RELOAD_LIMIT = 3;
const AUTO_RELOAD_DELAY_MS = 3000;

// 抛出的不一定是 Error（如 throw undefined），读 message/stack 不能再抛异常，
// 否则 fallback 自己崩掉，整棵树卸载成白屏。
const describeError = (error: unknown) => {
  const value = error as { message?: unknown; stack?: unknown } | null;
  const message =
    typeof value?.message === "string" ? value.message : String(error);
  const stack = typeof value?.stack === "string" ? value.stack : undefined;
  return { message, stack };
};

/** 记录一次自动重载；窗口期内次数已满时返回 false，避免重载循环。 */
const claimAutoReload = () => {
  try {
    const now = Date.now();
    const raw = window.sessionStorage.getItem(AUTO_RELOAD_KEY);
    const history = (JSON.parse(raw || "[]") as number[]).filter(
      (time) => typeof time === "number" && now - time < AUTO_RELOAD_WINDOW_MS
    );
    if (history.length >= AUTO_RELOAD_LIMIT) return false;
    history.push(now);
    window.sessionStorage.setItem(AUTO_RELOAD_KEY, JSON.stringify(history));
    return true;
  } catch {
    return false;
  }
};

function ErrorFallback({
  error,
  autoReload,
}: FallbackProps & { autoReload: boolean }) {
  const { message, stack } = describeError(error);

  useEffect(() => {
    if (!autoReload || !claimAutoReload()) return;
    const timer = window.setTimeout(
      () => window.location.reload(),
      AUTO_RELOAD_DELAY_MS
    );
    return () => window.clearTimeout(timer);
  }, [autoReload]);

  return (
    <div role="alert" style={{ padding: 16 }}>
      <h4>Something went wrong:(</h4>

      <pre>{message}</pre>

      <button
        type="button"
        style={{ marginTop: 8, padding: "4px 12px", cursor: "pointer" }}
        onClick={() => window.location.reload()}
      >
        重新加载界面
      </button>

      <details title="Error Stack">
        <summary>Error Stack</summary>
        <pre>{stack}</pre>
      </details>
    </div>
  );
}

interface Props {
  children?: ReactNode;
  /** 顶层边界使用：整个界面崩溃时自动重载（10 分钟内最多 3 次）。 */
  autoReload?: boolean;
}

export const BaseErrorBoundary = (props: Props) => {
  const autoReload = props.autoReload ?? false;
  return (
    <ErrorBoundary
      fallbackRender={(fallbackProps) => (
        <ErrorFallback {...fallbackProps} autoReload={autoReload} />
      )}
      onError={(error) => {
        const { message, stack } = describeError(error);
        invoke("report_frontend_error", { message, stack }).catch(
          () => undefined
        );
      }}
    >
      {props.children}
    </ErrorBoundary>
  );
};
