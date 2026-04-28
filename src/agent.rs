// Source: /data/home/swei/claudecode/openclaudecode/src/utils/model/agent.ts
use crate::env::EnvConfig;
use crate::error::AgentError;
use crate::query_engine::{QueryEngine, QueryEngineConfig};
use crate::stream::{CancelGuard, EventBroadcasters, EventSubscriber};
use crate::tools::ask::AskUserQuestionTool;
use crate::tools::bash::BashTool;
use crate::tools::brief::BriefTool;
use crate::tools::synthetic_output::SyntheticOutputTool;
use crate::tools::config::ConfigTool;
use crate::tools::cron::{CronCreateTool, CronDeleteTool, CronListTool};
use crate::tools::edit::FileEditTool;
use crate::tools::glob::GlobTool;
use crate::tools::grep::GrepTool;
use crate::tools::lsp::LSPTool;
use crate::tools::mcp_resource_reader::ReadMcpResourceTool;
use crate::tools::mcp_resources::ListMcpResourcesTool;
use crate::tools::mcp_tool::McpTool;
use crate::tools::mcp_auth::McpAuthTool;
use crate::tools::monitor::MonitorTool;
use crate::tools::notebook_edit::NotebookEditTool;
use crate::tools::plan::{EnterPlanModeTool, ExitPlanModeTool};
use crate::tools::read::FileReadTool as ReadTool;
use crate::tools::remote_trigger::RemoteTriggerTool;
use crate::tools::search::ToolSearchTool;
use crate::tools::send_user_file::SendUserFileTool;
use crate::tools::skill::SkillTool;
use crate::tools::skill::register_skills_from_dir;
use crate::skills::loader::load_all_skills;
use crate::utils::hooks::register_hooks_from_skills;
use crate::tools::sleep_tool::SleepTool;
use crate::tools::powershell::powershell_tool::PowerShellTool;
use crate::tools::task_output::TaskOutputTool;
use crate::tools::tasks::{TaskCreateTool, TaskGetTool, TaskListTool, TaskUpdateTool};
use crate::tools::team::{SendMessageTool, TeamCreateTool, TeamDeleteTool};
use crate::tools::todo::TodoWriteTool;
use crate::tools::web_browser::WebBrowserTool;
use crate::tools::web_fetch::WebFetchTool;
use crate::tools::web_search::WebSearchTool;
use crate::tools::worktree::{EnterWorktreeTool, ExitWorktreeTool};
use crate::tools::write::FileWriteTool as WriteTool;
use crate::permission::{PermissionResult, PermissionAllowDecision, PermissionDenyDecision, PermissionDecisionReason};
use crate::types::AgentEvent;
use crate::types::*;
use crate::types::ToolRender;
use std::sync::Arc;
use tokio::sync::mpsc;

// Implement ToolRender trait for each tool type, delegating to the existing methods
impl ToolRender for BashTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <BashTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <BashTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <BashTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for ReadTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <ReadTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <ReadTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <ReadTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for WriteTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <WriteTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <WriteTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <WriteTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for GlobTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <GlobTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <GlobTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <GlobTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for GrepTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <GrepTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <GrepTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <GrepTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for FileEditTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <FileEditTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <FileEditTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <FileEditTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for SkillTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <SkillTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <SkillTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <SkillTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for MonitorTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <MonitorTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <MonitorTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <MonitorTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for SendUserFileTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <SendUserFileTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <SendUserFileTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <SendUserFileTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for WebBrowserTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <WebBrowserTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <WebBrowserTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <WebBrowserTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for WebFetchTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <WebFetchTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <WebFetchTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <WebFetchTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for WebSearchTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <WebSearchTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <WebSearchTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <WebSearchTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for NotebookEditTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <NotebookEditTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <NotebookEditTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <NotebookEditTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TaskCreateTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TaskCreateTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TaskCreateTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TaskCreateTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TaskListTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TaskListTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TaskListTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TaskListTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TaskUpdateTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TaskUpdateTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TaskUpdateTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TaskUpdateTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TaskGetTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TaskGetTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TaskGetTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TaskGetTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TaskOutputTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TaskOutputTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TaskOutputTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TaskOutputTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TodoWriteTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TodoWriteTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TodoWriteTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TodoWriteTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for CronCreateTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <CronCreateTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <CronCreateTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <CronCreateTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for CronDeleteTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <CronDeleteTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <CronDeleteTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <CronDeleteTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for CronListTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <CronListTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <CronListTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <CronListTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for ConfigTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <ConfigTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <ConfigTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <ConfigTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for EnterWorktreeTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <EnterWorktreeTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <EnterWorktreeTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <EnterWorktreeTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for ExitWorktreeTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <ExitWorktreeTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <ExitWorktreeTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <ExitWorktreeTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for EnterPlanModeTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <EnterPlanModeTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <EnterPlanModeTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <EnterPlanModeTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for ExitPlanModeTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <ExitPlanModeTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <ExitPlanModeTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <ExitPlanModeTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for AskUserQuestionTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <AskUserQuestionTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <AskUserQuestionTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <AskUserQuestionTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for ToolSearchTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <ToolSearchTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <ToolSearchTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <ToolSearchTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TeamCreateTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TeamCreateTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TeamCreateTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TeamCreateTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for TeamDeleteTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <TeamDeleteTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <TeamDeleteTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <TeamDeleteTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for SendMessageTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <SendMessageTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <SendMessageTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <SendMessageTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for SleepTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <SleepTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <SleepTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <SleepTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for PowerShellTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <PowerShellTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <PowerShellTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <PowerShellTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for LSPTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <LSPTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <LSPTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <LSPTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for RemoteTriggerTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <RemoteTriggerTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <RemoteTriggerTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <RemoteTriggerTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for ListMcpResourcesTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <ListMcpResourcesTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <ListMcpResourcesTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <ListMcpResourcesTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for ReadMcpResourceTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <ReadMcpResourceTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <ReadMcpResourceTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <ReadMcpResourceTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for McpTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <McpTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <McpTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <McpTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for McpAuthTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <McpAuthTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <McpAuthTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <McpAuthTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for BriefTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <BriefTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <BriefTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <BriefTool>::render_tool_result_message(self, content)
    }
}
impl ToolRender for SyntheticOutputTool {
    fn user_facing_name(&self, input: Option<&serde_json::Value>) -> String {
        <SyntheticOutputTool>::user_facing_name(self, input)
    }
    fn get_tool_use_summary(&self, input: Option<&serde_json::Value>) -> Option<String> {
        <SyntheticOutputTool>::get_tool_use_summary(self, input)
    }
    fn render_tool_result_message(&self, content: &serde_json::Value) -> Option<String> {
        <SyntheticOutputTool>::render_tool_result_message(self, content)
    }
}

