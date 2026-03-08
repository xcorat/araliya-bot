# Agent / LLM Message Flow (from trace output)

Two turns captured: user sends `"hi"` then `"explain this project"`.

```mermaid
sequenceDiagram
    actor User
    participant Axum as axum0
    participant Agent as agentic-chat<br/>AgenticLoop
    participant ILlm as llm/instruct<br/>(Qwen 2.5)
    participant Tool as tools/execute<br/>newsmail_aggregator
    participant OAuth as Google OAuth
    participant MLlm as llm/complete<br/>(Qwen 2.5)

    Note over User,MLlm: Turn 1 — "hi"

    User->>Axum: "hi"
    Axum->>Agent: CommsMessage { content:"hi", session_id:None }

    Note over Agent: load/create session 019ca2d6

    Agent->>ILlm: LlmRequest { content: instruct_prompt<br/>[tool manifest + "hi"] }
    ILlm-->>Agent: '[{"tool":"newsmail_aggregator","action":"get","params":{"n_last":5}}]'<br/>(197 in / 29 out tokens)

    Note over Agent: parse_tool_calls → 1 call

    Agent->>Tool: ToolRequest { tool:"newsmail_aggregator", action:"get",<br/>args:{"n_last":5}, session_id:"019ca2d6" }
    Tool->>OAuth: token refresh
    OAuth-->>Tool: 401 invalid_grant — token revoked
    Tool-->>Agent: ToolResponse { ok:false, error:"refresh failed" }

    Note over Agent: context = "(no context retrieved)"

    Agent->>MLlm: LlmRequest {<br/>  system: persona prompt,<br/>  content: "Context: (no context retrieved)\n<br/>History: [9 prior turns]\n<br/>User: hi\nAI:" }
    MLlm-->>Agent: "Hello! How can I assist you today?"<br/>(274 in / 10 out tokens)

    Note over Agent: transcript_append assistant<br/>accumulate_spend

    Agent-->>Axum: CommsMessage { content:"Hello! How can I assist you today?",<br/>session_id:"019ca2d6" }
    Axum-->>User: "Hello! How can I assist you today?"

    Note over User,MLlm: Turn 2 — "explain this project"

    User->>Axum: "explain this project"
    Axum->>Agent: CommsMessage { content:"explain this project",<br/>session_id:"019ca2d6" }

    Note over Agent: load session 019ca2d6

    Agent->>ILlm: LlmRequest { content: instruct_prompt<br/>[tool manifest + "explain this project"] }
    ILlm-->>Agent: '[{"tool":"newsmail_aggregator","action":"get","params":{"n_last":500}}]'<br/>(199 in / 31 out tokens)

    Agent->>Tool: ToolRequest { tool:"newsmail_aggregator", action:"get",<br/>args:{"n_last":500}, session_id:"019ca2d6" }
    Tool->>OAuth: token refresh
    OAuth-->>Tool: 401 invalid_grant — token revoked
    Tool-->>Agent: ToolResponse { ok:false, error:"refresh failed" }

    Note over Agent: context = "(no context retrieved)"

    Agent->>MLlm: LlmRequest {<br/>  system: persona prompt,<br/>  content: "Context: (no context retrieved)\n<br/>History: [10 prior turns]\n<br/>User: explain this project\nAI:" }
    MLlm-->>Agent: "I'm ready to provide information and answer questions.<br/>What would you like to know about this project?"<br/>(291 in / 21 out tokens)

    Agent-->>Axum: CommsMessage { content:"I'm ready...", session_id:"019ca2d6" }
    Axum-->>User: "I'm ready to provide information..."
```

## Observations from this trace

- **Instruction LLM is too eager** — for both `"hi"` and `"explain this project"` it calls
  `newsmail_aggregator`, which is inappropriate. The instruct prompt needs tighter guidance
  or the tool manifest needs better descriptions.
- **OAuth token is revoked** — `newsmail_aggregator` fails both turns; agent degrades
  gracefully to `(no context retrieved)` and still responds.
- **History grows each turn** — response pass carries the full prior transcript (9 → 10
  turns), consuming more tokens each request (274 → 291 input tokens).
- **Session persists** — `session_id: 019ca2d6` is reused across both turns correctly.
