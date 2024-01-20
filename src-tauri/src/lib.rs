use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use holochain_client::AppInfo;
use holochain_types::prelude::{
    AppBundle, ExternIO, SerializedBytes, Signal, UnsafeBytes, ZomeName,
};
use holochain_types::web_app::WebAppBundle;
use tauri::{AppHandle, Manager, Runtime, Window, WindowBuilder, WindowUrl};
#[cfg(desktop)]
use tauri_plugin_cli::CliExt;
use tauri_plugin_holochain::{setup_holochain, HolochainExt};
use tauri_plugin_holochain_notification::{
    provider_fcm_app_bundle, provider_fcm_recipient_app_bundle, setup_notifications,
};
use tauri_plugin_notification::*;

const NOTIFICATIONS_RECIPIENT_APP_ID: &'static str = "notifications_fcm_recipient";
const NOTIFICATIONS_PROVIDER_APP_ID: &'static str = "notifications_provider_fcm";
const FCM_PROJECT_ID: &'static str = "rostanga-ce319";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::async_runtime::spawn(async {
        println!("Launching holochain as early as possible");
        log::info!("Launching holochain as early as possible");
        tauri_plugin_holochain::launch()
            .await
            .expect("Could not launch holochain");
    });

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
        .invoke_handler(tauri::generate_handler![launch_gather, is_android])
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_holochain::init(PathBuf::from("holochain")))
        // .plugin(tauri_plugin_holochain_notification::init())
        .setup(|app| {
            log::info!("Start tauri setup");

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
            let h2 = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                match setup(h).await {
                    Ok(_) => {}
                    Err(err) => h2
                        .emit("setup-error", format!("Failed to set up gather: {err:?}"))
                        .expect("Failed to send gather-setup-error"),
                }
            });

            if is_first_run(app.handle()) {
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
    setup_holochain(app.clone()).await?;
    log::info!("Successfully set up holochain");

    if !is_first_run(&app) {
        app.holochain()?.open_app(String::from("gather")).await?;
    }

    let installed_apps = install_initial_apps_if_necessary(&app, initial_apps()).await?;
    log::info!("Installed apps: {installed_apps:?}");

    setup_notifications(
        app.clone(),
        FCM_PROJECT_ID.into(),
        NOTIFICATIONS_PROVIDER_APP_ID.into(),
        NOTIFICATIONS_RECIPIENT_APP_ID.into(),
    )
    .await?;

    // TODO: remove all this
    let mut app_agent_websocket: holochain_client::AppAgentWebsocket = app
        .holochain()?
        .app_agent_websocket("gather".into())
        .await?;

    let h = app.clone();
    app_agent_websocket
        .on_signal(move |signal| {
            let h = h.clone();
            tauri::async_runtime::block_on(async move {
                use hc_zome_notifications_types::*;

                let Signal::App {
                        signal, zome_name, cell_id
                    } = signal
                    else {
                        return ();
                    };

                if zome_name.to_string() != "alerts" {
                    return ();
                }

                let Ok(alerts::Signal::LinkCreated { action, .. }) =
                    signal.into_inner().decode::<alerts::Signal>() else {
                    return ();
                };
                let holochain_types::prelude::Action::CreateLink(create_link) =
                    action.hashed.content
                    else {
                    return ();
                };

                let mut app_agent_websocket = h
                    .holochain()
                    .expect("Holochain was not initialized yet")
                    .app_agent_websocket(NOTIFICATIONS_PROVIDER_APP_ID.into())
                    .await
                    .expect("Failed to connect to holochain");

                let hrl = hrl::Hrl {
                    dna_hash: cell_id.dna_hash().clone(),
                    resource_hash: holochain_types::prelude::AnyDhtHash::from(action.hashed.hash),
                };

                app_agent_websocket
                    .call_zome(
                        "notifications_provider_fcm".into(),
                        ZomeName::from("notifications_provider_fcm"),
                        "notify_agent".into(),
                        ExternIO::encode(NotifyAgentInput {
                            notification: SerializedBytes::try_from(hrl)
                                .expect("Could not encode hrl"),
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

    create_setup_file(&app);

    Ok(())
}

pub enum InitialApp {
    App(AppBundle),
    WebApp(WebAppBundle),
}

fn initial_apps() -> BTreeMap<String, InitialApp> {
    let mut apps: BTreeMap<String, InitialApp> = BTreeMap::new();
    apps.insert(
        NOTIFICATIONS_PROVIDER_APP_ID.into(),
        InitialApp::App(provider_fcm_app_bundle()),
    );
    apps.insert(
        NOTIFICATIONS_RECIPIENT_APP_ID.into(),
        InitialApp::App(provider_fcm_recipient_app_bundle()),
    );
    let gather_web_app_bundle =
        WebAppBundle::decode(include_bytes!("../../workdir/gather.webhapp"))
            .expect("Failed to decode gather webhapp");
    apps.insert(
        String::from("gather"),
        InitialApp::WebApp(gather_web_app_bundle),
    );

    apps
}

pub async fn install_initial_apps_if_necessary<R: Runtime>(
    app_handle: &AppHandle<R>,
    apps: BTreeMap<String, InitialApp>,
) -> anyhow::Result<Vec<AppInfo>> {
    let mut admin_ws = app_handle.holochain()?.admin_websocket().await?;

    let installed_apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| tauri_plugin_holochain::Error::ConductorApiError(err))?;

    let mut new_apps: Vec<AppInfo> = Vec::new();

    for (app_id, initial_app) in apps {
        if installed_apps
            .iter()
            .find(|app| app.installed_app_id.eq(&app_id))
            .is_none()
        {
            let app_info = match initial_app {
                InitialApp::App(bundle) => {
                    app_handle
                        .holochain()?
                        .install_app(app_id, bundle, HashMap::new(), None)
                        .await
                }
                InitialApp::WebApp(bundle) => {
                    app_handle
                        .holochain()?
                        .install_web_app(app_id, bundle, HashMap::new(), None)
                        .await
                }
            }?;
            new_apps.push(app_info);
        }
    }

    // let installed_apps = futures::future::join_all(
    //     apps.into_iter()
    //         .filter(|(app_id, _)| {
    //             installed_apps
    //                 .iter()
    //                 .find(|app| app.installed_app_id.eq(app_id))
    //                 .is_none()
    //         })
    //         .map(|(app_id, initial_app)| async {
    //             match initial_app {
    //                 InitialApp::App(bundle) => Ok(app_handle
    //                     .holochain()?
    //                     .install_app(app_id, bundle, HashMap::new(), None)
    //                     .await?),
    //                 InitialApp::WebApp(bundle) => Ok(app_handle
    //                     .holochain()?
    //                     .install_web_app(app_id, bundle, HashMap::new(), None)
    //                     .await?),
    //             }
    //         }),
    // )
    // .await
    // .into_iter()
    // .collect::<tauri_plugin_holochain::Result<Vec<AppInfo>>>()?;

    Ok(new_apps)
}
// async fn install_initial_apps_if_necessary<R: Runtime>(
//     app_handle: &AppHandle<R>,
// ) -> anyhow::Result<Option<AppInfo>> {
//     let mut admin_ws = app_handle.holochain()?.admin_websocket().await?;

//     let apps = admin_ws
//         .list_apps(None)
//         .await
//         .map_err(|err| tauri_plugin_holochain::Error::ConductorApiError(err))?;

//     if let None = apps
//         .iter()
//         .find(|app| app.installed_app_id.eq(&String::from("gather")))
//     {
//         log::info!("Installing apps: gather");

//         let app_info = app_handle
//             .holochain()?
//             .install_web_app(
//                 String::from("gather"),
//                 gather_web_app_bundle,
//                 HashMap::new(),
//                 None,
//             )
//             .await?;

//         return Ok(Some(app_info));
//     }
//     Ok(None)
// }

#[tauri::command]
pub(crate) fn is_android() -> bool {
    cfg!(target_os = "android")
}

#[tauri::command]
pub(crate) async fn launch_gather(
    app: AppHandle,
    window: Window,
) -> tauri_plugin_holochain::Result<()> {
    log::info!("Launching gather");

    #[cfg(target_os = "android")]
    app.exit(0); // TODO: remove this

    app.holochain()?.open_app(String::from("gather")).await?;

    #[cfg(desktop)]
    window.close()?;

    Ok(())
}

fn is_first_run<R: Runtime>(app: &AppHandle<R>) -> bool {
    !setup_file_path(app).exists()
}
fn setup_file_path<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    app_dirs2::app_root(
        app_dirs2::AppDataType::UserData,
        &app_dirs2::AppInfo {
            name: "studio.darksoil.rostanga",
            author: "darksoil.studio",
        },
    )
    .expect("Can't get app dir")
    .join("setup")

//    app.path()
//        .app_data_dir()
//        .expect("Failed to get data dir")
//        .join("setup")
}
use std::io::Write;
fn create_setup_file<R: Runtime>(app: &AppHandle<R>) {
    let mut file =
        std::fs::File::create(setup_file_path(app)).expect("Failed to create setup file");
    file.write_all(b"Hello, world!")
        .expect("Failed to create setup file");
}
