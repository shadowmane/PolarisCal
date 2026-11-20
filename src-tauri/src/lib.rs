mod commands;
mod config;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let state =
                tauri::async_runtime::block_on(state::AppState::initialize(&config_dir, &data_dir));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_lookups,
            commands::create_lookup,
            commands::update_lookup,
            commands::list_assets,
            commands::get_asset,
            commands::create_asset,
            commands::update_asset,
            commands::set_asset_archived,
            commands::list_calibrations,
            commands::get_calibration,
            commands::create_calibration,
            commands::update_calibration,
            commands::set_calibration_archived,
            commands::due_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PolarisCal");
}
