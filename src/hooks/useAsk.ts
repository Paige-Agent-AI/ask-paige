import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AskReceipt,
  AskStatus,
  AskTurnStarted,
  ChatToolEvent,
  GatewayHealth,
  ModelCandidate,
  ModelSelection,
  ToolResultEvent,
} from "../types/ask";

import type { WorkflowRequest } from "../types/chats";

export type AskMode = "plan" | "build";

export interface ComposerState {
  mode: AskMode;
  attachments: string[];
  contextRefs: string[];
  workflow?: WorkflowRequest;
  skillId?: string;
  specialistHandle?: string;
  modelChoice: string | null; // "auto", a gateway-qualified model id, or the saved preference
}

export interface UseAskResult {
  status: AskStatus | null;
  busy: boolean;
  error: string | null;
  clearError: () => void;
  selection: ModelSelection | null;
  toolEvents: ChatToolEvent[];
  receipts: AskReceipt[];
  liveActivity: AskReceipt[];
  settledTurn: string | null;
  activeTurnId: string | null;
  send: (message: string, composer: ComposerState, chatId: string | null) => Promise<AskTurnStarted | null>;
  resume: (chatId: string, workspaceRoot?: string, useOpenrouterFree?: boolean, afterRestart?: boolean) => Promise<AskTurnStarted | null>;
  cancel: () => Promise<void>;
  refresh: () => Promise<void>;
  connectOpenrouter: (key: string) => Promise<boolean>;
  disconnectOpenrouter: () => Promise<void>;
  connectOmniroute: (key: string) => Promise<void>;
  disconnectOmniroute: () => Promise<void>;
  setModelSelection: (selection: "auto" | "manual", pin: string | null) => Promise<void>;
}

