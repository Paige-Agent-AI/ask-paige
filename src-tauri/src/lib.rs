// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod ask;
mod broker;
mod gateway;
mod mcp;
mod secrets;
mod terminal;

use ask::AskState;
use broker::BrokerState;
use mcp::McpState;
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
    tauri::Builder::default()
        .manage(Mutex::new(BrokerState::new()))
        .manage(Mutex::new(TerminalState::new()))
        .manage(Mutex::new(AskState::new()))
        .manage(mcp_state)
        .invoke_handler(tauri::generate_handler![
            greet,
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
            ask::ask_cancel,
            ask::ask_receipts,
            mcp::mcp_status,
            mcp::mcp_add_stdio,
            mcp::mcp_add_remote,
            mcp::mcp_connect,
            mcp::mcp_disconnect,
            mcp::mcp_reconnect,
            mcp::mcp_revoke,
            mcp::mcp_call_tool,
            mcp::mcp_receipts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}