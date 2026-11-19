#!/usr/bin/env node
// Increment 1D — TEST-ONLY local MCP stdio server fixture (never a real vendor).
// Speaks JSON-RPC 2.0 over newline-delimited stdin/stdout.
import readline from "node:readline";

const VERSION = "2024-11-05";

const TOOLS = [
  {
    name: "echo",
    description: "Echo back the provided text.",
    inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
  },
  {
    name: "add",
    description: "Add two numbers.",
    inputSchema: { type: "object", properties: { a: { type: "number" }, b: { type: "number" } }, required: ["a", "b"] },
  },
];

function respond(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id, result }) + "\n");
}

function handleCall(id, params) {
  const name = params?.name;
  const args = params?.arguments ?? {};
  if (name === "echo") {
    respond(id, { content: [{ type: "text", text: String(args.text ?? "") }] });
  } else if (name === "add") {
    const sum = Number(args.a) + Number(args.b);
    respond(id, { content: [{ type: "text", text: String(sum) }] });
  } else {
    respond(id, { content: [{ type: "text", text: `unknown tool: ${name}` }], isError: true });
  }
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });
rl.on("line", (line) => {
  if (!line.trim()) return;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }
  const id = msg.id;
  if (msg.method === "initialize") {
    respond(id, {
      protocolVersion: VERSION,
      capabilities: { tools: {}, resources: {}, prompts: {} },
      serverInfo: { name: "paige-stdio-test-server", version: "0.0.1" },
    });
  } else if (msg.method === "tools/list") {
    respond(id, { tools: TOOLS });
  } else if (msg.method === "tools/call") {
    handleCall(id, msg.params);
  } else if (msg.method === "ping") {
    respond(id, {});
  } else if (msg.method && msg.method.startsWith("notifications/")) {
    // notifications get no response
  } else if (id !== undefined) {
    respond(id, {});
  }
});

// stderr markers for the runtime test harness.
console.error("paige-stdio-test-server: listening");
