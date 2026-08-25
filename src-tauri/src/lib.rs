mod bookmarks;
mod commands;
mod diagnostics;
mod ftp_client;
mod remote_fs;
mod sftp_client;
mod ssh;
mod sync;
mod webdav_client;

use commands::AppState;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    tracing_subscriber::fmt::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let data_directory = app.path().app_data_dir().map_err(|error| error.to_string())?;
            let cache_directory = app.path().app_cache_dir().map_err(|error| error.to_string())?;
            diagnostics::migrate_legacy_data_directory(&data_directory)?;
            diagnostics::install_local_panic_reporter(&data_directory.join("diagnostics"));
            app.manage(Arc::new(
                AppState::new(data_directory, cache_directory).map_err(|error| error.to_string())?,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connection_connect,
            commands::connection_disconnect,
            commands::connection_list,
            commands::sftp_probe_host_key,
            commands::bookmarks_list,
            commands::bookmark_save,
            commands::bookmark_delete,
            commands::connection_history_list,
            commands::connection_history_record,
            commands::connection_history_clear,
            commands::transfer_history_list,
            commands::transfer_history_record,
            commands::transfer_history_clear,
            commands::sync_history_list,
            commands::sync_history_clear,
            commands::credential_load,
            commands::credential_save,
            commands::ssh_keys_list,
            commands::remote_list,
            commands::sync_preview,
            commands::sync_execute,
            commands::remote_create_directory,
            commands::remote_rename,
            commands::remote_delete,
            commands::remote_edit_open,
            commands::remote_edit_poll,
            commands::remote_edit_close,
            commands::drag_export_prepare,
            commands::drag_export_cleanup,
            commands::transfer_upload,
            commands::transfer_download,
            commands::local_path_info,
            commands::transfer_upload_directory,
            commands::transfer_pause,
            commands::transfer_resume,
            commands::transfer_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Harbor Transfer");
}
