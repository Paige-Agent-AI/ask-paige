#!/usr/bin/env node
// Increment 1E - TEST-ONLY local OpenAI-compatible gateway harness.
//
// This is NOT a provider. It exists so the automated test suite can exercise
// PAIGE Desktop's full inference machinery (catalog fetch, model selection,
// SSE streaming, tool-calls, tool result handling, receipts, cancellation)
// end-to-end without paid calls. The Desktop app only connects to it when the
// insecure-test-bypass cargo feature is compiled in AND PAIGE_TEST_GATEWAY=1.
//
// Implements:
//   GET  /v1/models           -> catalog of three scripted "models"
//   POST /v1/chat/completions -> OpenAI streaming chat with tool-calls
import http from "node:http";

const PORT = 41011;

const MODELS = [
  { id: "harness/alpha-coder", tools: true },
  { id: "harness/beta-analyst", tools: true },
  { id: "harness/gamma-mini", tools: false },
];

function sse(res, obj) {
  res.write(`data: ${JSON.stringify(obj)}\n\n`);
}

function textChunks(res, parts) {
  for (const p of parts) {
    sse(res, { id: "c", choices: [{ index: 0, delta: { content: p } }] });
  }
}

function finish(res, finishReason, textParts) {
  sse(res, {
    id: "c",
    choices: [{ index: 0, delta: {}, finish_reason: finishReason }],
  });
  if (textParts) textChunks(res, textParts);
  sse(res, {
    id: "c",
    choices: [],
    usage: { prompt_tokens: 128, completion_tokens: 64, total_tokens: 192 },
  });
  res.write("data: [DONE]\n\n");
  res.end();
}

function toolCall(res, id, name, args) {
  sse(res, {
    id: "c",
    choices: [{
      index: 0,
      delta: {
        tool_calls: [{ index: 0, id, type: "function", function: { name, arguments: "" } }],
      },
    }],
  });
  const a = JSON.stringify(args);
  sse(res, {
    id: "c",
    choices: [{
      index: 0,
      delta: { tool_calls: [{ index: 0, function: { arguments: a } }] },
    }],
  });
  sse(res, { id: "c", choices: [{ index: 0, delta: {}, finish_reason: "tool_calls" }] });
  sse(res, { id: "c", choices: [], usage: { prompt_tokens: 64, completion_tokens: 32, total_tokens: 96 } });
  res.write("data: [DONE]\n\n");
  res.end();
}

function lastUserContent(messages) {
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === "user" && typeof messages[i].content === "string") {
      return messages[i].content;
    }
  }
  return "";
}

function toolResultContents(messages) {
  return messages.filter((m) => m.role === "tool").map((m) => String(m.content));
}

const server = http.createServer((req, res) => {
  const url = req.url;
  if (req.method === "GET" && url === "/v1/models") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ object: "list", data: MODELS.map((m) => ({ id: m.id, object: "model" })) }));
    return;
  }
  if (req.method === "POST" && url === "/v1/chat/completions") {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      let parsed = {};
      try { parsed = JSON.parse(body); } catch {}
      const model = String(parsed.model || "");
      const messages = Array.isArray(parsed.messages) ? parsed.messages : [];
      const user = lastUserContent(messages);
      const toolsSeen = toolResultContents(messages);
      const isPlan = /MODE:\s*PLAN/i.test(user);
      const wantsTests = /test/i.test(user);
      const wantsWrite = /write|overwrite|replace|create/i.test(user);
      const wantsSlow = /slow/i.test(user);

      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      });

      if (model === "harness/gamma-mini") {
        finish(res, "stop", ["I am a small harness model without tools; I cannot inspect or run the project."]);
        return;
      }

      if (model === "harness/beta-analyst") {
        finish(res, "stop", [
          "Analysis complete. The authorized project is a small Node.js module with a node:test suite; ",
          "its tests can be run with `node --test` and its sources read through the file broker. ",
          "(This is the beta-analyst scripted route: research-style answer, no tool use.)",
        ]);
        return;
      }

      // alpha-coder scripted engineering loop.
      if (wantsSlow) {
        // Slow stream used by the cancellation test.
        const send = (i) => {
          if (i >= 12) {
            finish(res, "stop", ["(done)"]);
            return;
          }
          sse(res, { id: "c", choices: [{ index: 0, delta: { content: "…streaming " + i + " " } }] });
          setTimeout(() => send(i + 1), 400);
        };
        send(0);
        return;
      }

      if (isPlan && wantsWrite) {
        if (toolsSeen.length === 0) {
          textChunks(res, ["PLAN mode: I will propose the edit via the write tool so the policy engine can enforce the boundary."]);
          toolCall(res, "call-plan-write", "write_file", { path: "math.js", content: "// this write must be refused in PLAN mode\n" });
        } else {
          const refused = toolsSeen.some((t) => t.includes("PLAN mode does not permit file writes"));
          finish(res, "stop", [
            refused
              ? "The write was refused by PAIGE's PLAN-mode policy, as designed. No file was modified. Proposed change: replace math.js's add implementation with the corrected version (diff provided in chat)."
              : "Unexpected tool result; stopping.",
          ]);
        }
        return;
      }

      if (wantsTests) {
        if (toolsSeen.length === 0) {
          textChunks(res, ["I will inspect the project, then run its actual test suite."]);
          toolCall(res, "call-list-1", "list_files", {});
        } else if (toolsSeen.length === 1) {
          textChunks(res, ["Now running the real test suite through the terminal engine."]);
          toolCall(res, "call-run-1", "run_command", { command: "node", args: ["--test"] });
        } else {
          const runResult = toolsSeen[toolsSeen.length - 1];
          let passed = false;
          let snippet = "";
          try {
            const parsedResult = JSON.parse(runResult);
            passed = parsedResult.exit_code === 0;
            snippet = String(parsedResult.stdout || "").split("\n").filter(Boolean).slice(0, 3).join(" | ");
          } catch {
            passed = runResult.includes('"exit_code":0');
          }
          finish(res, "stop", [
            passed
              ? `All tests passed. Command: node --test. Result: ${snippet || "exit code 0"}. The add() function's suite is green.`
              : `The test run reported failures (or a non-zero exit). Result: ${snippet || "see output"}.`,
          ]);
        }
        return;
      }

      // Generic inspect flow.
      if (toolsSeen.length === 0) {
        textChunks(res, ["Inspecting the authorized project."]);
        toolCall(res, "call-list-g", "list_files", {});
      } else {
        let first = null;
        try {
          const listResult = JSON.parse(toolsSeen[0]);
          const files = listResult.files || [];
          const js = files.find((f) => f.rel_path && f.rel_path.endsWith(".js"));
          first = js ? js.rel_path : null;
        } catch {}
        finish(res, "stop", [
          `Inspection complete. The project contains its sources${first ? ` (including ${first})` : ""} inside the authorized lab root; use them as the working set. (alpha-coder scripted route.)`,
        ]);
      }
    });
    return;
  }
  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`test harness gateway listening on http://127.0.0.1:${PORT}`);
});
