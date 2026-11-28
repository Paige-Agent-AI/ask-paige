import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, listen, type UnlistenFn } from "../adapter/core";

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
  const ownedSession = useRef<string | null>(null);
  const earlyEvents = useRef<Array<{ id: string; accept: () => void }> | null>(null);
  const listenersReady = useRef<Promise<void> | null>(null);

  useEffect(() => {
    let mounted = true;

    const subscriptions: UnlistenFn[] = [];
    async function subscribe<T extends { session_id: string }>(name: string, accept: (event: T) => void) {
      const unlisten = await listen<T>(name, ({ payload }) => {
        if (!mounted) return;
        if (payload.session_id === ownedSession.current) accept(payload);
        else earlyEvents.current?.push({ id: payload.session_id, accept: () => accept(payload) });
      });
      if (mounted) subscriptions.push(unlisten);
      else unlisten();
    }
    listenersReady.current = Promise.all([
      subscribe<{ session_id: string; stream: "stdout" | "stderr"; line: string; timestamp: number }>(
        "terminal-output", ({ stream, line, timestamp }) => {
          setLines((prev) => [...prev, { stream, text: line, timestamp }]);
        },
      ),
      subscribe<{ session_id: string; exit_code: number; status: string }>(
        "terminal-exit", ({ exit_code, status }) => {
          setBusy(false);
          setSession((prev) => prev ? { ...prev, status: status === "exited" ? `exited:${exit_code}` : status } : prev);
          setLines((prev) => [
            ...prev, { stream: "system", text: `Process ${status} (exit code ${exit_code})`, timestamp: Date.now() },
          ]);
        },
      ),
    ]).then(() => {});
    void listenersReady.current.catch(() => {});
    return () => {
      mounted = false;
      subscriptions.forEach((unlisten) => unlisten());
    };
  }, []);

  const addSystemLine = useCallback((text: string) => {
    setLines((prev) => [...prev, { stream: "system", text, timestamp: Date.now() }]);
  }, []);

  const acceptSession = useCallback((result: SessionStarted) => {
    ownedSession.current = result.session_id;
    setSession(result);
    addSystemLine(`Started ${result.command_line} (pid ${result.pid})`);
    for (const event of earlyEvents.current ?? []) {
      if (event.id === result.session_id) event.accept();
    }
  }, [addSystemLine]);

  const execute = useCallback(
    async (command: string, args: string[] = [], cwd?: string) => {
      if (busy || earlyEvents.current) {
        addSystemLine("A command is already running.");
        return;
      }
      ownedSession.current = null;
      earlyEvents.current = [];
      setBusy(true);
      setPendingApproval(null);
      setLines([]);
      try {
        await listenersReady.current;
        const result = (await invoke("terminal_execute", {
          request: { command, args, cwd },
        })) as TerminalExecuteResult;

        if (result.type === "approval_required") {
          setPendingApproval(result);
          setBusy(false);
          addSystemLine(`Approval required: ${result.reason}`);
          return;
        }

        acceptSession(result);
      } catch (err) {
        setBusy(false);
        addSystemLine(`Error: ${String(err)}`);
      } finally {
        earlyEvents.current = null;
      }
    },
    [busy, addSystemLine, acceptSession],
  );

  const approve = useCallback(
    async (token: string) => {
      if (busy || earlyEvents.current) return;
      ownedSession.current = null;
      earlyEvents.current = [];
      setBusy(true);
      setPendingApproval(null);
      try {
        await listenersReady.current;
        const result = (await invoke("terminal_execute_approved", { token })) as TerminalExecuteResult;
        if (result.type === "approval_required") {
          setPendingApproval(result);
          setBusy(false);
          addSystemLine(`Approval required: ${result.reason}`);
          return;
        }
        acceptSession(result);
      } catch (err) {
        setBusy(false);
        addSystemLine(`Error: ${String(err)}`);
      } finally {
        earlyEvents.current = null;
      }
    },
    [busy, addSystemLine, acceptSession],
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
