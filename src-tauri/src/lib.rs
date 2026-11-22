mod certificates;
mod commands;
mod config;
mod connection;
mod export;
mod files;
mod pdf_merge;
mod state;

use std::path::PathBuf;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // POLARISCAL_HOME points config and data at another folder, so test
            // and demo runs never touch the real database in %APPDATA%.
            let (config_dir, data_dir) = match std::env::var_os("POLARISCAL_HOME") {
                Some(home) => (PathBuf::from(&home), PathBuf::from(home)),
                None => (app.path().app_config_dir()?, app.path().app_data_dir()?),
            };
            let state =
                tauri::async_runtime::block_on(state::AppState::initialize(&config_dir, &data_dir));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::auth_status,
            commands::setup_admin,
            commands::sign_in,
            commands::sign_out,
            commands::lock_session,
            commands::unlock_session,
            commands::touch_session,
            commands::change_password,
            commands::list_users,
            commands::create_user,
            commands::update_user,
            commands::reset_password,
            commands::get_settings,
            commands::update_settings,
            commands::list_audit,
            commands::list_lookups,
            commands::create_lookup,
            commands::update_lookup,
            commands::delete_lookup,
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
            commands::list_attachments,
            commands::add_attachments,
            commands::open_attachment,
            commands::reveal_attachment,
            commands::set_attachment_archived,
            commands::check_attachments_folder,
            connection::get_connection_settings,
            connection::read_shared_settings,
            connection::test_connection,
            connection::save_connection,
            connection::copy_database,
            export::export_xlsx,
            certificates::get_certificate_template,
            certificates::update_certificate_template,
            certificates::list_certificates,
            certificates::get_certificate,
            certificates::create_certificate,
            certificates::read_image_file,
            certificates::list_certificate_signers,
            certificates::get_user_signature,
            certificates::set_user_signature,
            certificates::save_certificate_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PolarisCal");
}
