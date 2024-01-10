use std::collections::HashMap;
use std::path::PathBuf;

use holochain_types::prelude::{ExternIO, SerializedBytes, Signal, UnsafeBytes, ZomeName};
use holochain_types::web_app::WebAppBundle;
use serde_json::Value;
use tauri::{AppHandle, Runtime, WindowBuilder, WindowUrl};
#[cfg(desktop)]
use tauri_plugin_cli::CliExt;
use tauri_plugin_holochain::HolochainExt;
use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_notification::*;

const NOTIFICATIONS_RECIPIENT_APP_ID: &'static str = "notifications_fcm_recipient";
const NOTIFICATIONS_PROVIDER_APP_ID: &'static str = "notifications_provider_fcm";
const FCM_PROJECT_ID: &'static str = "studio.darksoil.rostanga";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(
        tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Warn)
            // .clear_targets()
            // .target(Target::new(TargetKind::LogDir { file_name: None }))
            .build(),
    );

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_cli::init());
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_holochain::init(PathBuf::from("holochain")))
        .plugin(tauri_plugin_holochain_notification::init(
            FCM_PROJECT_ID.into(),
            NOTIFICATIONS_PROVIDER_APP_ID.into(),
            NOTIFICATIONS_RECIPIENT_APP_ID.into(),
        ))
        .setup(|app| {
            // #[cfg(desktop)]
            // {
            //     app.handle().plugin(tauri_plugin_cli::init())?;
            //     let args = app.cli().matches().expect("Can't get matches").args;
            //     if let Some(m) = args.get("profile") {
            //         if let Value::String(s) = m.value.clone() {
            //             subfolder = PathBuf::from(s);
            //         }
            //     }
            // }

            let h = app.handle();
            tauri::async_runtime::block_on(
                async move { install_initial_apps_if_necessary(h).await },
            )?;
            app.holochain().open_app(String::from("gather")).unwrap();

            #[cfg(mobile)]
            setup_notifications(app.handle())?;

            // TODO: remove all this
            let h = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut app_agent_websocket = h
                    .holochain()
                    .app_agent_websocket("gather".into())
                    .await
                    .expect("Failed to connect to holochain");

                let h = h.clone();

                app_agent_websocket.on_signal(move |signal| {
                    let h = h.clone();
                    tauri::async_runtime::block_on(async move {

                    use hc_zome_notifications_types::*;
                    let Signal::App {  signal , zome_name, .. } = signal else {
                        return ();
                    };

                    if zome_name.to_string() != "alerts" {
                        return ();
                    }

                    let Ok(alerts::Signal::LinkCreated {
                            action,
                            link_type}) = signal.into_inner().decode::<alerts::Signal>() else {
                        return ();
                    };
                    let holochain_types::prelude::Action::CreateLink(create_link) = action.hashed.content else {
                        return ();
                    };

                    let mut app_agent_websocket = h
                        .holochain()
                        .app_agent_websocket(NOTIFICATIONS_PROVIDER_APP_ID.into())
                        .await
                        .expect("Failed to connect to holochain");

                    app_agent_websocket
                        .call_zome(
                            "notifications_provider_fcm".into(), 
                            ZomeName::from("notifications_provider_fcm"), 
                            "notify_agent".into(), 
                            ExternIO::encode(NotifyAgentInput {
                                notification: SerializedBytes::from(
                                    UnsafeBytes::from(create_link.tag.0)
                                ),
                                agent: create_link.base_address
                                    .into_agent_pub_key()
                                    .expect("Could not convert to agent pubkey")
                    })
                    .expect("Could not encode notify agent input"))
                    .await.expect("Failed to notify agent");
                    });
                }).await.expect("Failed to set up on signal");
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn install_initial_apps_if_necessary<R: Runtime>(
    app_handle: &AppHandle<R>,
) -> anyhow::Result<()> {
    let mut admin_ws = app_handle.holochain().admin_websocket().await?;

    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| tauri_plugin_holochain::Error::ConductorApiError(err))?;

    println!("Installing apps");
    if let None = apps
        .iter()
        .find(|app| app.installed_app_id.eq(&String::from("gather")))
    {
        let gather_web_app_bundle =
            WebAppBundle::decode(include_bytes!("../../workdir/gather.webhapp"))
                .expect("Failed to decode gather webhapp");

        app_handle
            .holochain()
            .install_web_app(
                String::from("gather"),
                gather_web_app_bundle,
                HashMap::new(),
                None,
            )
            .await?;
    }
    Ok(())
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
