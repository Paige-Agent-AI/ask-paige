import { useState } from "react";
import { Button } from "../components/Button";
import { Card } from "../components/Card";
import { SparklesIcon } from "../components/Icons";
import "./AskPaige.css";

export function AskPaige() {
  const [input, setInput] = useState("");

  return (
    <div className="ask-paige">
      <div className="ask-paige-hero">
        <div className="ask-paige-icon">
          <SparklesIcon />
        </div>
        <h2 className="ask-paige-title">Good evening.</h2>
        <p className="ask-paige-lead">
          Ask PAIGE to research, build, review or deploy. What would you like to work on?
        </p>
      </div>

      <Card className="ask-paige-examples">
        <p className="ask-paige-section-label">Try asking</p>
        <div className="ask-paige-chips">
          {[
            "Review the lab README and propose an edit",
            "Summarize what changed in the last activity log",
            "What capabilities are connected right now?",
          ].map((chip) => (
            <button
              key={chip}
              className="ask-paige-chip"
              onClick={() => setInput(chip)}
            >
              {chip}
            </button>
          ))}
        </div>
      </Card>

      <div className="ask-paige-input-bar">
        <input
          type="text"
          className="ask-paige-input"
          placeholder="Ask PAIGE anything..."
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && input.trim()) {
              // Future: send to PAIGE orchestration.
              setInput("");
            }
          }}
        />
        <Button variant="primary" disabled={!input.trim()}>
          Send
        </Button>
      </div>

      <p className="ask-paige-hint">
        PAIGE chat is connected to this workstation. Terminal, agent team and cloud
        integrations require separate authorization.
      </p>
    </div>
  );
}