/// Construct ToolRenderFns from any ToolRender implementor.
/// The tool instance is wrapped in Arc so the render closures can share ownership.
fn make_render_fns<T: ToolRender + 'static>(tool: T) -> crate::query_engine::ToolRenderFns {
    let tool = Arc::new(tool);
    let t2 = Arc::clone(&tool);
    let t3 = Arc::clone(&tool);
    crate::query_engine::ToolRenderFns {
        user_facing_name: Arc::new(move |input| tool.user_facing_name(input)),
        get_tool_use_summary: Some(Arc::new(move |input| t2.get_tool_use_summary(input))),
        get_activity_description: None,
        render_tool_result_message: Some(Arc::new(
            move |content, _progress, _options| t3.render_tool_result_message(content),
        )),
    }
}

/// Register all built-in tool executors
pub(crate) fn register_all_tool_executors(engine: &mut QueryEngine) {
    type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

    // Bash tool - clone tool and ctx into async block
    let bash_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = BashTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let bash_tool = BashTool::new();
    let bash_rf = self::make_render_fns(bash_tool);
    engine.register_tool_with_render("Bash".to_string(), bash_executor, bash_rf);

    // Read tool - register backfill function (expand file_path for observers)
    engine.register_tool_backfill(
        "Read".to_string(),
        |input: &mut serde_json::Value| {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let expanded = crate::utils::path::expand_path(fp);
                if let Some(obj) = input.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(expanded));
                }
            }
        },
    );

    // FileRead tool
    let read_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ReadTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let read_tool = ReadTool::new();
    let read_rf = self::make_render_fns(read_tool);
    engine.register_tool_with_render("Read".to_string(), read_executor, read_rf);

    // FileWrite tool
    let write_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WriteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let write_tool = WriteTool::new();
    let write_rf = self::make_render_fns(write_tool);
    engine.register_tool_with_render("Write".to_string(), write_executor, write_rf);
    // Write tool - register backfill function (expand file_path for observers)
    engine.register_tool_backfill(
        "Write".to_string(),
        |input: &mut serde_json::Value| {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let expanded = crate::utils::path::expand_path(fp);
                if let Some(obj) = input.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(expanded));
                }
            }
        },
    );

    // Glob tool
    let glob_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = GlobTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let glob_tool = GlobTool::new();
    let glob_rf = self::make_render_fns(glob_tool);
    engine.register_tool_with_render("Glob".to_string(), glob_executor, glob_rf);

    // Grep tool
    let grep_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = GrepTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let grep_tool = GrepTool::new();
    let grep_rf = self::make_render_fns(grep_tool);
    engine.register_tool_with_render("Grep".to_string(), grep_executor, grep_rf);

    // FileEdit tool - with rendering metadata for TUI display
    let edit_executor = move |input: serde_json::Value,
                              ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = FileEditTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let edit_rf = make_render_fns(FileEditTool::new());
    engine.register_tool_with_render("FileEdit".to_string(), edit_executor, edit_rf);
    // FileEdit tool - register backfill function (expand file_path for observers)
    engine.register_tool_backfill(
        "FileEdit".to_string(),
        |input: &mut serde_json::Value| {
            if let Some(fp) = input.get("file_path").and_then(|v| v.as_str()) {
                let expanded = crate::utils::path::expand_path(fp);
                if let Some(obj) = input.as_object_mut() {
                    obj.insert("file_path".to_string(), serde_json::json!(expanded));
                }
            }
        },
    );

    // Skill tool - register skills from examples/skills directory
    use std::path::Path;
    register_skills_from_dir(Path::new("examples/skills"));

    let skill_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SkillTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let skill_tool = SkillTool::new();
    let skill_rf = self::make_render_fns(skill_tool);
    engine.register_tool_with_render("Skill".to_string(), skill_executor, skill_rf);

    // Monitor tool
    let monitor_executor = move |input: serde_json::Value,
                                 ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = MonitorTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let monitor_tool = MonitorTool::new();
    let monitor_rf = self::make_render_fns(monitor_tool);
    engine.register_tool_with_render("Monitor".to_string(), monitor_executor, monitor_rf);

    // SendUserFile tool
    let send_user_file_executor = move |input: serde_json::Value,
                                        ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SendUserFileTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let send_user_file_tool = SendUserFileTool::new();
    let send_user_file_rf = self::make_render_fns(send_user_file_tool);
    engine.register_tool_with_render("send_user_file".to_string(), send_user_file_executor, send_user_file_rf);

    // WebBrowser tool
    let web_browser_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WebBrowserTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let web_browser_tool = WebBrowserTool::new();
    let web_browser_rf = self::make_render_fns(web_browser_tool);
    engine.register_tool_with_render("WebBrowser".to_string(), web_browser_executor, web_browser_rf);

    // WebFetch tool
    let web_fetch_executor = move |input: serde_json::Value,
                                   ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WebFetchTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let web_fetch_tool = WebFetchTool::new();
    let web_fetch_rf = self::make_render_fns(web_fetch_tool);
    engine.register_tool_with_render("WebFetch".to_string(), web_fetch_executor, web_fetch_rf);

    // WebSearch tool
    let web_search_executor = move |input: serde_json::Value,
                                    ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = WebSearchTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let web_search_tool = WebSearchTool::new();
    let web_search_rf = self::make_render_fns(web_search_tool);
    engine.register_tool_with_render("WebSearch".to_string(), web_search_executor, web_search_rf);

    // NotebookEdit tool
    let notebook_edit_executor = move |input: serde_json::Value,
                                       ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = NotebookEditTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let notebook_edit_tool = NotebookEditTool::new();
    let notebook_edit_rf = self::make_render_fns(notebook_edit_tool);
    engine.register_tool_with_render("NotebookEdit".to_string(), notebook_edit_executor, notebook_edit_rf);

    // TaskCreate tool
    let task_create_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskCreateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let task_create_tool = TaskCreateTool::new();
    let task_create_rf = self::make_render_fns(task_create_tool);
    engine.register_tool_with_render("TaskCreate".to_string(), task_create_executor, task_create_rf);

    // TaskList tool
    let task_list_executor = move |input: serde_json::Value,
                                   ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskListTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let task_list_tool = TaskListTool::new();
    let task_list_rf = self::make_render_fns(task_list_tool);
    engine.register_tool_with_render("TaskList".to_string(), task_list_executor, task_list_rf);

    // TaskUpdate tool
    let task_update_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskUpdateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let task_update_tool = TaskUpdateTool::new();
    let task_update_rf = self::make_render_fns(task_update_tool);
    engine.register_tool_with_render("TaskUpdate".to_string(), task_update_executor, task_update_rf);

    // TaskGet tool
    let task_get_executor = move |input: serde_json::Value,
                                  ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskGetTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let task_get_tool = TaskGetTool::new();
    let task_get_rf = self::make_render_fns(task_get_tool);
    engine.register_tool_with_render("TaskGet".to_string(), task_get_executor, task_get_rf);

    // TaskOutput tool
    let task_output_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TaskOutputTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let task_output_tool = TaskOutputTool::new();
    let task_output_rf = self::make_render_fns(task_output_tool);
    engine.register_tool_with_render("TaskOutput".to_string(), task_output_executor, task_output_rf);

    // TodoWrite tool
    let todo_write_executor = move |input: serde_json::Value,
                                    ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TodoWriteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let todo_write_tool = TodoWriteTool::new();
    let todo_write_rf = self::make_render_fns(todo_write_tool);
    engine.register_tool_with_render("TodoWrite".to_string(), todo_write_executor, todo_write_rf);

    // CronCreate tool
    let cron_create_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = CronCreateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let cron_create_tool = CronCreateTool::new();
    let cron_create_rf = self::make_render_fns(cron_create_tool);
    engine.register_tool_with_render("CronCreate".to_string(), cron_create_executor, cron_create_rf);

    // CronDelete tool
    let cron_delete_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = CronDeleteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let cron_delete_tool = CronDeleteTool::new();
    let cron_delete_rf = self::make_render_fns(cron_delete_tool);
    engine.register_tool_with_render("CronDelete".to_string(), cron_delete_executor, cron_delete_rf);

    // CronList tool
    let cron_list_executor = move |input: serde_json::Value,
                                   ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = CronListTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let cron_list_tool = CronListTool::new();
    let cron_list_rf = self::make_render_fns(cron_list_tool);
    engine.register_tool_with_render("CronList".to_string(), cron_list_executor, cron_list_rf);

    // Config tool
    let config_executor = move |input: serde_json::Value,
                                ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ConfigTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let config_tool = ConfigTool::new();
    let config_rf = self::make_render_fns(config_tool);
    engine.register_tool_with_render("Config".to_string(), config_executor, config_rf);

    // EnterWorktree tool
    let enter_worktree_executor = move |input: serde_json::Value,
                                        ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = EnterWorktreeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let enter_worktree_tool = EnterWorktreeTool::new();
    let enter_worktree_rf = self::make_render_fns(enter_worktree_tool);
    engine.register_tool_with_render("EnterWorktree".to_string(), enter_worktree_executor, enter_worktree_rf);

    // ExitWorktree tool
    let exit_worktree_executor = move |input: serde_json::Value,
                                       ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ExitWorktreeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let exit_worktree_tool = ExitWorktreeTool::new();
    let exit_worktree_rf = self::make_render_fns(exit_worktree_tool);
    engine.register_tool_with_render("ExitWorktree".to_string(), exit_worktree_executor, exit_worktree_rf);

    // EnterPlanMode tool
    let enter_plan_mode_executor = move |input: serde_json::Value,
                                         ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = EnterPlanModeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let enter_plan_mode_tool = EnterPlanModeTool::new();
    let enter_plan_mode_rf = self::make_render_fns(enter_plan_mode_tool);
    engine.register_tool_with_render("EnterPlanMode".to_string(), enter_plan_mode_executor, enter_plan_mode_rf);

    // ExitPlanMode tool
    let exit_plan_mode_executor = move |input: serde_json::Value,
                                        ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ExitPlanModeTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let exit_plan_mode_tool = ExitPlanModeTool::new();
    let exit_plan_mode_rf = self::make_render_fns(exit_plan_mode_tool);
    engine.register_tool_with_render("ExitPlanMode".to_string(), exit_plan_mode_executor, exit_plan_mode_rf);

    // AskUserQuestion tool
    let ask_user_question_executor = move |input: serde_json::Value,
                                           ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = AskUserQuestionTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let ask_user_question_tool = AskUserQuestionTool::new();
    let ask_user_question_rf = self::make_render_fns(ask_user_question_tool);
    engine.register_tool_with_render("AskUserQuestion".to_string(), ask_user_question_executor, ask_user_question_rf);

    // ToolSearch tool
    let tool_search_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ToolSearchTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let tool_search_tool = ToolSearchTool::new();
    let tool_search_rf = self::make_render_fns(tool_search_tool);
    engine.register_tool_with_render("ToolSearch".to_string(), tool_search_executor, tool_search_rf);

    // TeamCreate tool
    let team_create_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TeamCreateTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let team_create_tool = TeamCreateTool::new();
    let team_create_rf = self::make_render_fns(team_create_tool);
    engine.register_tool_with_render("TeamCreate".to_string(), team_create_executor, team_create_rf);

    // TeamDelete tool
    let team_delete_executor = move |input: serde_json::Value,
                                     ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = TeamDeleteTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let team_delete_tool = TeamDeleteTool::new();
    let team_delete_rf = self::make_render_fns(team_delete_tool);
    engine.register_tool_with_render("TeamDelete".to_string(), team_delete_executor, team_delete_rf);

    // SendMessage tool
    let send_message_executor = move |input: serde_json::Value,
                                      ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SendMessageTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let send_message_tool = SendMessageTool::new();
    let send_message_rf = self::make_render_fns(send_message_tool);
    engine.register_tool_with_render("SendMessage".to_string(), send_message_executor, send_message_rf);

    // Sleep tool - wait for a duration without holding a shell process
    let sleep_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = SleepTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let sleep_tool = SleepTool::new();
    let sleep_rf = self::make_render_fns(sleep_tool);
    engine.register_tool_with_render("Sleep".to_string(), sleep_executor, sleep_rf);

    // PowerShell tool - execute PowerShell commands
    let powershell_executor = move |input: serde_json::Value,
                                    ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = PowerShellTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let powershell_tool = PowerShellTool::new();
    let powershell_rf = self::make_render_fns(powershell_tool);
    engine.register_tool_with_render("PowerShell".to_string(), powershell_executor, powershell_rf);

    // LSP tool - code intelligence via Language Server Protocol
    let lsp_executor = move |input: serde_json::Value,
                             ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = LSPTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let lsp_tool = LSPTool::new();
    let lsp_rf = self::make_render_fns(lsp_tool);
    engine.register_tool_with_render("LSP".to_string(), lsp_executor, lsp_rf);

    // RemoteTrigger tool - manage remote agent triggers
    let remote_trigger_executor = move |input: serde_json::Value,
                                        ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = RemoteTriggerTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let remote_trigger_tool = RemoteTriggerTool::new();
    let remote_trigger_rf = self::make_render_fns(remote_trigger_tool);
    engine.register_tool_with_render("RemoteTrigger".to_string(), remote_trigger_executor, remote_trigger_rf);

    // ListMcpResourcesTool - list MCP server resources
    let list_mcp_resources_executor = move |input: serde_json::Value,
                                            ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ListMcpResourcesTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let list_mcp_resources_tool = ListMcpResourcesTool::new();
    let list_mcp_resources_rf = self::make_render_fns(list_mcp_resources_tool);
    engine.register_tool_with_render(
        "ListMcpResourcesTool".to_string(),
        list_mcp_resources_executor,
        list_mcp_resources_rf,
    );

    // ReadMcpResourceTool - read MCP resources
    let read_mcp_resource_executor = move |input: serde_json::Value,
                                           ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = ReadMcpResourceTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let read_mcp_resource_tool = ReadMcpResourceTool::new();
    let read_mcp_resource_rf = self::make_render_fns(read_mcp_resource_tool);
    engine.register_tool_with_render(
        "ReadMcpResourceTool".to_string(),
        read_mcp_resource_executor,
        read_mcp_resource_rf,
    );

    // BriefTool (SendUserMessage) — primary visible output channel
    let brief_executor = move |input: serde_json::Value,
                               ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
        let tool_clone = BriefTool::new();
        let cwd = ctx.cwd.clone();
        let abort_signal = ctx.abort_signal.clone();
        Box::pin(async move {
            let ctx2 = ToolContext {
                cwd,
                abort_signal: abort_signal.clone(),
            };
            tool_clone.execute(input, &ctx2).await
        })
    };
    let brief_tool = BriefTool::new();
    let brief_rf = self::make_render_fns(brief_tool);
    engine.register_tool_with_render("SendUserMessage".to_string(), brief_executor, brief_rf);

    // SyntheticOutputTool (StructuredOutput) — structured output enforcement
    let synthetic_output_executor =
        move |input: serde_json::Value, ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
            let tool_clone = SyntheticOutputTool::new();
            let cwd = ctx.cwd.clone();
            let abort_signal = ctx.abort_signal.clone();
            Box::pin(async move {
                let ctx2 = ToolContext {
                    cwd,
                    abort_signal: abort_signal.clone(),
                };
                tool_clone.execute(input, &ctx2).await
            })
        };
    let synthetic_output_tool = SyntheticOutputTool::new();
    let synthetic_output_rf = self::make_render_fns(synthetic_output_tool);
    engine.register_tool_with_render("StructuredOutput".to_string(), synthetic_output_executor, synthetic_output_rf);

    // MCPTool — generic MCP tool execution dispatcher
    let mcp_tool_executor =
        move |input: serde_json::Value, ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
            let tool_clone = McpTool::new();
            let cwd = ctx.cwd.clone();
            let abort_signal = ctx.abort_signal.clone();
            Box::pin(async move {
                let ctx2 = ToolContext {
                    cwd,
                    abort_signal: abort_signal.clone(),
                };
                tool_clone.execute(input, &ctx2).await
            })
        };
    let mcp_tool = McpTool::new();
    let mcp_tool_rf = self::make_render_fns(mcp_tool);
    engine.register_tool_with_render("MCPTool".to_string(), mcp_tool_executor, mcp_tool_rf);

    // McpAuthTool — authenticate MCP server via OAuth
    let mcp_auth_executor =
        move |input: serde_json::Value, ctx: &ToolContext|
          -> BoxFuture<Result<ToolResult, AgentError>> {
            let tool_clone = McpAuthTool::new();
            let cwd = ctx.cwd.clone();
            let abort_signal = ctx.abort_signal.clone();
            Box::pin(async move {
                let ctx2 = ToolContext {
                    cwd,
                    abort_signal: abort_signal.clone(),
                };
                tool_clone.execute(input, &ctx2).await
            })
        };
    let mcp_auth_tool = McpAuthTool::new();
    let mcp_auth_rf = self::make_render_fns(mcp_auth_tool);
    engine.register_tool_with_render("McpAuth".to_string(), mcp_auth_executor, mcp_auth_rf);
}

