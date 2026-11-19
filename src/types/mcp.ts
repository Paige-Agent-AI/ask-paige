export interface McpTool {
  name: string;
  description?: string;
  input_schema: Record<string, unknown>;
}

export interface McpResource {
  uri: string;
  name?: string;
  mime_type?: string;
}

export interface McpPrompt {
  name: string;
  description?: string;
}

export interface McpCapabilities {
  tools: McpTool[];
  resources: McpResource[];
  prompts: McpPrompt[];
}

export interface McpServerStatus {
  id: string;
  name: string;
  kind: string;
  display: string;
  state: string;
  detail: string;
  capabilities: McpCapabilities;
  last_seen_at?: number;
  server_info?: Record<string, unknown>;
}

export interface McpStatus {
  servers: McpServerStatus[];
}

export interface McpReceipt {
  timestamp: number;
  connection_id: string;
  tool: string;
  outcome: string;
  summary: string;
  details: string;
}

export interface McpCallResult {
  connection_id: string;
  tool: string;
  content: unknown[];
  is_error: boolean;
}
