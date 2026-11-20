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

export type AskMode = "plan" | "build";

export interface ComposerState {
  mode: AskMode;
  attachments: string[];
  contextRefs: string[];
  modelChoice: "auto" | string; // "auto" or a model id
}

export interface UseAskResult {
  status: AskStatus | null;
  busy: boolean;
  error: string | null;
  selection: ModelSelection | null;
  streamingText: string;
  toolEvents: ChatToolEvent[];
  receipts: AskReceipt[];
  send: (message: string, composer: ComposerState, history: unknown[]) => Promise<void>;
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
  const [selection, setSelection] = useState<ModelSelection | null>(null);
  const [streamingText, setStreamingText] = useState("");
  const [toolEvents, setToolEvents] = useState<ChatToolEvent[]>([]);
  const [receipts, setReceipts] = useState<AskReceipt[]>([]);
  const turnRef = useRef<string | null>(null);
  const unlisteners = useRef<UnlistenFn[]>([]);

  const refresh = useCallback(async () => {
    try {
      const s = (await invoke("ask_status")) as AskStatus;
      setStatus(s);
    } catch (e) {
      setError("Gateway probe failed: " + String(e));
    }
  }, []);

  const loadReceipts = useCallback(async () => {
    try {
      const r = (await invoke("ask_receipts")) as AskReceipt[];
      setReceipts(r);
    } catch {
      /* non-fatal */
    }
  }, []);

  useEffect(() => {
    let mounted = true;
    (async () => {
      const subs: UnlistenFn[] = [];
      subs.push(
        await listen<{ turn_id: string; text: string }>("ask-delta", (event) => {
          if (!mounted || event.payload.turn_id !== turnRef.current) return;
          setStreamingText((prev) => prev + event.payload.text);
        }),
      );
      subs.push(
        await listen<{ turn_id: string; tool: string; call_id: string }>("ask-tool-start", (event) => {
          if (!mounted || event.payload.turn_id !== turnRef.current) return;
          setToolEvents((prev) => [
            ...prev,
            { tool: event.payload.tool + " …", ok: true, summary: "started", timestamp: Date.now() },
          ]);
        }),
      );
      subs.push(
        await listen<ToolResultEvent>("ask-tool-result", (event) => {
          if (!mounted || event.payload.turn_id !== turnRef.current) return;
          setToolEvents((prev) => [
            ...prev,
            {
              tool: event.payload.tool,
              ok: event.payload.ok,
              summary: event.payload.summary,
              timestamp: Date.now(),
            },
          ]);
          loadReceipts();
        }),
      );
      subs.push(
        await listen<{ turn_id: string; selection: ModelSelection }>("ask-model-selected", (event) => {
          if (!mounted) return;
          setSelection(event.payload.selection);
        }),
      );
      subs.push(
        await listen<{ turn_id: string; status: string; text: string }>("ask-done", (event) => {
          if (!mounted || event.payload.turn_id !== turnRef.current) return;
          setBusy(false);
          turnRef.current = null;
          loadReceipts();
        }),
      );
      subs.push(
        await listen<{ turn_id: string; message: string }>("ask-error", (event) => {
          if (!mounted || event.payload.turn_id !== turnRef.current) return;
          setBusy(false);
          turnRef.current = null;
          setError(event.payload.message);
        }),
      );
      unlisteners.current = subs;
    })();
    return () => {
      mounted = false;
      unlisteners.current.forEach((u) => u());
      unlisteners.current = [];
    };
  }, [loadReceipts]);

  useEffect(() => {
    refresh();
    loadReceipts();
  }, [refresh, loadReceipts]);

  const send = useCallback(
    async (message: string, composer: ComposerState, history: unknown[]) => {
      if (busy) return;
      setBusy(true);
      setError(null);
      setStreamingText("");
      setToolEvents([]);
      try {
        const started = (await invoke("ask_send", {
          request: {
            message,
            mode: composer.mode,
            attachments: composer.attachments.map((path) => ({ path })),
            context_refs: composer.contextRefs,
            history,
            model_choice: composer.modelChoice === "auto" ? null : composer.modelChoice,
          },
        })) as AskTurnStarted;
        turnRef.current = started.turn_id;
        setSelection(started.selection);
      } catch (e) {
        setBusy(false);
        setError(String(e));
      }
    },
    [busy],
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
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [refresh],
  );

  return {
    status,
    busy,
    error,
    selection,
    streamingText,
    toolEvents,
    receipts,
    send,
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
