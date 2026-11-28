import { useCallback, useEffect, useState } from "react";
import { invoke, listen, type UnlistenFn } from "../adapter/core";
import type { McpCallResult, McpReceipt, McpServerStatus, McpStatus, OAuthProvider } from "../types/mcp";

export interface McpMessageEvent {
  connection_id: string;
  stream: string;
  line: string;
  timestamp: number;
}

export interface McpToolResultEvent {
  connection_id: string;
  tool: string;
  outcome: string;
  summary: string;
}

export interface UseMcpResult {
  servers: McpServerStatus[];
  receipts: McpReceipt[];
  messages: McpMessageEvent[];
  error: string | null;
  refreshing: boolean;
  refresh: () => Promise<void>;
  addStdio: (id: string, name: string, command: string, args: string[], env: Record<string, string>, autoConnect: boolean) => Promise<McpServerStatus>;
  addRemote: (id: string, name: string, url: string, authHeader?: string, authToken?: string, autoConnect?: boolean) => Promise<McpServerStatus>;
  signIn: (provider: OAuthProvider, id?: string, target?: string) => Promise<McpServerStatus>;
  cancelSignIn: () => Promise<void>;
  connect: (id: string) => Promise<McpServerStatus>;
  disconnect: (id: string) => Promise<void>;
  reconnect: (id: string) => Promise<McpServerStatus>;
  revoke: (id: string) => Promise<void>;
  callTool: (connectionId: string, tool: string, args: Record<string, unknown>) => Promise<McpCallResult>;
}

export function useMcp(): UseMcpResult {
  const [servers, setServers] = useState<McpServerStatus[]>([]);
  const [receipts, setReceipts] = useState<McpReceipt[]>([]);
  const [messages, setMessages] = useState<McpMessageEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      const s = (await invoke("mcp_status")) as McpStatus;
      setServers(s.servers);
      setError(null);
    } catch (e) {
      setError("MCP status failed: " + String(e));
    } finally {
      setRefreshing(false);
    }
  }, []);

  const loadReceipts = useCallback(async () => {
    try {
      const r = (await invoke("mcp_receipts")) as McpReceipt[];
      setReceipts(r);
    } catch {
      /* non-fatal */
    }
  }, []);

  useEffect(() => {
    let mounted = true;
    const subs: UnlistenFn[] = [];
    (async () => {
      subs.push(
        await listen<McpMessageEvent>("mcp-message", (event) => {
          if (!mounted) return;
          setMessages((prev) => [...prev.slice(-99), event.payload]);
        }),
      );
      subs.push(
        await listen<McpToolResultEvent>("mcp-tool-result", () => {
          if (!mounted) return;
          loadReceipts();
        }),
      );
      const unlisten = await listen("mcp-status-changed", () => {
        if (mounted) void refresh();
      });
      if (mounted) subs.push(unlisten);
      else unlisten();
    })();
    return () => {
      mounted = false;
      subs.forEach((u) => u());
    };
  }, [loadReceipts, refresh]);

  useEffect(() => {
    refresh();
    loadReceipts();
  }, [refresh, loadReceipts]);

  const withRefresh = async <T,>(promise: Promise<T>): Promise<T> => {
    try {
      const result = await promise;
      await refresh();
      await loadReceipts();
      setError(null);
      return result;
    } catch (e) {
      await refresh();
      setError(String(e));
      throw e;
    }
  };

  const addStdio = useCallback(
    async (id: string, name: string, command: string, args: string[], env: Record<string, string>, autoConnect: boolean) => {
      return withRefresh(
        invoke("mcp_add_stdio", {
          request: { id, name, command, args, env, auto_connect: autoConnect },
        }) as Promise<McpServerStatus>,
      );
    },
    [refresh, loadReceipts],
  );

  const addRemote = useCallback(
    async (id: string, name: string, url: string, authHeader?: string, authToken?: string, autoConnect?: boolean) => {
      return withRefresh(
        invoke("mcp_add_remote", {
          request: { id, name, url, auth_header: authHeader, auth_token: authToken, auto_connect: autoConnect ?? false },
        }) as Promise<McpServerStatus>,
      );
    },
    [refresh, loadReceipts],
  );

  const connect = useCallback(
    async (id: string) => {
      return withRefresh(invoke("mcp_connect", { id }) as Promise<McpServerStatus>);
    },
    [refresh, loadReceipts],
  );

  const signIn = useCallback(
    (provider: OAuthProvider, id?: string, target?: string) => withRefresh(invoke<McpServerStatus>(
      "mcp_" + provider + "_oauth",
      { id: id ?? null, projectRef: provider === "supabase" ? target ?? null : null,
        repository: provider === "github" ? target ?? null : null,
        page: provider === "notion" ? target ?? null : null },
    )),
    [refresh, loadReceipts],
  );

  const disconnect = useCallback(
    async (id: string) => {
      await withRefresh(invoke("mcp_disconnect", { id }) as Promise<McpStatus>);
    },
    [refresh, loadReceipts],
  );

  const reconnect = useCallback(
    async (id: string) => {
      return withRefresh(invoke("mcp_reconnect", { id }) as Promise<McpServerStatus>);
    },
    [refresh, loadReceipts],
  );

  const revoke = useCallback(
    async (id: string) => {
      await withRefresh(invoke("mcp_revoke", { id }) as Promise<McpStatus>);
    },
    [refresh, loadReceipts],
  );

  const callTool = useCallback(
    async (connectionId: string, tool: string, args: Record<string, unknown>) => {
      return withRefresh(
        invoke("mcp_call_tool", {
          request: { connection_id: connectionId, tool, arguments: args },
        }) as Promise<McpCallResult>,
      );
    },
    [refresh, loadReceipts],
  );

  return {
    servers,
    receipts,
    messages,
    error,
    refreshing,
    refresh,
    addStdio,
    addRemote,
    signIn,
    cancelSignIn: () => invoke<void>("mcp_oauth_cancel"),
    connect,
    disconnect,
    reconnect,
    revoke,
    callTool,
  };
}
