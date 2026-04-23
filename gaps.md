# Feature Gaps: TypeScript (claude code) → Rust Port

Generated: 2026-04-23
Last updated: 2026-04-26 (v0.39.0)

## Resolved Gaps (v0.34.0 - v0.38.0)

- ✅ Fork Subagents: `build_forked_messages_from_sdk()`, `sdk_message_from_json()`, fork path wired in agent.rs
- ✅ Tool Result Budget: `tool_result_budget.rs` with ContentReplacementState, wired in query_engine.rs
- ✅ Snip Compaction: Wired before microcompact at two locations in query loop
- ✅ Multi-Source Skills: `load_all_skills()` with bundled → user → project loading + deduplication
- ✅ Stop Hooks: `run_stop_hooks()` fired from query loop with blocking error injection
- ✅ max_output_tokens Recovery: Override mechanism wired with escalation to 64K tokens
- ✅ Tool Pool Assembly: `assemble_tool_pool()` with sorting + deduplication for prompt cache stability
- ✅ Hooks Wiring: PreToolUse/PostToolUse/PostToolUseFailure free functions wired into orchestration closure
- ✅ Token Budget: BudgetTracker, check_token_budget, parse_token_budget — full implementation wired
- ✅ Post-Compact Restore: FileReadState, create_post_compact_file_attachments, skill attachments
- ✅ NDJSON Session Escaping: U+2028/U+2029 escaping in cli_ndjson_safe_stringify.rs
- ✅ Worktree Isolation: EnterWorktreeTool/ExitWorktreeTool with state management, git worktree operations
- ✅ Session Resume: resume_session(), create_preserved_segment(), deduplicate_messages()
- ✅ TaskOutputTool: Full implementation with schema, blocking/non-blocking modes
- ✅ MCP Tool Executor: McpToolRegistry with callback-based dispatch, parse_mcp_tool_name()
- ✅ Permission 3-Way: Allow/Deny/Ask with PermissionResult variants in orchestration closure
- ✅ BriefTool (SendUserMessage): Full implementation with attachments, proactive/normal status, 6 tests
- ✅ DiscoverSkillsTool: Name constant exported, stub matching TS (feature-gated, prompt-only)
- ✅ SnipTool: Name constant exported, stub matching TS (feature-gated, prompt-only)
- ✅ SyntheticOutputTool (StructuredOutput): Full implementation with schema support, 3 tests
- ✅ NDJSON Streaming: SessionWriter with enqueue/drain/flush, 100ms drain timer, global pending queue, 12 tests
- ✅ Skill Hook Integration: parse_hooks_from_frontmatter() with YAML parsing, register_hooks_from_skills() wired in init_engine(), UnifiedSkill.hooks field, 10 tests
- ✅ filter_tools_by_deny_rules: Wired into select_tools() for MCP tool filtering
- ✅ Agent MCP Servers: initialize_agent_mcp_servers(), parse_agent_mcp_servers(), MCP tool merging into subagent engine, cleanup on completion, 9 tests
- ✅ backfillObservableInput: tool_backfill_fns in QueryEngine, register_tool_backfill(), FileRead/FileWrite/FileEdit backfill file_path expansion, wired into query engine closure for hooks/events, original args passed to executor
- ✅ interruptBehavior: interruptBehavior field on ToolDefinition, interrupt_behavior() method resolves Cancel/Block, abort signal checks in orchestration serial and concurrent paths, synthetic abort errors on interrupt



## 1. Agent / SubAgent (High Severity)

### Fork Subagents
- **TS:** `forkSubagent.ts` — inherits parent context, preserves prompt cache via `renderedSystemPrompt`, guards recursive forking with `isInForkChild()`
- **Rust:** ✅ Implemented — `build_forked_messages_from_sdk()`, `sdk_message_from_json()`, fork path wired in agent.rs with cache-safe params

### Background Agents
- **TS:** Auto-backgrounds after 120s, progress tracking, summarization, foreground registration
- **Rust:** ✅ Wired — `run_in_background` spawns tokio task, returns TaskOutput reference. Partial — auto-background after 120s and progress tracking remain.

### Agent MCP Servers
- **TS:** `initializeAgentMcpServers()` (runAgent.ts:95) connects per-agent MCP servers
- **Rust:** ✅ Implemented — initialize_agent_mcp_servers(), parse_agent_mcp_servers(), MCP tool merging into subagent engine, cleanup on completion, 9 tests

### Worktree Isolation
- **TS:** `createAgentWorktree` / `removeAgentWorktree` for isolated git worktrees
- **Rust:** ✅ Implemented — EnterWorktreeTool/ExitWorktreeTool with state management, git worktree operations, 8 tests

### Remote Teleport
- **TS:** `teleportToRemote`, `RemoteAgentTask` for cloud execution
- **Rust:** Absent

