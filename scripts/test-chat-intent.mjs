import assert from "node:assert/strict";
import { test } from "node:test";
import { resolveChatIntent } from "../src/conversationIntent.ts";

const context = {
  skills: [{ id: "code-review", name: "Code review" }, { id: "regression-testing", name: "Regression testing" }],
  specialists: [{ slug: "project-reviewer", name: "Project Reviewer" }],
  attachments: [],
};
const pick = (message, overrides) => resolveChatIntent(message, { ...context, ...overrides });
const visual = "visual_qa_v1";

test("a request about PAIGE's current screen selects a one-time window capture", () => {
  for (const request of [
    "Look at this screen and show me what you’d improve.",
    "Capture your own window and propose a fix.",
    "Inspect the current PAIGE Desktop window and suggest a fix in src/styles.css.",
  ]) {
    const intent = pick(request);
    assert.equal(intent?.template, visual, request);
    assert.equal(intent.captureWindow, true, request);
    assert.equal(intent.proposalOnly, true, request);
  }
  assert.equal(pick("Look at this screen and show me what you'd improve.", {
    attachments: ["shots/selected.png"],
  }).before, "shots/selected.png");
  assert.equal(pick("Capture my entire screen and suggest a fix."), null);
  assert.equal(pick("Capture your own window every day and suggest a fix.").kind, "unavailable");
  assert.equal(pick("Look at this screen and suggest a fix without sharing it."), null);
});

test("a natural inspection request selects Visual QA and asks for missing files", () => {
  assert.deepEqual(pick("PAIGE, could you inspect the screenshot and propose a fix?"),
    { kind: "workflow", template: visual, proposalOnly: true, before: undefined, css: undefined });
  assert.equal(pick("Please inspect the screenshot and tell me what you see."), null);
  assert.deepEqual(pick('Please look at "Design QA\\Before.PNG" and suggest a fix in styles/main.css.'),
    { kind: "workflow", template: visual, proposalOnly: true, before: "Design QA/Before.PNG", css: "styles/main.css" });
  assert.deepEqual(pick("Find a fix for the clipped button in the attached screenshot.", { attachments: ["shots/a.png", "src/x.css"] }),
    { kind: "workflow", template: visual, proposalOnly: true, before: "shots/a.png", css: "src/x.css" });
});

test("visual routing never guesses between inputs or infers an out-of-project file", () => {
  assert.equal(pick("Inspect a.png or b.png and propose a fix in src/a.css.").before, undefined);
  assert.equal(pick("Inspect the screenshot ../before.png and propose a fix in C:\\repo\\x.css.").before, undefined);
  assert.equal(pick("Inspect the screenshot ../before.png and propose a fix in C:\\repo\\x.css.").css, undefined);
  assert.equal(pick("Inspect the screenshot and propose a fix without uploading it."), null);
  assert.equal(pick("Inspect the screenshot and propose a fix without sending it to OpenRouter."), null);
  assert.equal(pick("Inspect our screenshot and propose a fix but keep the screenshot on this device."), null);
  assert.equal(pick("Inspect the screenshot and propose a fix but don't send it off-device."), null);
  assert.equal(pick("Review screenshots/before.png and identify a fix in src/a.css without applying it or running tests.").proposalOnly, true);
  assert.equal(pick("Fix the bug without changing files."), null);
  assert.equal(pick("Inspect the screenshot and don't fix it."), null);
  assert.equal(pick("Inspect the screenshot and propose a fix, then apply it.").proposalOnly, false);
  assert.equal(pick("Fix the clipped label in this screenshot.").proposalOnly, false);
  assert.equal(pick("Check why the error happens every day."), null);
  assert.equal(pick("Fix the bug if the branch is mine."), null);
});

test("engineering and Word research use their existing workflow IDs", () => {
  assert.deepEqual(pick("Please fix the bug in the pinned worktree and run the tests."),
    { kind: "workflow", template: "engineering_v1" });
  assert.deepEqual(pick("Research the topic using official sources and write a Word report."),
    { kind: "workflow", template: "research_v1" });
  assert.deepEqual(pick("Review this code for regressions."), { kind: "skill", id: "code-review" });
  assert.deepEqual(pick("Use the Regression testing skill to reproduce this issue."), { kind: "skill", id: "regression-testing" });
  assert.equal(pick("Use the Regression testing skill to reproduce it.", { skills: [] }).kind, "unavailable");
  assert.deepEqual(pick("Could you ask the Project Reviewer to review the diff?"), { kind: "specialist", slug: "project-reviewer" });
  assert.equal(pick("Ask the Project Reviewer to review this.", { specialists: [] }), null);
});

test("existing direct commands, explicit mentions, hypothetical requests, and mixed operations keep control", () => {
  for (const request of [
    "/linear ANT-171",
    "@project-reviewer inspect the screenshot and propose a fix",
    "How do I inspect a screenshot and propose a fix?",
    "Don't inspect the screenshot or propose a fix",
    "If I ask you to inspect the screenshot and propose a fix, how would you do it?",
    '"Inspect the screenshot and propose a fix"',
    "Fix the bug and deploy the application.",
  ]) {
    assert.equal(pick(request), null, request);
  }
  assert.equal(pick("Schedule an automation to research this and write a Word report.").kind, "unavailable");
  assert.equal(pick("Research this and write a Word report every weekday.").kind, "unavailable");
});

test("uses an installed GitHub skill by its natural-language name", () => {
  const id = "github-aabbccddeeff001122334455";
  const skill = { id, name: "web-design-guidelines" };
  const request = "Please use the web design guidelines skill to review my settings.";
  assert.deepEqual(pick(request, { skills: [skill] }), { kind: "skill", id });
  assert.equal(pick(request, { skills: [] }).kind, "unavailable");
  assert.equal(pick(request, { skills: [skill, { ...skill, id: "other" }] }).kind, "unavailable");
});
