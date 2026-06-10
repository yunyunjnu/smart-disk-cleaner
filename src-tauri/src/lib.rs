mod commands;
mod events;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::ai::explain_file_with_ai,
            commands::ai::generate_ai_cleanup_plan,
            commands::ai::open_apps_and_features,
            commands::process::list_top_processes,
            commands::process::get_process_monitor_snapshot,
            commands::process::explain_process_with_ai,
            commands::process::ask_process_follow_up_with_ai,
            commands::process::terminate_process,
            commands::scan::start_scan,
            commands::scan::start_scan_v2,
            commands::scan::get_latest_scan_report,
            commands::scan::get_directory_overview,
            commands::scan::get_directory_overview_v2,
            commands::scan::get_app_overview,
            commands::scan::get_app_overview_v2,
            commands::scan::query_file_tree,
            commands::scan::query_file_tree_v2,
            commands::scan::query_directory_tree_v2,
            commands::scan::load_directory_children,
            commands::scan::cancel_scan,
            commands::scan::diagnose_path,
            commands::cleanup::execute_cleanup,
            commands::cleanup::get_operation_history,
            commands::cleanup::get_path_blockers,
            commands::migration::get_migration_advice,
            commands::migration::refine_migration_plan_with_ai,
            commands::migration::execute_migration_plan,
            commands::migration::get_migration_run_history,
            commands::migration::rollback_migration_run,
            commands::registry::list_registry_startup_entries,
            commands::registry::list_registry_path_issues,
            commands::registry::export_registry_backup,
            commands::registry::preview_registry_change,
            commands::registry::apply_registry_change,
            commands::registry::rollback_registry_change,
            commands::config::load_config,
            commands::config::save_config,
            commands::config::list_ai_models,
            commands::config::test_ai_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