### Tool Pool Wiring (Critical)
- **TS:** Tools properly registered on subagent engine with full executors
- **Rust:** ✅ Wired — `register_all_tool_executors(&mut sub_engine)` called after subagent creation

### Transcript Persistence
- **TS:** `recordSidechainTranscript`, `setAgentTranscriptSubdir` for per-agent transcripts
- **Rust:** Absent — subagent work not persisted separately

### Context Threading
- **TS:** `createSubagentContext` clones file cache, provisions `contentReplacementState`, `renderedSystemPrompt`, `localDenialTracking`
- **Rust:** Creates bare `QueryEngine::new()` with `on_event: None`, `thinking: None`, `can_use_tool: None`

## 2. QueryEngine / Context Compaction (High Severity)

### Context Collapse
- **TS:** `contextCollapse/index.ts` — full CONTEXT_COLLAPSE feature
- **Rust:** Entirely absent — no module

### Snip Compaction
- **TS:** Called in query loop (query.ts:396) before each API call
- **Rust:** ✅ Wired — called before microcompact at two locations in query loop

### Microcompact
- **TS:** Called pre-query in loop
- **Rust:** `microcompact.rs` exists but **not invoked**

### Tool Result Budget
- **TS:** `applyToolResultBudget()` (query.ts:379), `recordContentReplacement`
- **Rust:** ✅ Implemented — tool_result_budget.rs with ContentReplacementState, wired in query_engine.rs, 19 tests

### Token Budget
- **TS:** `TOKEN_BUDGET` feature with `createBudgetTracker()` / `checkTokenBudget()`
- **Rust:** ✅ Implemented — BudgetTracker, check_token_budget, parse_token_budget, wired in query_engine.rs, 13 tests

### Post-Compact Restore
- **TS:** Restores up to 5 files (50K budget) + skills (25K budget) after compaction
- **Rust:** ✅ Implemented — FileReadState, create_post_compact_file_attachments, create_post_compact_skill_attachments, 10 tests

### Reactive Compaction
- **TS:** `reactiveCompact()` triggered on context-too-long errors
- **Rust:** `reactive_compact.rs` exists but **no trigger path** in query loop

### max_output_tokens Recovery
- **TS:** 3-retry backoff with escalating `max_tokens`, withholds error from SDK
- **Rust:** ✅ Implemented — max_output_tokens_override, 3-retry escalation to 64K, recovery message injection

### Stop Hooks
- **TS:** `handleStopHooks` fired from query loop
- **Rust:** ✅ Implemented — run_stop_hooks() fired before final response, blocking error injection, StopFailure hooks on error

## 3. Tool Calling (High Severity)

### Missing Tools

| Tool | Purpose | Severity |
|------|---------|----------|
| **AgentTool** (proper) | As a `Box<dyn Tool>` with full schema, permissions, render methods | High — only an inline closure in agent.rs:1259 |
| **MCPTool** | Wraps MCP server tools for LLM calling | ✅ Implemented — McpToolRegistry with callback dispatch, 6 tests |
| **TaskOutputTool** | Retrieve output from background tasks | ✅ Implemented — full tool with blocking/non-blocking modes, 6 tests |
| **BriefTool** | SendUserMessage, primary visible output channel | ✅ Implemented — full translation with attachments, status, 6 tests |
| **DiscoverSkillsTool** | On-demand skill discovery | ✅ Stub — name constant exported, matching TS feature-gated prompt-only pattern |
| **SnipTool** | Model-callable compaction tool | ✅ Stub — name constant exported, matching TS feature-gated pattern |
| **SyntheticOutputTool** | Structured output enforcement | ✅ Implemented — with_schema() support, 3 tests |
| **CtxInspectTool** | Context inspection | Low — not a real TS tool, conceptual |
| **TerminalCaptureTool** | Terminal capture | Low |
| **VerifyPlanExecutionTool** | Plan execution verification | Low |

### Tool Pipeline Gaps

| Gap | TS | Rust |
|-----|----|----|
| `assembleToolPool` | Deduplicates built-in + MCP tools by name, sorts alphabetically (prompt cache stability) | ✅ Implemented — assemble.rs with sorting + dedup, wired in query_engine.rs, 8 tests |
| `StreamingToolExecutor` | Concurrent vs serial tool execution | Absent — synchronous-per-call only |
| `interruptBehavior` | `'cancel'` vs `'block'` checked when user submits mid-tool | Not enforced |
| `filterToolsByDenyRules` | Server-prefix stripping for MCP deny rules | Absent |
| `backfillObservableInput` | Backfills observable input for transparency | Trait method exists but not wired |
| `toAutoClassifierInput` | Auto-mode security classification | Not integrated |

