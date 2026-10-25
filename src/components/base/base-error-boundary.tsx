import { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { ErrorBoundary, FallbackProps } from "react-error-boundary";

function ErrorFallback({ error }: FallbackProps) {
  return (
    <div role="alert" style={{ padding: 16 }}>
      <h4>Something went wrong:(</h4>

      <pre>{error.message}</pre>

      <button
        type="button"
        style={{ marginTop: 8, padding: "4px 12px", cursor: "pointer" }}
        onClick={() => window.location.reload()}
      >
        重新加载界面
      </button>

      <details title="Error Stack">
        <summary>Error Stack</summary>
        <pre>{error.stack}</pre>
      </details>
    </div>
  );
}

interface Props {
  children?: ReactNode;
}

export const BaseErrorBoundary = (props: Props) => {
  return (
    <ErrorBoundary
      FallbackComponent={ErrorFallback}
      onError={(error) => {
        invoke("report_frontend_error", {
          message: error.message,
          stack: error.stack,
        }).catch(() => undefined);
      }}
    >
      {props.children}
    </ErrorBoundary>
  );
};
