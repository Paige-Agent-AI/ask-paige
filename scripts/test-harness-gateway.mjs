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
let heldContinuation = false;
let cutoffsAfterEdit = 0;
let unusableReplies = 0;

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

function toolCall(res, id, name, args, finishReason = "tool_calls") {
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
  if (finishReason) sse(res, { id: "c", choices: [{ index: 0, delta: {}, finish_reason: finishReason }] });
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
  if (req.method === "GET" && url === "/fixtures/continuation") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ held: heldContinuation }));
    return;
  }
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
      const wantsApprovalFixture = user === "MODE: BUILD.\nWrite the ANT-172 approval fixture.";

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
      if (user === "MODE: PLAN.\n@test-reviewer Delegate the fixture reads to a supporting specialist. Have them call read_file for the fixture files.") {
        const instruction = String(messages[0]?.content ?? "");
        const child = instruction.includes('"handle":"fixture-verifier"');
        const returned = messages.some(entry => entry.role === "assistant" &&
          String(entry.content).includes('"supporting_specialist":"fixture-verifier"'));
        if (!toolsSeen.length && parsed.tool_choice?.function?.name === "delegate_specialist") {
          toolCall(res, "assign-fixture-verifier", "delegate_specialist", {
            name: "Fixture Verifier", assignment: "Read the seven numbered fixture files.",
          });
        } else if (!toolsSeen.length) {
          finish(res, "stop", ["A supporting specialist can verify those files."]);
        } else if (child && toolsSeen.length === 1 && parsed.tool_choice?.function?.name !== "read_file") {
          finish(res, "stop", ["The fixture files were already mentioned."]);
        } else if (child && toolsSeen.length < 8) {
          toolCall(res, `specialist-read-${toolsSeen.length}`, "read_file", {
            path: `continuation-progress-${toolsSeen.length}.txt`,
          });
        } else if (child && !parsed.tools) {
          finish(res, "stop", ["The supporting specialist checked seven files."]);
        } else if (returned && !parsed.tools && instruction.includes('"handle":"test-reviewer"')) {
          finish(res, "stop", ["The lead reviewed the supporting specialist’s seven verified reads."]);
        } else {
          finish(res, "stop", ["The specialist context or tool limit was incorrect."]);
        }
        return;
      }
      if (user === "MODE: PLAN.\nVerify the closing reply at the tool limit.") {
        if (toolsSeen.length < 8) {
          toolCall(res, `limit-read-${toolsSeen.length}`, "read_file", {
            path: `continuation-progress-${toolsSeen.length + 1}.txt`,
          });
        } else {
          finish(res, "stop", [parsed.tools
            ? "The final reply unexpectedly allowed tools."
            : "I checked the eight requested files. The closing reply used those verified reads."]);
        }
        return;
      }
      if (user === "MODE: PLAN.\nVerify the repeated-read stop fixture.") {
        toolCall(res, `repeated-read-${toolsSeen.length}`, "read_file", { path: "continuation-progress-1.txt" });
        return;
      }
      if (user === "MODE: BUILD.\nVerify the Workbench Plan file proposal gate.") {
        const content = "Plan-verified fixture edit.\n";
        switch (toolsSeen.length) {
          case 0:
            toolCall(res, "plan-early-write", "write_file", { path: "continuation-progress-1.txt", content });
            break;
          case 1:
            toolCall(res, "plan-project-read", "read_file", { path: "continuation-progress-1.txt" });
            break;
          case 2:
            toolCall(res, "plan-submit", "submit_workbench_plan", {
              goal: "Update the fixture text", approach: "Replace the single existing line",
              files: ["continuation-progress-1.txt"], test_command: "node --test",
              expected_result: "Fixture tests pass", research_needed: false,
              research_note: "The native project read establishes the preimage.",
            });
            break;
          case 3:
            toolCall(res, "plan-late-write", "write_file", {
              path: "continuation-progress-1.txt", expected_sha256: JSON.parse(toolsSeen[1]).sha256,
              replacements: [{ old_text: "Independent input 1.\n", new_text: content }],
            });
            break;
          default:
            finish(res, "stop", ["Finished the Workbench Plan fixture."]);
        }
        return;
      }
      if (user === "MODE: BUILD.\nVerify the durable Engineering restart workflow.") {
        const count = toolsSeen.length;
        const test = "cargo test --offline --manifest-path Cargo.toml";
        if (count < 64) {
          toolCall(res, `eng-check-${count + 1}`, "read_file", { path: `checks/check-${count + 1}.txt` });
        } else if (count === 64) {
          toolCall(res, "eng-source", "read_file", { path: "src/lib.rs" });
        } else if (count === 65) {
          toolCall(res, "eng-plan", "submit_workbench_plan", {
            goal: "Return the expected fixture value", approach: "Change one value after verifying the source",
            files: ["src/lib.rs"], test_command: test, expected_result: "The Rust fixture test passes",
            research_needed: false, research_note: "The local Rust fixture shows the single mismatch.",
          });
        } else if (count === 66) {
          toolCall(res, "eng-approved-edit", "write_file", {
            path: "src/lib.rs", expected_sha256: JSON.parse(toolsSeen[64]).sha256,
            replacements: [{ old_text: "    41", new_text: "    42" }],
          });
        } else {
          finish(res, "stop", ["Unexpected model request after the Engineering restart gate."]);
        }
        return;
      }

      if (user === "MODE: BUILD.\nVerify recovery from an empty and a printed tool reply. Call the native tool.") {
        if (toolsSeen.length > 0) {
          finish(res, "stop", ["The fixture file was read with the native tool."]);
        } else if (unusableReplies++ === 0) {
          finish(res, "stop", ["  "]);
        } else if (unusableReplies === 2) {
          finish(res, "stop", ["I will use the tool.\n<tool_call>write_file\n<path>printed-call-must-not-exist.txt</path>\n</tool_call>"]);
        } else {
          toolCall(res, "reply-recovery-read", "read_file", { path: "continuation-progress-1.txt" });
        }
        return;
      }

      if (user === "MODE: BUILD.\nVerify recovery after a model cutoff.") {
        const edit = { path: "cutoff-fixture.txt", content: "One verified edit before cutoff.\n" };
        if (toolsSeen.length === 0) {
          toolCall(res, "cutoff-write", "write_file", edit);
        } else if (toolsSeen.length === 1 && cutoffsAfterEdit < 2) {
          const finishReason = cutoffsAfterEdit++ === 0 ? "length" : null;
          toolCall(res, "cutoff-discard", "write_file", {
            path: "cutoff-must-not-exist.txt", content: "An incomplete response must never execute.\n",
          }, finishReason);
        } else if (toolsSeen.length === 1) {
          toolCall(res, "cutoff-replayed-edit", "write_file", edit);
        } else if (cutoffsAfterEdit++ === 2) {
          sse(res, { choices: [{
            index: 0, finish_reason: "tool_calls",
            delta: { tool_calls: [{ index: 0, id: "truncated-arguments", type: "function",
              function: { name: "write_file", arguments: '{"path":"cutoff-must-not-exist.txt","content":' } }] },
          }] });
          res.end("data: [DONE]\n\n");
        } else if (cutoffsAfterEdit === 4) {
          sse(res, { choices: [{ index: 0, finish_reason: "stop",
            delta: { tool_calls: [{ index: 0, id: "incomplete-tool" }] } }] });
          res.end("data: [DONE]\n\n");
        } else {
          finish(res, "stop", [JSON.parse(toolsSeen.at(-1)).outcome === "already_applied"
            ? "Automatic recovery preserved the verified edit."
            : "Automatic recovery lost the verified edit."]);
        }
        return;
      }
      if (user === "MODE: BUILD.\nVerify bounded cutoff recovery.") {
        toolCall(res, "persistent-cutoff", "write_file", {
          path: "cutoff-must-not-exist.txt", content: "Never apply a cutoff.\n",
        }, "length");
        return;
      }
      if (user === "MODE: BUILD.\nVerify the disposable coding continuation fixture.") {
        const edit = { path: "continuation-fixture.txt", content: "One verified coding edit.\n" };
        if (toolsSeen.length === 0) {
          toolCall(res, "continuation-write", "write_file", edit);
        } else if (toolsSeen.length <= 10) {
          toolCall(res, `continuation-read-${toolsSeen.length}`, "read_file", {
            path: `continuation-progress-${toolsSeen.length}.txt`,
          });
        } else if (toolsSeen.length === 11 && !heldContinuation) {
          // Hold at the next model request so the isolated client can be killed
          // after the verified round and restarted with its original history.
          heldContinuation = true;
          res.flushHeaders();
        } else if (toolsSeen.length === 11) {
          toolCall(res, "continuation-repeat-with-new-id", "write_file", edit);
        } else {
          const replay = JSON.parse(toolsSeen.at(-1));
          finish(res, "stop", [replay.outcome === "already_applied"
            ? "Continuation completed. The verified edit was not repeated."
            : "Continuation returned an unexpected duplicate-edit result."]);
        }
        return;
      }
      if (wantsApprovalFixture) {
        if (toolsSeen.length === 0) {
          toolCall(res, "ant172-approval-write", "write_file", {
            path: "ant172-approval-demo.txt",
            content: "ANT-172 native approval fixture\n",
          });
        } else {
          const receipt = JSON.parse(toolsSeen.at(-1));
          finish(res, "stop", [receipt.readback_verified
            ? "The disposable approval file was written and its readback verified."
            : "The disposable write did not reach verified completion. Check the receipt."]);
        }
        return;
      }

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