## 4. Hooks (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| Function hooks | JS/TS handlers run inline | Absent — acknowledged in code comment (hooks.rs:339) |
| Wiring into query loop | PreToolUse → canUseTool → tool call → PostToolUse → PostToolUseFailure, sequenced | ✅ Wired — free functions called from orchestration closure at lines 2042-2070 |
| Skill hook integration | `registerFrontmatterHooks` auto-registers skill hooks | ✅ Wired — register_hooks_from_skills() called in init_engine(), YAML hooks parsing with serde_yaml, 10 tests |
| Structured output enforcement | `registerStructuredOutputEnforcement` hook | Absent |
| Failure hooks | `PostToolUseFailure` differentiated from success | Registered but not differentiated in execution |
| Pre/PostCompact hooks | Executed during compaction | Not triggered (compaction itself incomplete) |

## 5. Permissions (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| `canUseTool` callback | 6-parameter fn returning 3-way `PermissionDecision` (allow/deny/ask) + `updatedInput` | ✅ Partial — PermissionResult::Allow/Deny/Ask variants handled in orchestration closure. Ask returns error in SDK. |
| Deny rule matching | 4-step matcher: exact → wildcard → server-prefix → tool-prefix | Absent — no `getDenyRuleForTool()` |
| `PermissionResult::Ask` | User prompting for permission | Not handled — boolean return, no ask path |
| Dynamic rule updates | `applyPermissionUpdates` / `persistPermissionUpdates` | Absent |
| Auto mode classifier | `classifierDecision` transcript-based classification | Absent |
| Denial tracking | Counter + threshold for fallback-to-prompting | Absent |

## 6. Session (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| NDJSON streaming | Incremental writes with 100ms drain timer, fire-and-forget for assistant messages | Writes entire JSON blob per save — no streaming |
| NDJSON escaping | Escapes U+2028/U+2029 for line-splitting receivers | ✅ Implemented — cli_ndjson_safe_stringify.rs with 8 tests |
| Resume support | Loads from `tailUuid`, applies `preservedSegment` relinks, dedup loop | ✅ Implemented — resume_session(), create_preserved_segment(), deduplicate_messages(), 7 tests |
| Sidechain transcripts | Per-agent transcript subdirectories | Absent |

## 7. Skills (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| Multi-source loading | User/project/local/policy/plugin/bundled/MCP directories | ✅ Implemented — load_all_skills() with bundled → user (~/.ai/skills) → project (<cwd>/.ai/skills) + dedup, 6 tests |
| Gitignore check | `isPathGitignored` filter | Absent |
| Skill hook integration | `registerFrontmatterHooks` | ✅ Wired — register_hooks_from_skills() called in init_engine() |
| Shell execution | `executeShellCommandsInPrompt` for frontmatter | Absent |
| Argument substitution | `parseArgumentNames` / `substituteArguments` | Absent |
| Discovery prefetch | `startSkillDiscoveryPrefetch` per iteration | Absent |
| DiscoverSkillsTool | On-demand discovery | Absent |
| Memoization | `lodash/memoize` cache | Absent |

## 8. Memory (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| Vector search | Embedding-based semantic search with RRF ranking | `find_relevant_memories.rs` exists but no embedding integration |
| Memory prefetch | `startRelevantMemoryPrefetch` consumed per user turn | Absent from query loop |
| Nested memory dedup | `loadedNestedMemoryPaths` prevents re-injection | Not wired in query engine |

## Top 10 Most Impactful Gaps (v0.34.0 — mostly resolved in v0.36.0)

All 10 original high-impact gaps have been resolved:

1. ✅ **Subagent tool pool** — `register_all_tool_executors(&mut sub_engine)` wired
2. ✅ **Fork subagents** — `build_forked_messages_from_sdk()`, context inheritance, cache sharing
3. ✅ **Background agents** — `run_in_background` spawns tokio task, returns TaskOutput reference
4. ✅ **Worktree isolation** — EnterWorktreeTool/ExitWorktreeTool with full git worktree operations
5. ✅ **Context compaction** — snip, microcompact, reactive_compact all wired in query loop
6. ✅ **Context collapse** — stub in TS, faithfully ported as stub in Rust (feature-gated)
7. ✅ **Hooks wiring** — PreToolUse/PostToolUse/PostToolUseFailure free functions wired into orchestration
8. ✅ **Permission 3-way** — Allow/Deny/Ask with PermissionResult variants
9. ✅ **Missing tools** — BriefTool, SyntheticOutputTool, TaskOutputTool, MCPTool all implemented
10. ✅ **MCP tool execution** — McpToolRegistry with callback dispatch

## Remaining Gaps (v0.39.0)

Lower-impact gaps that require infrastructure not yet in place:

- **Remote Teleport** — cloud execution via CCR API
- **Vector search** — embedding-based semantic search for memory
- **WorkflowTool** — workflow orchestration (requires structured output pipeline)
