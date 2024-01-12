use std::collections::HashMap;
use std::path::PathBuf;

use holochain_client::AppInfo;
use holochain_types::prelude::{ExternIO, SerializedBytes, Signal, UnsafeBytes, ZomeName};
use holochain_types::web_app::WebAppBundle;
use tauri::{AppHandle, Manager, Runtime, Window, WindowBuilder, WindowUrl};
#[cfg(desktop)]
use tauri_plugin_cli::CliExt;
use tauri_plugin_holochain::HolochainExt;
use tauri_plugin_notification::*;

const NOTIFICATIONS_RECIPIENT_APP_ID: &'static str = "notifications_fcm_recipient";
const NOTIFICATIONS_PROVIDER_APP_ID: &'static str = "notifications_provider_fcm";
const FCM_PROJECT_ID: &'static str = "studio.darksoil.rostanga";

#[tauri::command]
pub(crate) fn launch_gather(app: AppHandle, window: Window) -> tauri_plugin_holochain::Result<()> {
    #[cfg(desktop)]
    window.close()?;

    app.holochain().open_app(String::from("gather"))?;

    Ok(())
}

fn is_first_run() -> bool {
    let app_data_dir = app_dirs2::app_root(
        app_dirs2::AppDataType::UserData,
        &app_dirs2::AppInfo {
            name: "studio.darksoil.rostanga",
            author: "darksoil.studio",
        },
    )
    .expect("Can't get app dir");
    !app_data_dir.join("setup").exists()
}
use std::io::Write;
fn create_setup_file() {
    let app_data_dir = app_dirs2::app_root(
        app_dirs2::AppDataType::UserData,
        &app_dirs2::AppInfo {
            name: "studio.darksoil.rostanga",
            author: "darksoil.studio",
        },
    )
    .expect("Can't get app dir");

    let mut file =
        std::fs::File::create(app_data_dir.join("setup")).expect("Failed to create setup file");
    file.write_all(b"Hello, world!")
        .expect("Failed to create setup file");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(
        tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            // .clear_targets()
            // .target(Target::new(TargetKind::LogDir { file_name: None }))
            .build(),
    );

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_cli::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![launch_gather,])
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

            //            #[cfg(mobile)]
            //            setup_notifications(app.handle())?;

            let h = app.handle().clone();
            app.handle().listen_global("holochain-ready", move |_| {
                let h = h.clone();
                let h2 = h.clone();
                let h3 = h.clone();

                tauri::async_runtime::spawn(async move {
                    match setup(h).await {
                        Ok(_) => h2
                            .emit("gather-setup-complete", ())
                            .expect("Failed to send gather-setup-complete"),
                        Err(err) => h2
                            .emit("setup-error", format!("Failed to set up gather: {err:?}"))
                            .expect("Failed to send gather-setup-error"),
                    }
                });


                #[cfg(mobile)]
                setup_notifications(&h3).expect("Failed to setup notifications");
            });

            if is_first_run() {
                let mut window_builder = WindowBuilder::new(
                    app.handle(),
                    "Welcome",
                    WindowUrl::App("index.html".into()),
                );

                #[cfg(desktop)]
                {
                    window_builder = window_builder.min_inner_size(1000.0, 800.0);
                }
                let window = window_builder.build()?;
            }
            log::info!("Finishing setup");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn setup<R: Runtime>(app: AppHandle<R>) -> anyhow::Result<()> {
    if let None = install_initial_apps_if_necessary(&app).await? {
        // Gather is already installed, skipping splashscreen
        let mut app_agent_websocket: holochain_client::AppAgentWebsocket = app.holochain().app_agent_websocket("gather".into()).await?;
        let r = app_agent_websocket.call_zome("gather".into(), "gather".into(), "entry_defs".into(), ExternIO::encode(()).unwrap()).await;
        log::warn!("Result {r:?}");

        app.holochain().open_app(String::from("gather"))?;
    }

    // TODO: remove all this
    let mut app_agent_websocket: holochain_client::AppAgentWebsocket = app.holochain().app_agent_websocket("gather".into()).await?;
    //let r = app_agent_websocket.call_zome("gather".into(), "gather".into(), "entry_defs".into(), ExternIO::encode(()).unwrap()).await;


    let h = app.clone();

    app_agent_websocket
        .on_signal(move |signal| {
            let h = h.clone();
            tauri::async_runtime::spawn(async move {
                use hc_zome_notifications_types::*;

                let Signal::App {
                    signal, zome_name, ..
                } = signal
                else {
                    return ();
                };

                if zome_name.to_string() != "alerts" {
                    return ();
                }

                let Ok(alerts::Signal::LinkCreated { action, link_type }) =
                    signal.into_inner().decode::<alerts::Signal>()
                else {
                    return ();
                };
                let holochain_types::prelude::Action::CreateLink(create_link) =
                    action.hashed.content
                else {
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
                            notification: SerializedBytes::from(UnsafeBytes::from(
                                create_link.tag.0,
                            )),
                            agent: create_link
                                .base_address
                                .into_agent_pub_key()
                                .expect("Could not convert to agent pubkey"),
                        })
                        .expect("Could not encode notify agent input"),
                    )
                    .await
                    .expect("Failed to notify agent");
            });
        })
        .await?;

    create_setup_file();

    Ok(())
}

async fn install_initial_apps_if_necessary<R: Runtime>(
    app_handle: &AppHandle<R>,
) -> anyhow::Result<Option<AppInfo>> {
    let mut admin_ws = app_handle.holochain().admin_websocket().await?;

    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| tauri_plugin_holochain::Error::ConductorApiError(err))?;

    log::info!("Installing apps");
    if let None = apps
        .iter()
        .find(|app| app.installed_app_id.eq(&String::from("gather")))
    {
        let gather_web_app_bundle =
            WebAppBundle::decode(include_bytes!("../../workdir/gather.webhapp"))
                .expect("Failed to decode gather webhapp");

        let app_info = app_handle
            .holochain()
            .install_web_app(
                String::from("gather"),
                gather_web_app_bundle,
                HashMap::new(),
                None,
            )
            .await?;

        return Ok(Some(app_info));
    }
    Ok(None)
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
