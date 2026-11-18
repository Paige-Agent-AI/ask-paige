// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod broker;

use broker::BrokerState;
use std::sync::Mutex;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(BrokerState::new()))
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}