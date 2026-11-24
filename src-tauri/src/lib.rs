// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod ask;
mod continuation;
mod conversation_commands;
mod linear_lookup;
mod notion_lookup;
mod supabase_logs;
mod broker;
mod chats;
mod chat_images;
mod connected_tools;
mod gateway;
mod github_mcp;
mod github_skills;
mod mcp;
mod project;
mod project_access;
mod provider_oauth;
mod secrets;
mod scope;
mod skills;
mod terminal;
mod team;
mod workflows;
mod visual_qa;
#[cfg(windows)]
mod window_capture;
mod workbench_plan;
mod route_evidence;
mod web_search;
mod word_report;

use ask::AskState;
use broker::BrokerState;
use mcp::McpState;
use project::ProjectState;
use terminal::TerminalState;
use std::sync::Mutex;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mcp_state = Mutex::new(McpState::new());
    mcp::load_persisted_servers(&mcp_state);
    let project_state = Mutex::new(ProjectState::new());
    // Load the single dynamic scope registry before any broker/terminal state
    // exists: it is the root authority for every subsequent check.
    scope::init();
    tauri::Builder::default()
        .manage(Mutex::new(BrokerState::new()))
        .manage(Mutex::new(TerminalState::new()))
        .manage(Mutex::new(AskState::new()))
        .manage(mcp_state)
        .manage(project_state)
        .manage(github_skills::SkillInstallState::default())
        .setup(|app| {
            // Restore the selected chat's verified project before opening the workspace.
            let _ = chats::chat_select(app.handle().clone(), None);
            mcp::reconnect_saved_oauth(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            github_skills::skills_installed,
            github_skills::skills_search,
            github_skills::skills_catalog,
            github_skills::skills_prepare,
            github_skills::skills_install,
            broker::broker_status,
            broker::broker_operations,
            broker::broker_grant_root,
            broker::broker_revoke,
            broker::broker_list_files,
            broker::broker_read_file,
            broker::broker_propose_write,
            broker::broker_approve,
            broker::broker_deny,
            broker::broker_cancel,
            broker::broker_apply,
            broker::broker_reconcile,
            #[cfg(feature = "insecure-test-bypass")]
            broker::broker_test_force_readback_failure,
            terminal::terminal_execute,
            terminal::terminal_execute_approved,
            terminal::terminal_input,
            terminal::terminal_kill,
            terminal::terminal_list,
            terminal::terminal_receipts,
            terminal::terminal_output,
            ask::ask_status,
            ask::ask_probe,
            ask::ask_connect_openrouter,
            ask::ask_disconnect_openrouter,
            ask::ask_set_model_selection,
            ask::ask_send,
            ask::ask_resume,
            ask::ask_end_task,
            workflows::workflow_accept_artifact,
            workflows::workflow_read_word,
            visual_qa::workflow_read_visual,
            chats::chat_read_image,
            visual_qa::workflow_compare_visual,
            visual_qa::workflow_retry_visual_test,
            ask::ask_cancel,
            ask::ask_open_confirmation,
            ask::ask_receipts,
            ask::ask_connect_omniroute,
            ask::ask_disconnect_omniroute,
            mcp::mcp_status,
            mcp::mcp_add_stdio,
            mcp::mcp_add_remote,
            mcp::mcp_linear_oauth,
            mcp::mcp_supabase_oauth,
            mcp::mcp_notion_oauth,
            mcp::mcp_paige_oauth,
            provider_oauth::mcp_paige_operator_login,
            mcp::mcp_github_oauth,
            provider_oauth::mcp_oauth_cancel,
            mcp::mcp_connect,
            mcp::mcp_disconnect,
            mcp::mcp_reconnect,
            mcp::mcp_revoke,
            mcp::mcp_call_tool,
            mcp::mcp_receipts,
            project::project_grant,
            project::project_rename,
            project::project_remove,
            project::project_open_folder,
            project::project_pick_folder,
            project::project_revoke,
            project::project_list,
            project::project_status,
            project::git_files,
            project::git_log_cmd,
            project::git_diff_cmd,
            project::git_branches_cmd,
            project::git_worktrees_cmd,
            project::git_create_worktree_cmd,
            project::git_remove_worktree_cmd,
            project::git_create_branch_cmd,
            project::git_checkout_cmd,
            project::git_add_cmd,
            project::git_commit_cmd,
            scope::scope_status,
            scope::scope_set_mode,
            scope::scope_add_root,
            scope::scope_remove_root,
            scope::scope_revoke_all,
            chats::chat_list,
            chats::chat_search,
            chats::chat_new,
            chats::chat_select,
            chats::chat_pin_workspace,
            chats::chat_rename,
            chats::chat_archive,
            chats::chat_delete,
            chats::chat_export,
            team::team_roster,
            team::team_create,
            team::team_name_specialist,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
