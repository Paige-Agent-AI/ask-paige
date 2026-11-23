#!/usr/bin/env node
// Increment 1D — TEST-ONLY local authenticated MCP remote HTTPS server fixture
// (self-signed; only the 1D local test client uses it). Speaks JSON-RPC 2.0
// over POST on /mcp, using a session and streaming selected replies.
import https from "node:https";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PORT = 8443;
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

function computeResult(msg) {
  if (msg.method === "initialize") {
    return { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "paige-remote-test-server", version: "0.0.1" } };
  }
  if (msg.method === "tools/list") return { tools: TOOLS };
  if (msg.method === "tools/call") {
    const params = msg.params ?? {};
    if (params.name === "echo") return { content: [{ type: "text", text: String(params.arguments?.text ?? "") }] };
    if (params.name === "add") return { content: [{ type: "text", text: String(Number(params.arguments?.a) + Number(params.arguments?.b)) }] };
    return { content: [{ type: "text", text: `unknown tool: ${params.name}` }], isError: true };
  }
  if (msg.method === "ping") return {};
  if (msg?.id !== undefined) return {};
  return null;
}

function handleJsonRpc(msg) {
  if (msg?.id === undefined) return null;
  const result = computeResult(msg);
  if (result === null) return null;
  return { jsonrpc: "2.0", id: msg.id, result };
}

async function main() {
  const pfx = process.env.PAIGE_MCP_TEST_CERT
    ? path.resolve(process.env.PAIGE_MCP_TEST_CERT)
    : path.join(root, "scripts", "paige-mcp-remote-test.pfx");
  let pfxBuf;
  try {
    pfxBuf = fs.readFileSync(pfx);
  } catch (e) {
    console.error(`[remote-mcp-fixture] certificate file missing: ${pfx} (${e.message})`);
    process.exit(3);
  }
  const server = https.createServer({ pfx: pfxBuf, passphrase: "paige-mcp-test" }, (req, res) => {
    if (req.method !== "POST" || req.url !== "/mcp") {
      res.statusCode = 404;
      res.end("not found");
      return;
    }
    const auth = req.headers["authorization"] ?? "";
    const okToken = process.env.PAIGE_MCP_TEST_TOKEN
      ? auth === `Bearer ${process.env.PAIGE_MCP_TEST_TOKEN}`
      : true;
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      if (!okToken) {
        res.statusCode = 401;
        res.end("unauthorized");
        return;
      }
      let msg;
      try {
        msg = JSON.parse(body);
      } catch (e) {
        res.statusCode = 400;
        res.end(String(e));
        return;
      }
      const accepts = (req.headers.accept ?? "").split(",").map((value) => value.trim());
      if (!accepts.includes("application/json") || !accepts.includes("text/event-stream")) {
        res.writeHead(406).end();
        return;
      }
      if (msg.method === "initialize") {
        if (req.headers["mcp-session-id"] || msg.params?.protocolVersion !== "2025-06-18") {
          res.writeHead(400).end();
          return;
        }
        const session = String(++nextSession);
        sessions.set(session, false);
        res.setHeader("Mcp-Session-Id", session);
      } else {
        const session = req.headers["mcp-session-id"];
        if (!sessions.has(session) || req.headers["mcp-protocol-version"] !== "2025-06-18") {
          res.writeHead(400).end();
          return;
        }
        if (msg.method === "notifications/initialized") {
          sessions.set(session, true);
          res.writeHead(202).end();
          return;
        }
        if (!sessions.get(session)) {
          res.writeHead(400).end();
          return;
        }
        if (msg.method === "tools/call" && msg.params?.arguments?.text === "expire fixture session") {
          sessions.delete(session);
          res.writeHead(404).end();
          return;
        }
      }
      const result = handleJsonRpc(msg);
      if (result === null) {
        res.statusCode = 202;
        res.end();
      } else if (msg.method === "initialize" || msg.method === "tools/call") {
        res.setHeader("Content-Type", "text/event-stream; charset=utf-8");
        res.write(": keepalive\r\n\r\n");
        res.write("id: early\r\ndata:\r\n\r\n");
        res.write('data: {"jsonrpc":"2.0","method":"notifications/message"}\r\n\r\n');
        res.end("event: message\r\ndata: " + JSON.stringify(result) + "\r\n\r\n");
      } else {
        res.setHeader("Content-Type", "application/json");
        res.end(JSON.stringify(result));
      }
    });
  });
  await new Promise((res, rej) => {
    server.once("error", rej);
    server.listen(PORT, "127.0.0.1", res);
  });
  console.error(`[remote-mcp-fixture] https MCP server listening on https://127.0.0.1:${PORT}/mcp`);
}

let nextSession = 0;
const sessions = new Map();

main().catch((e) => {
  console.error("[remote-mcp-fixture] fatal:", e.message);
  process.exit(1);
});
