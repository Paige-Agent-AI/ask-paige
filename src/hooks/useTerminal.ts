import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface TerminalLine {
  stream: "stdout" | "stderr" | "system";
  text: string;
  timestamp: number;
}

export interface TerminalSessionInfo {
  session_id: string;
  pid: number;
  command_line: string;
  cwd: string;
  status: string;
  started_at: number;
}

export interface ApprovalRequired {
  type: "approval_required";
  token: string;
  reason: string;
  command_line: string;
}

export interface SessionStarted {
  type: "session";
  session_id: string;
  pid: number;
  command_line: string;
  cwd: string;
  status: string;
  started_at: number;
}

export type TerminalExecuteResult = ApprovalRequired | SessionStarted;

export function useTerminal() {
  const [lines, setLines] = useState<TerminalLine[]>([]);
  const [session, setSession] = useState<TerminalSessionInfo | null>(null);
  const [pendingApproval, setPendingApproval] = useState<ApprovalRequired | null>(null);
  const [busy, setBusy] = useState(false);
  const unlistenOutput = useRef<UnlistenFn | null>(null);
  const unlistenExit = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    let mounted = true;

    (async () => {
      const out = await listen<{ session_id: string; stream: string; line: string; timestamp: number }>(
        "terminal-output",
        (event) => {
          if (!mounted) return;
          const { session_id, stream, line, timestamp } = event.payload;
          if (session && session.session_id !== session_id) return;
          setLines((prev) => [...prev, { stream: stream as "stdout" | "stderr", text: line, timestamp }]);
        },
      );
      const exit = await listen<{ session_id: string; exit_code: number; finished_at: number; status: string }>(
        "terminal-exit",
        (event) => {
          if (!mounted) return;
          const { session_id, exit_code, status } = event.payload;
          if (session && session.session_id !== session_id) return;
          setBusy(false);
          setSession((prev) =>
            prev ? { ...prev, status: status === "killed" ? `killed` : `exited:${exit_code}` } : prev,
          );
          setLines((prev) => [
            ...prev,
            { stream: "system", text: `Process ${status} (exit code ${exit_code})`, timestamp: Date.now() },
          ]);
        },
      );
      unlistenOutput.current = out;
      unlistenExit.current = exit;
    })();

    return () => {
      mounted = false;
      unlistenOutput.current?.();
      unlistenExit.current?.();
    };
  }, [session?.session_id]);

  const addSystemLine = useCallback((text: string) => {
    setLines((prev) => [...prev, { stream: "system", text, timestamp: Date.now() }]);
  }, []);

  const execute = useCallback(
    async (command: string, args: string[] = [], cwd?: string) => {
      if (busy) {
        addSystemLine("A command is already running.");
        return;
      }
      setBusy(true);
      setPendingApproval(null);
      setLines([]);
      try {
        const result = (await invoke("terminal_execute", {
          request: { command, args, cwd },
        })) as TerminalExecuteResult;

        if (result.type === "approval_required") {
          setPendingApproval(result);
          setBusy(false);
          addSystemLine(`Approval required: ${result.reason}`);
          return;
        }

        setSession(result);
        addSystemLine(`Started ${result.command_line} (pid ${result.pid})`);
      } catch (err) {
        setBusy(false);
        addSystemLine(`Error: ${String(err)}`);
      }
    },
    [busy, addSystemLine],
  );

  const approve = useCallback(
    async (token: string) => {
      if (busy) return;
      setBusy(true);
      setPendingApproval(null);
      try {
        const result = (await invoke("terminal_execute_approved", { token })) as TerminalExecuteResult;
        if (result.type === "approval_required") {
          setPendingApproval(result);
          setBusy(false);
          addSystemLine(`Approval required: ${result.reason}`);
          return;
        }
        setSession(result);
        addSystemLine(`Started ${result.command_line} (pid ${result.pid})`);
      } catch (err) {
        setBusy(false);
        addSystemLine(`Error: ${String(err)}`);
      }
    },
    [busy, addSystemLine],
  );

  const sendInput = useCallback(
    async (input: string) => {
      if (!session || !session.session_id) {
        addSystemLine("No active session.");
        return;
      }
      try {
        await invoke("terminal_input", { request: { session_id: session.session_id, input } });
      } catch (err) {
        addSystemLine(`Input error: ${String(err)}`);
      }
    },
    [session, addSystemLine],
  );

  const kill = useCallback(async () => {
    if (!session || !session.session_id) {
      addSystemLine("No active session.");
      return;
    }
    try {
      await invoke("terminal_kill", { sessionId: session.session_id });
      addSystemLine("Kill signal sent.");
    } catch (err) {
      addSystemLine(`Kill error: ${String(err)}`);
    }
  }, [session, addSystemLine]);

  return {
    lines,
    session,
    busy,
    pendingApproval,
    execute,
    approve,
    sendInput,
    kill,
  };
}
