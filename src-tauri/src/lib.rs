use std::collections::HashMap;
use std::path::PathBuf;

use holochain_types::web_app::WebAppBundle;
use serde_json::Value;
use tauri::{AppHandle, Runtime, WindowBuilder, WindowUrl};
#[cfg(desktop)]
use tauri_plugin_cli::CliExt;
use tauri_plugin_holochain::HolochainExt;
use tauri_plugin_notification::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut initial_apps: HashMap<String, WebAppBundle> = HashMap::new();

    initial_apps.insert(
        String::from("gather"),
        WebAppBundle::decode(include_bytes!("../../gather.webhapp")).unwrap(),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let mut subfolder = PathBuf::from("holochain");

            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_cli::init())?;
            #[cfg(desktop)]
            if let Some(m) = app
                .cli()
                .matches()
                .expect("Can't get matches")
                .args
                .get("profile")
            {
                if let Value::String(s) = m.value.clone() {
                    subfolder = PathBuf::from(s);
                }
            }

            let config = tauri_plugin_holochain::TauriPluginHolochainConfig {
                initial_apps,
                subfolder,
            };

            app.handle().plugin(tauri_plugin_holochain::init(config))?;
            app.holochain().open_app(String::from("gather")).unwrap();

            #[cfg(mobile)]
            setup_notifications(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_notifications<R: Runtime>(
    app_handle: &AppHandle<R>,
) -> tauri_plugin_notification::Result<()> {
    let mut permissions_state = app_handle.notification().permission_state()?;
    if let PermissionState::Unknown = permissions_state {
        permissions_state = app_handle.notification().request_permission()?;
    }
    let h = app_handle.clone();

    if let PermissionState::Granted = permissions_state {
        // h.notification()
        //     .create_channel(tauri_plugin_notification::Channel::builder("test", "test").build())
        //     .expect("Failed to create channel");
        // let r = app.background_tasks().schedule_background_task(
        //     ScheduleBackgroundTaskRequest {
        //         label: String::from("hi"),
        //         interval: 1,
        //     },
        //     move || {
        //         h.notification()
        //             .builder()
        //             .channel_id("test")
        //             .title("Hey!")
        //             .show()
        //             .expect("Failed to send notification");
        //     },
        // )?;
    }
    Ok(())
}