export function useAsk(): UseAskResult {
  const [status, setStatus] = useState<AskStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const clearError = useCallback(() => setError(null), []);
  const [selection, setSelection] = useState<ModelSelection | null>(null);
  const [toolEvents, setToolEvents] = useState<ChatToolEvent[]>([]);
  const [receipts, setReceipts] = useState<AskReceipt[]>([]);
  const [liveActivity, setLiveActivity] = useState<AskReceipt[]>([]);
  const [settledTurn, setSettledTurn] = useState<string | null>(null);
  const [activeTurnId, setActiveTurnId] = useState<string | null>(null);
  const turnRef = useRef<string | null>(null);
  const earlyEvents = useRef<Array<(turnId: string) => void> | null>(null);
  const listenersReady = useRef<Promise<void> | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = (await invoke("ask_status")) as AskStatus;
      setStatus(s);
    } catch (e) {
      setError("Gateway probe failed: " + String(e));
    }
  }, []);

  useEffect(() => {
    let mounted = true;
    const subs: UnlistenFn[] = [];
    async function subscribe<T extends { turn_id: string }>(name: string, accept: (payload: T) => void) {
      const unsubscribe = await listen<T>(name, ({ payload }) => {
        if (!mounted) return;
        if (payload.turn_id === turnRef.current) accept(payload);
        else earlyEvents.current?.push((turnId) => {
          if (payload.turn_id === turnId) accept(payload);
        });
      });
      if (mounted) subs.push(unsubscribe);
      else unsubscribe();
    }
    const settle = (id: string) => {
      setSettledTurn(id);
      setBusy(false);
      turnRef.current = null;
      setActiveTurnId(null);
    };
    listenersReady.current = Promise.all([
      subscribe<{ turn_id: string; tool: string }>("ask-tool-start", (payload) => {
        setToolEvents((prev) => [...prev, { tool: payload.tool, ok: true, summary: "Started", timestamp: Date.now() }]);
      }),
      subscribe<ToolResultEvent>("ask-tool-result", (payload) => {
        setToolEvents((prev) => [...prev, { tool: payload.tool, ok: payload.ok, summary: payload.summary, timestamp: Date.now() }]);
      }),
      subscribe<AskReceipt>("ask-activity", (payload) => {
        setLiveActivity((previous) => [...previous, payload]);
      }),
      subscribe<{ turn_id: string }>("ask-done", (payload) => settle(payload.turn_id)),
      subscribe<{ turn_id: string; message: string; reply_saved?: boolean }>("ask-error", (payload) => {
        setError(payload.reply_saved ? null : payload.message);
        settle(payload.turn_id);
      }),
    ]).then(() => {});
    void listenersReady.current.catch(() => {});
    return () => {
      mounted = false;
      subs.forEach((unsubscribe) => unsubscribe());
    };
  }, []);

  useEffect(() => {
    refresh();
    void invoke<AskReceipt[]>("ask_receipts").then(setReceipts).catch(() => {});
  }, [refresh]);

  const start = useCallback(
    async (invokeTurn: () => Promise<AskTurnStarted>): Promise<AskTurnStarted | null> => {
      if (turnRef.current || earlyEvents.current) return null;
      earlyEvents.current = [];
      setBusy(true);
      setError(null);
      setSettledTurn(null);
      setToolEvents([]);
      setLiveActivity([]);
      try {
        await listenersReady.current;
        const started = await invokeTurn();
        turnRef.current = started.turn_id;
        setActiveTurnId(started.turn_id);
        setSelection(started.selection);
        earlyEvents.current.forEach((replay) => replay(started.turn_id));
        return started;
      } catch (e) {
        setBusy(false);
        setError(String(e));
        return null;
      } finally {
        earlyEvents.current = null;
      }
    },
    [],
  );

  const send = useCallback(
    (message: string, composer: ComposerState, chatId: string | null) => start(() => invoke<AskTurnStarted>("ask_send", {
      request: {
        message, mode: composer.mode, attachments: composer.attachments.map((path) => ({ path })),
        context_refs: composer.contextRefs, chat_id: chatId, persist_chat: true, model_choice: composer.modelChoice,
        workflow: composer.workflow, skill_id: composer.skillId, specialist_handle: composer.specialistHandle,
      },
    })),
    [start],
  );

  const resume = useCallback(
    (chatId: string, workspaceRoot?: string, useOpenrouterFree?: boolean, afterRestart?: boolean) => start(() =>
      invoke<AskTurnStarted>("ask_resume", { chatId, workspaceRoot, useOpenrouterFree, afterRestart })),
    [start],
  );

  const cancel = useCallback(async () => {
    if (!turnRef.current) return;
    try {
      await invoke("ask_cancel", { turnId: turnRef.current });
    } catch {
      /* non-fatal */
    }
  }, []);

  const connectOpenrouter = useCallback(
    async (key: string) => {
      try {
        const s = (await invoke("ask_connect_openrouter", { key })) as AskStatus;
        setStatus(s);
        return true;
      } catch (e) {
        setError(String(e));
        return false;
      }
    },
    [],
  );

  const disconnectOpenrouter = useCallback(async () => {
    try {
      const s = (await invoke("ask_disconnect_openrouter")) as AskStatus;
      setStatus(s);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const connectOmniroute = useCallback(
    async (key: string) => {
      try {
        const s = (await invoke("ask_connect_omniroute", { key })) as AskStatus;
        setStatus(s);
      } catch (e) {
        setError(String(e));
        throw e;
      }
    },
    [],
  );

  const disconnectOmniroute = useCallback(async () => {
    try {
      const s = (await invoke("ask_disconnect_omniroute")) as AskStatus;
      setStatus(s);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const setModelSelection = useCallback(
    async (sel: "auto" | "manual", pin: string | null) => {
      try {
        await invoke("ask_set_model_selection", { selection: sel, manualPin: pin });
        setStatus((current) => current && { ...current, selection: sel, manual_pin: pin });
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  return {
    status,
    busy,
    error,
    clearError,
    selection,
    toolEvents,
    receipts,
    liveActivity,
    settledTurn,
    activeTurnId,
    send,
    resume,
    cancel,
    refresh,
    connectOpenrouter,
    disconnectOpenrouter,
    connectOmniroute,
    disconnectOmniroute,
    setModelSelection,
  };
}

export type { AskStatus, AskReceipt, GatewayHealth, ModelCandidate };
