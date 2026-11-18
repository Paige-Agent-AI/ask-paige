fn main() {
    let mut app_manifest = tauri_build::AppManifest::new();
    app_manifest = app_manifest.commands(&[
        "greet",
        "broker_status",
        "broker_grant_root",
        "broker_revoke",
        "broker_list_files",
        "broker_read_file",
        "broker_propose_write",
        "broker_approve",
        "broker_deny",
        "broker_cancel",
        "broker_apply",
        "broker_operations",
        "broker_reconcile",
        "broker_test_force_readback_failure",
    ]);
    let attributes = tauri_build::Attributes::new().app_manifest(app_manifest);
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}