/// Subscriber info for fan-out event delivery
///
/// Thread-safe, Clone-able agent handle for tokio async usage.
/// All internal state is held behind a single `Arc<Mutex<>>` — cloning Agent
/// just increments the reference count. All public methods take `&self`.
///
/// # Sharing across tasks
///
/// ```rust,ignore
/// let agent = Agent::new("claude-sonnet-4-6");
///
/// // Clone into another task
/// let agent2 = agent.clone();
/// let handle = tokio::spawn(async move {
///     agent2.query("do work").await
/// });
///
/// // Subscribe from the original
/// let (mut sub, _guard) = agent.subscribe();
/// ```
#[derive(Clone)]
pub struct Agent {
    pub(crate) inner: std::sync::Arc<parking_lot::Mutex<AgentInner>>,
}

#[cfg(test)]
impl Agent {
    /// Test-only accessor for the inner agent state.
    pub(crate) fn inner_for_test(&self) -> &std::sync::Arc<parking_lot::Mutex<AgentInner>> {
        &self.inner
    }
}

pub(crate) struct AgentInner {
    model: String,
    api_key: Option<String>,
    base_url: Option<String>,
    cwd: String,
    system_prompt: Option<String>,
    max_turns: u32,
    max_budget_usd: Option<f64>,
    max_tokens: u32,
    fallback_model: Option<String>,
    pub(crate) thinking: Option<ThinkingConfig>,
    mcp_servers: Option<std::collections::HashMap<String, crate::mcp::McpServerConfig>>,
    tool_pool: Vec<ToolDefinition>,
    pub(crate) allowed_tools: Vec<String>,
    pub(crate) disallowed_tools: Vec<String>,
    #[cfg(test)]
    pub on_event: Option<std::sync::Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    #[cfg(not(test))]
    pub(crate) on_event: Option<std::sync::Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    session_id: String,
    abort_controller: std::sync::Arc<crate::utils::AbortController>,
    /// Persisted QueryEngine for multi-turn reuse (matches TypeScript pattern).
    /// Shared via Arc<parking_lot::RwLock<QueryEngine>> so spawned tasks from
    /// query() can access the same conversation state (messages, usage, turns).
    /// Write lock held for the duration of query(); read locks for get_messages().
    /// `None` until first query — lazily initialized.
    engine: Option<Arc<parking_lot::RwLock<QueryEngine>>>,
    /// Event broadcast channels for subscribe() callers
    broadcasters: EventBroadcasters,
}

