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
        if let Err(err) = tauri_plugin_holochain::launch().await {
            log::error!("Could not not launch holochain: {err:?}");
        }
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
    #[cfg(mobile)]
    {
        builder = builder.plugin(tauri_plugin_notification::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![launch_gather, is_android])
        .plugin(tauri_plugin_holochain::init(PathBuf::from("holochain")))
        // .plugin(tauri_plugin_notification::init())
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
                    Err(err) => {
                        if let Err(err) =
                            h2.emit("setup-error", format!("Failed to set up gather: {err:?}"))
                        {
                            log::error!("Failed to send setup-error:  {err:?}");
                        }
                    }
                }
            });

            if is_first_run()? {
                let mut window_builder = WindowBuilder::new(
                    app.handle(),
                    "Welcome",
                    WindowUrl::App("index.html".into()),
                )
                .title("röstånga");

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

    let mut initial_apps = initial_apps();

    let mut apps_hashes: BTreeMap<String, String> = BTreeMap::new();

    for (app_id, (hash, _)) in initial_apps.iter() {
        apps_hashes.insert(app_id.clone(), hash.clone());
    }
    if !is_first_run()? {
        let mut mock_initial: BTreeMap<String, String> = BTreeMap::new();
        mock_initial.insert(
            "gather".into(),
            "39c01032253455d18ae3f70b68edf6ee8332874d55b4b213ed2a022b105b54b1".into(),
        );
        mock_initial.insert(
            NOTIFICATIONS_PROVIDER_APP_ID.into(),
            "d85ff103b0f34918877ce5b71ea40da3950ad5d3d2c30210eb0c45fa12c37b15".into(),
        );
        mock_initial.insert(
            NOTIFICATIONS_RECIPIENT_APP_ID.into(),
            "6cd9abe0747bc9786786ef318a04586776526efe90a33fae924525709c69d99c".into(),
        );

        let apps = get_installed_apps().unwrap_or(mock_initial);

        // TODO: what if there is no setup

        for (app_id, current_hash) in apps {
            if let Some((new_hash, initial_app)) = initial_apps.remove(&app_id) {
                log::error!("Current hash {current_hash} newhash {new_hash}");
                if !current_hash.eq(&new_hash) {
                    // Update
                    match initial_app {
                        InitialApp::WebApp(web_app) => {
                            app.holochain()?.update_web_app(app_id, web_app).await?;
                        }
                        _ => {}
                    }
                }
            }
        }

        app.holochain()?.open_app(String::from("gather")).await?;
    } else {
        let installed_apps = install_initial_apps_if_necessary(&app, initial_apps).await?;
        log::info!("Installed apps: {installed_apps:?}");
    }
    save_installed_apps(apps_hashes)?;

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

    Ok(())
}

pub enum InitialApp {
    App(AppBundle),
    WebApp(WebAppBundle),
}

fn initial_apps() -> BTreeMap<String, (String, InitialApp)> {
    let mut apps: BTreeMap<String, (String, InitialApp)> = BTreeMap::new();
    let (h1, provider_app) = provider_fcm_app_bundle();
    apps.insert(
        NOTIFICATIONS_PROVIDER_APP_ID.into(),
        (h1, InitialApp::App(provider_app)),
    );
    let (h2, recipient_app) = provider_fcm_recipient_app_bundle();
    apps.insert(
        NOTIFICATIONS_RECIPIENT_APP_ID.into(),
        (h2, InitialApp::App(recipient_app)),
    );
    let bytes = include_bytes!("../../workdir/gather.webhapp");
    let hash = sha256::digest(bytes);
    let gather_web_app_bundle =
        WebAppBundle::decode(bytes).expect("Failed to decode gather webhapp");

    apps.insert(
        String::from("gather"),
        (hash, InitialApp::WebApp(gather_web_app_bundle)),
    );

    apps
}

pub async fn install_initial_apps_if_necessary<R: Runtime>(
    app_handle: &AppHandle<R>,
    apps: BTreeMap<String, (String, InitialApp)>,
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
                (_, InitialApp::App(bundle)) => {
                    app_handle
                        .holochain()?
                        .install_app(app_id, bundle, HashMap::new(), None)
                        .await
                }
                (_, InitialApp::WebApp(bundle)) => {
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

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    window.close()?;

    Ok(())
}

fn is_first_run() -> anyhow::Result<bool> {
    Ok(!setup_file_path()?.exists())
}
fn setup_file_path() -> anyhow::Result<PathBuf> {
    let root = app_dirs2::app_root(
        app_dirs2::AppDataType::UserData,
        &app_dirs2::AppInfo {
            name: "studio.darksoil.rostanga",
            author: "darksoil.studio",
        },
    )?;

    Ok(root.join("setup"))

    //    app.path()
    //        .app_data_dir()
    //        .expect("Failed to get data dir")
    //        .join("setup")
}
use std::io::Write;
fn save_installed_apps(installed_apps: BTreeMap<String, String>) -> anyhow::Result<()> {
    let mut file = std::fs::File::create(setup_file_path()?)?;

    let data = serde_json::to_string(&installed_apps)?;

    file.write(&data.as_bytes())?;

    Ok(())
}
fn get_installed_apps() -> anyhow::Result<BTreeMap<String, String>> {
    let s = std::fs::read_to_string(setup_file_path()?)?;

    let apps: BTreeMap<String, String> = serde_json::from_str(s.as_str())?;

    Ok(apps)
}
