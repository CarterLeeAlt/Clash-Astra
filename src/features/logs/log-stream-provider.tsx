import { createContext, useContext, type ReactNode } from "react";
import { useLogStream } from "./use-log-stream";

interface LogStreamActions {
  clearLogs: () => void;
}

const LogStreamContext = createContext<LogStreamActions | null>(null);

export const useLogActions = () => {
  const context = useContext(LogStreamContext);
  if (!context) {
    throw new Error("useLogActions must be used within LogStreamProvider");
  }
  return context;
};

export const LogStreamProvider = ({ children }: { children: ReactNode }) => {
  const { clearLogs } = useLogStream();

  return (
    <LogStreamContext.Provider value={{ clearLogs }}>
      {children}
    </LogStreamContext.Provider>
  );
};
