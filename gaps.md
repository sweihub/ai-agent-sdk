# Feature Gaps: TypeScript (claude code) → Rust Port

Generated: 2026-04-23
Last updated: 2026-04-24 (v0.57.0)

## Resolved Gaps (v0.34.0 - v0.50.0)

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
- ✅ Auto Mode Classifier: PermissionMode::Auto with denial tracking, allowlist for safe tools, fallback after 3 denials, 14 tests
- ✅ Denial Tracking: DenialTrackingState with counter + threshold, manual Clone/Debug impl, integrated into Auto mode
- ✅ Gitignore Skill Check: is_path_gitignored() via git check-ignore, wired into load_skills_from_dir(), 2 tests
- ✅ side_query Module: SideQueryOptions builder, side_query() Anthropic format, side_query_simple() OpenAI format, side_query_with_tools(), SideQueryMemorySelection parser, 4 tests
- ✅ Memory Prefetch: find_relevant_memories() with LLM-based selection, loaded_nested_memory_paths on QueryEngineConfig, wired into query loop
- ✅ Deny Rule 4-Step Matching: tool_matches_rule in permissions.rs + PermissionContext::check_tool() implement exact → server-prefix → tool-prefix → wildcard, 12 tests
- ✅ Subagent Context Threading: can_use_tool, on_event, thinking, user_context, system_context threaded to both subagent creation sites
- ✅ Skill Argument Substitution: parse_argument_names() and substitute_arguments() with {{{arg}} pattern, 7 tests
- ✅ Nested Memory Dedup: loaded_nested_memory_paths prevents re-injection across parent/subagent engines
- ✅ AgentTool Proper Struct: `AgentTool` with `Tool` trait, `AgentToolConfig`, `create_agent_tool_executor()` factory, 4 tests
- ✅ Skill Shell Execution: `execute_shell_commands_in_prompt()` with block/inline pattern parsing, parallel execution, 21 tests
- ✅ InterruptBehavior Enforcement: `block` tools ignore abort signal, `cancel` tools respect it, 6 tests
- ✅ Settings Persistence: `settings/mod.rs` with read/write/merge, `persist_permission_update()` wired, 12 tests
- ✅ Skill Memoization: `load_all_skills_cached()` / `load_skills_from_dir_cached()` with LRU cache (50 entries), 5 tests
- ✅ Skill Shell Permission Gating: `can_execute` callback in `execute_shell_commands_in_prompt()`, PowerShell fallback to bash, 7 tests
- ✅ Pre/PostCompact hooks: Wired into snip, microcompact, and reactive compact paths in query_engine.rs
- ✅ PostToolUseFailure: Differentiated from PostToolUse success path in orchestration closure
- ✅ Hook Data Plane: `simulate_query_loop()` with QueryEngine, `query_model_without_streaming()` real API call, `query_model_without_streaming_impl()` with dual-format extraction, 29 tests
- ✅ Concurrency-Safe Tool Annotations: `ToolAnnotations::concurrency_safe()` on 20 read-only tools (FileRead, Glob, Grep, WebSearch, TaskList, TaskGet, ToolSearch, CronList, TodoWrite, Skill, TaskStop, TaskOutput, AskUserQuestion, Config, SendUserMessage, StructuredOutput, CronDelete, LSP, ReadMcpResourceTool, ListMcpResourcesTool), enabling `partition_tool_calls()` parallel execution



## 1. Agent / SubAgent (High Severity)

### Fork Subagents
- **TS:** `forkSubagent.ts` — inherits parent context, preserves prompt cache via `renderedSystemPrompt`, guards recursive forking with `isInForkChild()`
- **Rust:** ✅ Implemented — `build_forked_messages_from_sdk()`, `sdk_message_from_json()`, fork path wired in agent.rs with cache-safe params

### Background Agents
- **TS:** `run_in_background` spawns tokio task, returns TaskOutput reference. No 120s auto-background timer in TS source.
- **Rust:** ✅ Wired — `run_in_background` spawns tokio task, returns TaskOutput reference. Matches TS capability.

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
- **Rust:** ✅ Wired — `record_sidechain_transcript()` called after subagent execution in agent_tool.rs

### Context Threading
- **TS:** `createSubagentContext` clones file cache, provisions `contentReplacementState`, `renderedSystemPrompt`, `localDenialTracking`
- **Rust:** ✅ Wired — can_use_tool, on_event, thinking, user_context, system_context threaded to both subagent creation sites; fork subagents also inherit denial tracking

## 2. QueryEngine / Context Compaction (High Severity)

### Context Collapse
- **TS:** `contextCollapse/index.ts` — stub behind feature gate (all no-op functions)
- **Rust:** ✅ Stub — faithful translation of TS stub, same no-op functions, feature-gated

### Snip Compaction
- **TS:** Called in query loop (query.ts:396) before each API call
- **Rust:** ✅ Wired — called before microcompact at two locations in query loop

### Microcompact
- **TS:** Called pre-query in loop
- **Rust:** ✅ Wired — called at two locations in query loop before auto-compact

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
- **Rust:** ✅ Wired — triggered on 413 error in query loop

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
| **AgentTool** (proper) | As a `Box<dyn Tool>` with full schema, permissions, render methods | ✅ Implemented — `AgentTool` with `Tool` trait, `AgentToolConfig`, 4 tests |
| **MCPTool** | Wraps MCP server tools for LLM calling | ✅ Implemented — McpToolRegistry with callback dispatch, 6 tests |
| **TaskOutputTool** | Retrieve output from background tasks | ✅ Implemented — full tool with blocking/non-blocking modes, 6 tests |
| **BriefTool** | SendUserMessage, primary visible output channel | ✅ Implemented — full translation with attachments, status, 6 tests |
| **DiscoverSkillsTool** | On-demand skill discovery | ✅ Stub — name constant exported, matching TS feature-gated prompt-only pattern |
| **SnipTool** | Model-callable compaction tool | ✅ Stub — name constant exported, matching TS feature-gated pattern |
| **SyntheticOutputTool** | Structured output enforcement | ✅ Implemented — with_schema() support, 3 tests |
| **AgentTool** (proper) | As a `Box<dyn Tool>` with full schema, permissions, render methods | Medium — registered as inline closures in agent.rs (2 sites); functional but not a proper Tool struct |
| **TerminalCaptureTool** | Terminal capture | Low |
| **VerifyPlanExecutionTool** | Plan execution verification | Low |

### Tool Pipeline Gaps

| Gap | TS | Rust |
|-----|----|----|
| `assembleToolPool` | Deduplicates built-in + MCP tools by name, sorts alphabetically (prompt cache stability) | ✅ Implemented — assemble.rs with sorting + dedup, wired in query_engine.rs, 8 tests |
| `StreamingToolExecutor` | Concurrent vs serial tool execution | ✅ Wired — 20 tools marked `concurrency_safe` (FileRead, Glob, Grep, WebSearch, TaskList, TaskGet, ToolSearch, CronList, TodoWrite, Skill, TaskStop, TaskOutput, AskUserQuestion, Config, SendUserMessage, StructuredOutput, CronDelete, LSP, ReadMcpResourceTool, ListMcpResourcesTool). `partition_tool_calls()` batches concurrent-safe tools; non-safe tools run serially. |
| `interruptBehavior` | `'cancel'` vs `'block'` checked when user submits mid-tool | ✅ Enforced — `Block` tools ignore abort signal, `Cancel` tools respect it, 6 tests |
| `filterToolsByDenyRules` | Server-prefix stripping for MCP deny rules | ✅ Implemented — 4-step matching in assemble.rs + permissions.rs |
| `backfillObservableInput` | Backfills observable input for transparency | ✅ Wired — tool_backfill_fns in QueryEngine |
| `toAutoClassifierInput` | Auto-mode security classification | ✅ Integrated — PermissionMode::Auto with allowlist + denial tracking |

## 4. Hooks (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| Function hooks | JS/TS handlers run inline | ✅ Implemented — `add_function_hook()` / `remove_function_hook()` infrastructure in session_hooks.rs. Data plane wired: `simulate_query_loop()` uses QueryEngine, `query_model_without_streaming()` makes real API calls, `query_model_without_streaming_impl()` in api_query_hook_helper works. |
| Wiring into query loop | PreToolUse → canUseTool → tool call → PostToolUse → PostToolUseFailure, sequenced | ✅ Wired — free functions called from orchestration closure at lines 2042-2070 |
| Skill hook integration | `registerFrontmatterHooks` auto-registers skill hooks | ✅ Wired — register_hooks_from_skills() called in init_engine(), YAML hooks parsing with serde_yaml, 10 tests |
| Structured output enforcement | `registerStructuredOutputEnforcement` hook | ✅ Implemented — `register_structured_output_enforcement()` calls `add_function_hook()` with Stop event callback checking `has_successful_tool_call()` |
| Failure hooks | `PostToolUseFailure` differentiated from success | ✅ Differentiated — `run_post_tool_use_failure_hooks()` fired on `Err`, `run_post_tool_use_hooks()` fired on `Ok` |
| Pre/PostCompact hooks | Executed during compaction | ✅ Wired — `execute_pre_compact_hooks()` / `execute_post_compact_hooks()` called around snip, microcompact, and reactive compact paths |

## 5. Permissions (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| `canUseTool` callback | 6-parameter fn returning 3-way `PermissionDecision` (allow/deny/ask) + `updatedInput` | ✅ Partial — PermissionResult::Allow/Deny/Ask variants handled in orchestration closure. Ask returns error in SDK. |
| Deny rule matching | 4-step matcher: exact → wildcard → server-prefix → tool-prefix | ✅ Implemented — tool_matches_rule + PermissionContext::check_tool() with 4-step matching, 12 tests |
| `PermissionResult::Ask` | User prompting for permission | Not handled — boolean return, no ask path |
| Dynamic rule updates | `applyPermissionUpdates` / `persistPermissionUpdates` | ✅ Implemented — `persist_permission_update()` with settings file I/O, 12 tests |
| Auto mode classifier | `classifierDecision` transcript-based classification | ✅ Implemented — PermissionMode::Auto with allowlist, denial tracking, fallback message, 14 tests |
| Denial tracking | Counter + threshold for fallback-to-prompting | ✅ Implemented — DenialTrackingState with counter + threshold, integrated into Auto mode

## 6. Session (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| NDJSON streaming | Incremental writes with 100ms drain timer, fire-and-forget for assistant messages | ✅ Implemented — SessionWriter with dequeue/drain/flush, 100ms drain timer, global pending queue, 12 tests |
| NDJSON escaping | Escapes U+2028/U+2029 for line-splitting receivers | ✅ Implemented — cli_ndjson_safe_stringify.rs with 8 tests |
| Resume support | Loads from `tailUuid`, applies `preservedSegment` relinks, dedup loop | ✅ Implemented — resume_session(), create_preserved_segment(), deduplicate_messages(), 7 tests |
| Sidechain transcripts | Per-agent transcript subdirectories | ✅ Implemented — `record_sidechain_transcript()` writes to {session_id}/sidechains/{agent_id}.jsonl |

## 7. Skills (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| Multi-source loading | User/project/local/policy/plugin/bundled/MCP directories | ✅ Implemented — load_all_skills() with bundled → user (~/.ai/skills) → project (<cwd>/.ai/skills) + dedup, 6 tests |
| Gitignore check | `isPathGitignored` filter | ✅ Implemented — is_path_gitignored() via git check-ignore, wired into load_skills_from_dir(), 2 tests |
| Skill hook integration | `registerFrontmatterHooks` | ✅ Wired — register_hooks_from_skills() called in init_engine() |
| Shell execution | `executeShellCommandsInPrompt` for frontmatter | ✅ Implemented — `execute_shell_commands_in_prompt()` with permission gating, PowerShell fallback, 21+ tests |
| Argument substitution | `parseArgumentNames` / `substituteArguments` | ✅ Implemented — parse_argument_names(), substitute_arguments() with {{{arg}} pattern, 7 tests |
| Discovery prefetch | `startSkillDiscoveryPrefetch` per iteration | Absent |
| DiscoverSkillsTool | On-demand discovery | Absent |
| Memoization | `lodash/memoize` cache | ✅ Implemented — LRU-memoized `load_all_skills_cached()` / `load_skills_from_dir_cached()`, 5 tests |

## 8. Memory (Medium Severity)

| Gap | TS | Rust |
|-----|----|----|
| Vector search | Embedding-based semantic search with RRF ranking | `find_relevant_memories.rs` exists with LLM-based selection, no embedding integration |
| Memory prefetch | `startRelevantMemoryPrefetch` consumed per user turn | ✅ Implemented — find_relevant_memories() with sideQuery LLM selection, wired into query loop |
| Nested memory dedup | `loadedNestedMemoryPaths` prevents re-injection | ✅ Implemented — loaded_nested_memory_paths on QueryEngineConfig, inherited by subagents |

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

## Remaining Gaps (v0.56.0)

Lower-impact gaps that require external dependencies or are stubs in TS:

- **Remote Teleport** — cloud execution via CCR API (external deps)
- **Vector search** — embedding-based semantic search for memory (external deps, LLM-based selection exists)
- **WorkflowTool** — workflow orchestration (stub in TS, skipped)
- **Skill discovery prefetch** — `startSkillDiscoveryPrefetch` per iteration (stub in TS)

Already implemented (no further work needed):
- ✅ AgentTool as proper Tool struct (v0.49.0)
- ✅ Skill shell execution (v0.50.0)
- ✅ Skill shell permission gating + PowerShell fallback (v0.53.0)
- ✅ Dynamic permission updates (already in codebase)
- ✅ InterruptBehavior enforcement (v0.51.0)
- ✅ Settings persistence (v0.51.0)
- ✅ Sidechain transcripts (v0.50.0)
- ✅ Skill memoization with LRU cache (v0.53.0)
- ✅ Hook data plane: `simulate_query_loop()`, `query_model_without_streaming()`, `query_model_without_streaming_impl()` with real API calls (v0.55.0)
- ✅ Structured output enforcement (hook registered, calls add_function_hook with Stop event) (v0.56.0)
- ✅ Pre/PostCompact hooks wired into snip, microcompact, and reactive compact paths (v0.56.0)
- ✅ PostToolUseFailure hooks differentiated from success path (v0.56.0)