impl Agent {
    /// Create a new agent with the given model name.
    ///
    /// The model defaults to the `AI_MODEL` environment variable, then
    /// `"claude-sonnet-4-6"`. All other config (API key, base URL, max turns,
    /// thinking) also defaults from environment.
    ///
    /// Chain builder methods to customize:
    /// ```ignore
    /// let agent = Agent::new("claude-sonnet-4-6")
    ///     .max_turns(20)
    ///     .system_prompt("You are a code reviewer.")
    ///     .thinking(ThinkingConfig::Enabled { budget_tokens: 4096 });
    /// ```
    ///
    /// Returns a [`Clone`]able handle — all internal state uses interior
    /// mutability, so [`Agent::query`] takes `&self` and the agent can be
    /// shared across async tasks via `Arc<Agent>`.
    pub fn new(model: &str) -> Self {
        let env_config = EnvConfig::load();
        let model = env_config.model.unwrap_or_else(|| model.to_string());
        let api_key = env_config.auth_token.clone();
        let base_url = env_config.base_url.clone();
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());
        let default_max_tokens =
            crate::utils::context::get_max_output_tokens_for_model(&model) as u32;

        Self {
            inner: std::sync::Arc::new(parking_lot::Mutex::new(AgentInner {
                model,
                api_key,
                base_url,
                cwd,
                system_prompt: None,
                max_turns: 10,
                max_budget_usd: None,
                max_tokens: default_max_tokens,
                fallback_model: None,
                thinking: None,
                mcp_servers: None,
                tool_pool: vec![],
                allowed_tools: vec![],
                disallowed_tools: vec![],
                on_event: None,
                session_id: uuid::Uuid::new_v4().to_string(),
                abort_controller: std::sync::Arc::new(
                    crate::utils::create_abort_controller_default(),
                ),
                engine: None,
                broadcasters: EventBroadcasters::new(),
            })),
        }
    }

    /// Configure the model name.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn model(mut self, model: &str) -> Self {
        self.inner.lock().model = model.to_string();
        self
    }

    /// Set the API key.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn api_key(mut self, api_key: &str) -> Self {
        self.inner.lock().api_key = Some(api_key.to_string());
        self
    }

    /// Set a custom base URL for the API.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn base_url(mut self, base_url: &str) -> Self {
        self.inner.lock().base_url = Some(base_url.to_string());
        self
    }

    /// Set the working directory.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn cwd(mut self, cwd: &str) -> Self {
        self.inner.lock().cwd = cwd.to_string();
        self
    }

    /// Set a custom system prompt.
    pub fn system_prompt(mut self, prompt: &str) -> Self {
        self.inner.lock().system_prompt = Some(prompt.to_string());
        self
    }

    /// Set the maximum number of turns.
    ///
    /// Triggers engine recreation if the engine is already initialized.
    pub fn max_turns(mut self, max_turns: u32) -> Self {
        self.inner.lock().max_turns = max_turns;
        self
    }

    /// Set the maximum budget in USD.
    pub fn max_budget_usd(mut self, budget: f64) -> Self {
        self.inner.lock().max_budget_usd = Some(budget);
        self
    }

    /// Set the maximum tokens for a single response.
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.inner.lock().max_tokens = max_tokens;
        self
    }

    /// Set the fallback model.
    pub fn fallback_model(mut self, model: &str) -> Self {
        self.inner.lock().fallback_model = Some(model.to_string());
        self
    }

    /// Set thinking configuration for extended thinking.
    pub fn thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.inner.lock().thinking = Some(thinking);
        self
    }

    /// Set tool definitions.
    pub fn tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.inner.lock().tool_pool = tools;
        self
    }

    /// Only allow specific tools by name.
    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.inner.lock().allowed_tools = tools;
        self
    }

    /// Explicitly disallow specific tools by name.
    pub fn disallowed_tools(mut self, tools: Vec<String>) -> Self {
        self.inner.lock().disallowed_tools = tools;
        self
    }

    /// Set MCP server configurations.
    pub fn mcp_servers(
        mut self,
        servers: std::collections::HashMap<String, crate::mcp::McpServerConfig>,
    ) -> Self {
        self.inner.lock().mcp_servers = Some(servers);
        self
    }

    /// Set an event callback for streaming agent events.
    ///
    /// Can be called at any time (before or after `query()`). Takes `self`
    /// so it can be updated between queries for dynamic event handling.
    ///
    /// For fully async event handling, prefer [`Agent::subscribe()`] with
    /// the [`EventSubscriber`] stream instead.
    pub fn on_event<F>(mut self, callback: F) -> Self
    where
        F: Fn(AgentEvent) + Send + Sync + 'static,
    {
        self.inner.lock().on_event = Some(std::sync::Arc::new(callback));
        self
    }

    /// Uses interior mutability — takes `&self` so the agent can be shared across tasks.
    /// Lazily creates the QueryEngine on first query.
    fn init_engine(&self) {
        let mut inner = self.inner.lock();
        if inner.engine.is_none() {
            let cwd = inner.cwd.clone();
            let allowed_tools = inner.allowed_tools.clone();
            let disallowed_tools = inner.disallowed_tools.clone();
            let tool_pool = inner.tool_pool.clone();
            let can_use_tool: Option<
                std::sync::Arc<dyn Fn(ToolDefinition, serde_json::Value) -> PermissionResult + Send + Sync>,
            > = if !allowed_tools.is_empty() || !disallowed_tools.is_empty() {
                Some(std::sync::Arc::new(
                    move |tool_def: ToolDefinition, _input: serde_json::Value| {
                        if !allowed_tools.is_empty() && !allowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is not in the allowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "allowed tools filter".to_string() },
                            ));
                        }
                        if disallowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is in the disallowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "disallowed tools filter".to_string() },
                            ));
                        }
                        PermissionResult::Allow(PermissionAllowDecision::default())
                    },
                ))
            } else {
                None
            };
            let config = QueryEngineConfig {
                cwd: cwd.clone(),
                model: inner.model.clone(),
                api_key: inner.api_key.clone(),
                base_url: inner.base_url.clone(),
                tools: tool_pool,
                system_prompt: None,
                max_turns: inner.max_turns,
                max_budget_usd: inner.max_budget_usd,
                max_tokens: inner.max_tokens,
                fallback_model: inner.fallback_model.clone(),
                user_context: std::collections::HashMap::new(),
                system_context: std::collections::HashMap::new(),
                can_use_tool,
                on_event: inner.on_event.clone(),
                thinking: inner.thinking.clone(),
                abort_controller: Some(inner.abort_controller.clone()),
                token_budget: None,
                agent_id: None,
                session_state: None,
                loaded_nested_memory_paths: std::collections::HashSet::new(),
                task_budget: None,
                orphaned_permission: None,
            };
            let mut engine = QueryEngine::new(config);
            register_all_tool_executors(&mut engine);

            // Register hooks from loaded skills
            let session_id = inner.session_id.clone();
            if let Ok(skills) = load_all_skills(&cwd) {
                let _set_app_state = Arc::new(|_: &dyn Fn(&mut serde_json::Value)| {})
                    as Arc<dyn Fn(&dyn Fn(&mut serde_json::Value)) + Send + Sync>;
                register_hooks_from_skills(_set_app_state, &session_id, &skills);
            }

            inner.engine = Some(Arc::new(parking_lot::RwLock::new(engine)));
        }
    }

    /// One-shot query — creates an agent, sends the prompt, and returns the text.
    ///
    /// Use this for single-turn interactions where you don't need conversation history
    /// or multi-turn reuse. For persistent agents, use `Agent::new()` + `.query()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let answer = Agent::prompt("claude-sonnet-4-6", "Explain quantum computing")
    ///     .await?;
    /// println!("{answer}");
    /// ```
    pub async fn prompt(model: &str, prompt: &str) -> Result<String, AgentError> {
        let agent = Self::new(model);
        let result = agent.query(prompt).await?;
        Ok(result.text)
    }

    /// Get the configured model name.
    pub fn get_model(&self) -> String {
        self.inner.lock().model.clone()
    }

    /// Get the session ID.
    pub fn get_session_id(&self) -> String {
        self.inner.lock().session_id.clone()
    }

    /// Get all messages in the conversation history.
    /// Delegates to the persisted QueryEngine which owns the message state
    /// (matches TypeScript: engine.mutableMessages).
    pub fn get_messages(&self) -> Vec<Message> {
        let engine_opt = {
            let inner = self.inner.lock();
            inner.engine.clone()
        };
        if let Some(engine) = engine_opt {
            let eng = engine.read();
            eng.get_messages()
        } else {
            Vec::new()
        }
    }

    /// Get all tools available to the agent
    pub fn get_tools(&self) -> Vec<ToolDefinition> {
        self.inner.lock().tool_pool.clone()
    }

    /// Set system prompt for the agent (interior mutability).
    pub fn set_system_prompt(&self, prompt: &str) {
        self.inner.lock().system_prompt = Some(prompt.to_string());
    }

    /// Set the working directory for the agent (interior mutability).
    pub fn set_cwd(&self, cwd: &str) {
        self.inner.lock().cwd = cwd.to_string();
    }


    /// Set thinking configuration for the agent (interior mutability).
    pub fn set_thinking(&self, thinking: Option<ThinkingConfig>) {
        self.inner.lock().thinking = thinking;
    }

    /// Set the model name at runtime (interior mutability).
    ///
    /// Changes the model for subsequent `query()` calls, matching TypeScript's
    /// `QueryEngine.setModel()`.
    pub fn set_model(&self, model: &str) {
        self.inner.lock().model = model.to_string();
    }

    /// Execute a tool directly.
    pub(crate) async fn execute_tool(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, AgentError> {
        // Clone all needed data before dropping the lock
        let (cwd, model, api_key, base_url, abort_controller, allowed_tools, disallowed_tools, on_event, thinking) = {
            let inner = self.inner.lock();
            (
                inner.cwd.clone(),
                inner.model.clone(),
                inner.api_key.clone(),
                inner.base_url.clone(),
                inner.abort_controller.clone(),
                inner.allowed_tools.clone(),
                inner.disallowed_tools.clone(),
                inner.on_event.clone(),
                inner.thinking.clone(),
            )
        };

        let mut engine = QueryEngine::new(QueryEngineConfig {
            cwd: cwd.clone(),
            model: model.clone(),
            api_key: api_key.clone(),
            base_url: base_url.clone(),
            tools: vec![],
            system_prompt: None,
            max_turns: 10,
            max_budget_usd: None,
            max_tokens: crate::utils::context::get_max_output_tokens_for_model(&model) as u32,
            fallback_model: None,
            user_context: std::collections::HashMap::new(),
            system_context: std::collections::HashMap::new(),
            can_use_tool: None,
            on_event: None,
            thinking: None,
            abort_controller: Some(abort_controller.clone()),
            token_budget: None,
            agent_id: None,
            session_state: None,
            loaded_nested_memory_paths: std::collections::HashSet::new(),
            task_budget: None,
            orphaned_permission: None,
        });

        // Snapshot parent context fields to thread into subagent closures
        let parent_can_use_tool: Option<
            std::sync::Arc<dyn Fn(ToolDefinition, serde_json::Value) -> PermissionResult + Send + Sync>,
        > = {
            if !allowed_tools.is_empty() || !disallowed_tools.is_empty() {
                Some(std::sync::Arc::new(
                    move |tool_def: ToolDefinition, _input: serde_json::Value| {
                        if !allowed_tools.is_empty() && !allowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is not in the allowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "allowed tools filter".to_string() },
                            ));
                        }
                        if disallowed_tools.contains(&tool_def.name) {
                            return PermissionResult::Deny(PermissionDenyDecision::new(
                                &format!("Tool '{}' is in the disallowed tools list", tool_def.name),
                                PermissionDecisionReason::Other { reason: "disallowed tools filter".to_string() },
                            ));
                        }
                        PermissionResult::Allow(PermissionAllowDecision::default())
                    },
                ))
            } else {
                None
            }
        };
        let parent_on_event = on_event;
        let parent_thinking = thinking;

        // Register all tool executors (including Bash, Read, Write, etc.)
        register_all_tool_executors(&mut engine);

        // Register Agent tool using the AgentTool struct
        {
            use crate::tools::agent::{AgentTool, AgentToolConfig, create_agent_tool_executor};
            let agent_tool = Arc::new(AgentTool::new(AgentToolConfig {
                cwd: cwd.clone(),
                api_key: api_key.clone(),
                base_url: base_url.clone(),
                model: model.clone(),
                tool_pool: crate::tools::get_all_base_tools(),
                abort_controller: abort_controller.clone(),
                can_use_tool: parent_can_use_tool,
                on_event: parent_on_event,
                thinking: parent_thinking,
                parent_messages: Vec::new(),
                parent_user_context: std::collections::HashMap::new(),
                parent_system_context: std::collections::HashMap::new(),
                parent_session_id: None,
            }));
            engine.register_tool(
                "Agent".to_string(),
                create_agent_tool_executor(agent_tool),
            );
        }
        let tool_call_id = uuid::Uuid::new_v4().to_string();
        engine.execute_tool(name, input, tool_call_id).await
    }

    /// Build the system prompt by combining AI.md, memory mechanics,
    /// base system prompt, and custom system prompt.
    fn build_system_prompt(&self, cwd: &std::path::Path) -> Option<String> {
        use crate::ai_md::load_ai_md;
        use crate::memdir::load_memory_prompt_sync;
        use crate::prompts::build_system_prompt as base_build_system_prompt;

        let inner = &*self.inner.lock();
        let ai_md_prompt = load_ai_md(cwd).ok().flatten();
        let memory_mechanics_prompt =
            if inner.system_prompt.is_some() && crate::memdir::has_auto_mem_path_override() {
                load_memory_prompt_sync()
            } else {
                None
            };
        let base_system_prompt = base_build_system_prompt();

        let system_prompt = match (
            &ai_md_prompt,
            &memory_mechanics_prompt,
            &inner.system_prompt,
        ) {
            (Some(ai_md), Some(mem), Some(custom)) => Some(format!(
                "{}\n\n{}\n\n{}\n\n{}",
                ai_md, mem, base_system_prompt, custom
            )),
            (Some(ai_md), Some(mem), None) => {
                Some(format!("{}\n\n{}\n\n{}", ai_md, mem, base_system_prompt))
            }
            (Some(ai_md), None, Some(custom)) => {
                Some(format!("{}\n\n{}\n\n{}", ai_md, base_system_prompt, custom))
            }
            (Some(ai_md), None, None) => Some(format!("{}\n\n{}", ai_md, base_system_prompt)),
            (None, Some(mem), Some(custom)) => {
                Some(format!("{}\n\n{}\n\n{}", mem, base_system_prompt, custom))
            }
            (None, Some(mem), None) => Some(format!("{}\n\n{}", mem, base_system_prompt)),
            (None, None, Some(custom)) => Some(format!("{}\n\n{}", base_system_prompt, custom)),
            (None, None, None) => Some(base_system_prompt),
        };
        system_prompt
    }

    /// Select the tools to use: all base tools if tool_pool is empty, otherwise the tool pool.
    /// Tools are sorted by name for prompt cache stability (matches TypeScript assembleToolPool).
    /// Applies deny rules to filter out disallowed MCP tools.
    fn select_tools(&self) -> Vec<ToolDefinition> {
        let inner = &*self.inner.lock();
        let tools = if inner.tool_pool.is_empty() {
            crate::tools::get_all_base_tools()
        } else {
            inner.tool_pool.clone()
        };
        let disallowed_tools = inner.disallowed_tools.clone();

        // Sort by name for prompt cache stability (built-in tools are already sorted)
        let mut sorted = tools;
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        // Deduplicate by name (first occurrence wins)
        let mut seen = std::collections::HashSet::new();
        sorted.retain(|t| seen.insert(t.name.clone()));

        // Apply deny rules (MCP server-prefix, wildcard, exact match)
        if !disallowed_tools.is_empty() {
            sorted = crate::tools::filter_tools_by_deny_rules(&sorted, &disallowed_tools);
        }
        sorted
    }

    /// Main query method - handles the full agent loop including tool use,
    /// streaming responses, and multi-turn interaction with the LLM.
    ///
    /// Reuses a persisted QueryEngine across calls so that conversation history,
    /// usage tracking, and tool state accumulate naturally (matches TypeScript pattern).
    ///
    /// Takes `&self` (interior mutability) — the agent can be shared across tasks.
    pub async fn query(&self, prompt: &str) -> Result<QueryResult, AgentError> {
        self.init_engine();

        // Clone all data from AgentInner before any .await (MutexGuard is not Send).
        // Single lock acquisition.
        let (cwd, on_event, thinking, abort_controller, engine, broadcasters) = {
            let inner = self.inner.lock();
            (
                inner.cwd.clone(),
                inner.on_event.clone(),
                inner.thinking.clone(),
                inner.abort_controller.clone(),
                Arc::clone(inner.engine.as_ref().unwrap()),
                inner.broadcasters.clone(),
            )
        };
        let cwd_path = std::path::Path::new(&cwd);

        let system_prompt = self.build_system_prompt(&cwd_path);
        let tools = self.select_tools();

        let start = std::time::Instant::now();
        let query_result: Result<
            (String, ExitReason, String, crate::types::TokenUsage, u32),
            AgentError,
        > = {
            let mut eng = engine.write();

            // Update per-query config
            eng.config.system_prompt = system_prompt;
            eng.config.tools = tools;
            // Wrap on_event to also broadcast to channel subscribers
            let on_event = {
                let cb = on_event;
                Some(Arc::new(move |event: crate::types::AgentEvent| {
                    if let Some(ref f) = cb {
                        f(event.clone());
                    }
                    broadcasters.broadcast(&event);
                }) as std::sync::Arc<dyn Fn(crate::types::AgentEvent) + Send + Sync>)
            };
            eng.config.on_event = on_event;
            eng.config.thinking = thinking;

            // Snapshot engine config values needed by the subagent closure
            let engine_tools = eng.config.tools.clone();
            let engine_model = eng.config.model.clone();
            let engine_api_key = eng.config.api_key.clone();
            let engine_base_url = eng.config.base_url.clone();
            let engine_cwd = eng.config.cwd.clone();
            let subagent_abort = abort_controller.clone();
            // Additional values for fork subagent path
            let engine_messages = eng.messages.clone();
            let engine_user_context = eng.config.user_context.clone();
            let engine_system_context = eng.config.system_context.clone();
            let engine_thinking = eng.config.thinking.clone();
            let engine_can_use_tool = eng.config.can_use_tool.clone();
            let engine_on_event = eng.config.on_event.clone();

            // Register Agent tool using the AgentTool struct (with fork support)
            {
                use crate::tools::agent::{AgentTool, AgentToolConfig, create_agent_tool_executor};
                let agent_tool = Arc::new(AgentTool::new(AgentToolConfig {
                    cwd: engine_cwd,
                    api_key: engine_api_key,
                    base_url: engine_base_url,
                    model: engine_model,
                    tool_pool: engine_tools,
                    abort_controller: subagent_abort,
                    can_use_tool: engine_can_use_tool,
                    on_event: engine_on_event,
                    thinking: engine_thinking,
                    parent_messages: engine_messages,
                    parent_user_context: engine_user_context,
                    parent_system_context: engine_system_context,
                    parent_session_id: None,
                }));
                eng.register_tool(
                    "Agent".to_string(),
                    create_agent_tool_executor(agent_tool),
                );
            }

            // Run the query — on error, broadcast Done with ModelError and preserve state
            match eng.submit_message(prompt).await {
                Ok(r) => Ok((
                    r.0,
                    r.1,
                    eng.config.model.clone(),
                    eng.get_usage(),
                    eng.get_turn_count(),
                )),
                Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    // Do NOT reset messages — preserve state for user replay (matches TypeScript)
                    // Only reset counters
                    eng.reset_counters();

                    // Detect image errors (ImageSizeError / ImageResizeError patterns)
                    // Matches TypeScript: error instanceof ImageSizeError || ImageResizeError
                    let is_image_error = crate::services::api::errors::is_media_size_error(&e.to_string())
                        && e.to_string().to_lowercase().contains("image");

                    if is_image_error {
                        // Write image error as API error message
                        eng.messages.push(crate::types::Message {
                            role: crate::types::MessageRole::Assistant,
                            content: e.to_string(),
                            is_api_error_message: Some(true),
                            error_details: Some(e.to_string()),
                            ..Default::default()
                        });
                        // Flush session storage
                        let _ = crate::utils::session_storage::flush_session_storage();
                        // Broadcast Done with ImageError exit reason
                        if let Some(ref cb) = eng.config.on_event {
                            cb(AgentEvent::Done {
                                result: QueryResult {
                                    text: String::new(),
                                    exit_reason: ExitReason::ImageError {
                                        error: e.to_string(),
                                    },
                                    usage: Default::default(),
                                    num_turns: 0,
                                    duration_ms,
                                },
                            });
                        }
                    } else {
                        // Write error as API error message in conversation (matches TypeScript)
                        let api_err = crate::services::api::errors::error_to_api_message(&e.to_string(), None);
                        eng.messages.push(crate::types::Message {
                            role: crate::types::MessageRole::Assistant,
                            content: api_err.content.clone().unwrap_or_default(),
                            is_api_error_message: Some(true),
                            error_details: api_err.error_details.clone(),
                            ..Default::default()
                        });
                        // Flush session storage before error result (matches TypeScript flushSessionStorage)
                        let _ = crate::utils::session_storage::flush_session_storage();
                        // Broadcast Done event so the TUI unblocks
                        if let Some(ref cb) = eng.config.on_event {
                            cb(AgentEvent::Done {
                                result: QueryResult {
                                    text: String::new(),
                                    exit_reason: ExitReason::ModelError {
                                        error: e.to_string(),
                                    },
                                    usage: Default::default(),
                                    num_turns: 0,
                                    duration_ms,
                                },
                            });
                        }
                    }
                    Err(e)
                }
            }
        }; // Lock released here

        let (response_text, exit_reason, current_model, usage, turns) = query_result?;

        // Track model in case it changed (for recreation detection)
        if current_model != self.get_model() {
            self.inner.lock().model = current_model;
        }

        Ok(QueryResult {
            text: response_text,
            usage: TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
                iterations: usage.iterations,
            },
            num_turns: turns,
            duration_ms: start.elapsed().as_millis() as u64,
            exit_reason,
        })
    }

    /// Reset the agent's conversation history, keeping configuration intact.
    ///
    /// Clears all messages, usage tracking, and turn count. This starts a fresh
    /// conversation while preserving model, API key, tools, and other settings.
    pub fn reset(&self) {
        let engine_opt = {
            let inner = self.inner.lock();
            inner.engine.clone()
        };
        if let Some(engine) = engine_opt {
            let mut eng = engine.write();
            eng.reset();
        }
    }

    /// Subscribe to agent events for the current and subsequent queries.
    ///
    /// Returns an [`EventSubscriber`] (implements [`futures_util::Stream`]) and a
    /// [`CancelGuard`]. Events flow to the subscriber until the guard is dropped.
    ///
    /// Takes `&self` (not `&mut self`) — you can subscribe from a shared reference,
    /// enabling decoupled TUI architectures where the subscriber is owned by a
    /// separate task from the agent.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let (mut sub, _guard) = agent.subscribe();
    /// tokio::pin!(sub);
    ///
    /// // Run query asynchronously
    /// tokio::spawn(async move {
    ///     agent.query("hello").await;
    /// });
    ///
    /// // Consume events
    /// while let Some(ev) = sub.next().await {
    ///     // render in TUI
    /// }
    /// ```
    pub fn subscribe(&self) -> (EventSubscriber, CancelGuard) {
        let inner = self.inner.lock();
        inner.broadcasters.subscribe()
    }

    /// Interrupt the agent loop. This aborts the current `query()` call,
    /// cancelling any in-flight API requests and tool execution.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let agent = Agent::new("claude-sonnet-4-6");
    ///
    /// tokio::spawn(async move {
    ///     agent.query("Do a lot of work").await.unwrap();
    /// });
    ///
    /// tokio::time::sleep(Duration::from_secs(5)).await;
    /// agent.interrupt(); // Cancel the running prompt
    /// ```
    pub fn interrupt(&self) {
        let inner = self.inner.lock();
        inner.abort_controller.abort(None);
    }

    /// Generate a short session recap summarizing the conversation so far.
    ///
    /// Produces a 1-3 sentence summary suitable for a "while you were away"
    /// card or CLI status display. Uses the small/fast model (Haiku) with
    /// the last 30 messages and optional session memory context.
    ///
    /// Returns `AwaySummaryResult` which indicates whether the LLM produced
    /// a summary, was aborted, or returned empty (no conversation history).
    ///
    /// # Example
    /// ```ignore
    /// let agent = Agent::new("claude-sonnet-4-6")
    ///     .api_key("sk-ant-...");
    /// agent.query("build a REST API").await.ok();
    /// // ... user steps away ...
    /// let recap = agent.recap().await;
    /// if let Some(summary) = recap.summary {
    ///     println!("※ {}", summary);
    /// }
    /// ```
    pub async fn recap(
        &self,
    ) -> crate::services::away_summary::AwaySummaryResult {
        let engine_opt = {
            let inner = self.inner.lock();
            inner.engine.clone()
        };
        let messages = if let Some(engine) = engine_opt.as_ref() {
            let eng = engine.read();
            eng.get_messages()
        } else {
            Vec::new()
        };
        let (api_key, abort_ctrl) = {
            let inner = self.inner.lock();
            (inner.api_key.clone(), inner.abort_controller.clone())
        };

        let api_key = match api_key {
            Some(k) if !k.is_empty() => k,
            _ => std::env::var("AI_AUTH_TOKEN")
                .or_else(|_| std::env::var("AI_API_KEY"))
                .unwrap_or_default(),
        };

        crate::services::away_summary::generate_away_summary(
            &messages,
            &api_key,
            abort_ctrl.signal().abort_flag(),
        )
        .await
    }
}

/// Build system prompt for subagent based on agent type
pub(crate) fn build_agent_system_prompt(
    agent_description: &str,
    agent_type: Option<&str>,
) -> String {
    let base_prompt = "You are an agent that helps users with software engineering tasks. Use the tools available to you to assist the user.\n\nComplete the task fully—don't gold-plate, but don't leave it half-done. When you complete the task, respond with a concise report covering what was done and any key findings.";

    match agent_type {
        Some("Explore") => {
            format!(
                "{}\n\nYou are an Explore agent. Your goal is to explore and understand the codebase thoroughly. Use search and read tools to investigate. Report your findings in detail.",
                base_prompt
            )
        }
        Some("Plan") => {
            format!(
                "{}\n\nYou are a Plan agent. Your goal is to plan and analyze tasks before execution. Break down complex tasks into steps. Provide a detailed plan.",
                base_prompt
            )
        }
        Some("Review") => {
            format!(
                "{}\n\nYou are a Review agent. Your goal is to review code and provide constructive feedback. Be thorough and focus on best practices.",
                base_prompt
            )
        }
        _ => {
            // General purpose agent
            format!("{}\n\nTask description: {}", base_prompt, agent_description)
        }
    }
}
