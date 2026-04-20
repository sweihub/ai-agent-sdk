This an agent sdk Rust port of `claude code` CLI project in typescript: ~/claudecode/openclaudecode

core features:
- agent / subagent
- session
- skills
- memory
- tools
- plugin
- hooks
- command
- permission
- context compaction

## Notices

Increment the version(major.minor.build) in Cargo.toml before git commit. (feature -> +minor, fix -> +build)

run all unit tests and examples before git commit.

Keep the original project's structure, flavor and logics, file name from camelCase to snake_case.

Localize all environment variables from prefix `CLAUDE_CODE_` to `AI_CODE_`, `ANTHROPIC_` to `AI_`.

Localize directory name `.claude` to `.ai`, file name `CLAUDE.md` to `AI.md`.

Unit tests go to `src/tests/` directory.

Ensure translated Rust file starts with a comment of its source TypeScript path.

Always check original typescript logics to fix the Rust issues.

Never create simplified Rust file from typescript, must completely translate.

No TODO and stub to Rust code, translate it!

Don't suspect `MiniMax` model issue, it must be your own fault!

Avoid using of `unsafe` in Rust.

Fix or suppress any build warnings, allow dead code!

Run all unit tests and examples when you think you are done!

Always translate README.md into READCN.md (Chinese) if any changes.

## Feature Gates

**ALWAYS enable ALL JavaScript feature-gated features when translating to Rust.** Do NOT skip features gated with `feature('FEATURE_NAME')` or `process.env.USER_TYPE === 'ant'` in TypeScript.

When translating TypeScript code:
1. Find all `feature()` calls and `process.env.*` checks in the source file
2. Enable ALL feature-gated functionality in Rust - do NOT conditionally compile
3. Register all tools that exist in TypeScript, even if they return null/not implemented
4. The Rust SDK should provide the same capabilities as the full TypeScript version

Examples of feature gates to ALWAYS enable:
- `feature('WEB_BROWSER_TOOL')` → enable WebBrowser tool
- `feature('MONITOR_TOOL')` → enable Monitor tool
- `feature('KAIROS')` → enable send_user_file tool
- `feature('REACTIVE_COMPACT')` → enable reactive compaction
- `feature('CONTEXT_COLLAPSE')` → enable context collapse
- `feature('TOKEN_BUDGET')` → enable token budget tracking
- `process.env.USER_TYPE === 'ant'` → enable ant-specific features

The Rust port should have ALL features enabled, not a subset.

