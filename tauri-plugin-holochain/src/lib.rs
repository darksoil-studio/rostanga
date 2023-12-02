use std::{collections::HashMap, path::PathBuf, sync::Mutex, time::Duration};

use commands::install_web_app::install_web_app;
use filesystem::FileSystem;
use http_server::{pong_iframe, read_asset};
use hyper::StatusCode;
use launch::launch;
use serde::{Deserialize, Serialize};
use tauri::{
    http::response,
    plugin::{Builder, TauriPlugin},
    scope::ipc::RemoteDomainAccessScope,
    AppHandle, Manager, Runtime, WindowBuilder, WindowUrl,
};

use holochain::conductor::ConductorHandle;
use holochain_client::AdminWebsocket;
use holochain_keystore::MetaLairClient;
use holochain_types::web_app::WebAppBundle;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod config;
mod error;
mod filesystem;
mod http_server;
mod launch;

pub use error::{Error, Result};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HolochainRuntimeInfo {
    http_server_port: u16,
    app_port: u16,
    admin_port: u16,
}

/// Access to the push-notifications APIs.
pub struct HolochainPlugin<R: Runtime> {
    pub app_handle: AppHandle<R>,
    pub filesystem: FileSystem,
    pub runtime_info: HolochainRuntimeInfo,
    pub lair_client: MetaLairClient,
}

impl<R: Runtime> HolochainPlugin<R> {
    pub fn open_app(&self, app_id: String) -> Result<()> {
        println!("Opening app {}", app_id);

        let app_id_env_command = format!(r#"window.__APP_ID__ = "{}";"#, app_id);

        let mut window_builder = WindowBuilder::new(
            &self.app_handle,
            app_id.clone(),
            // WindowUrl::App(PathBuf::from("index.html")),
            WindowUrl::External(
                url::Url::parse(
                    format!("http://localhost:{}", self.runtime_info.http_server_port).as_str(),
                )
                .expect("Cannot parse localhost url"),
            ),
        )
        .initialization_script(app_id_env_command.as_str());

        #[cfg(desktop)]
        {
            window_builder = window_builder.min_inner_size(1000.0, 800.0);
        }
        let window = window_builder.build()?;

        self.app_handle.ipc_scope().configure_remote_access(
            RemoteDomainAccessScope::new("localhost")
                .add_window(window.label())
                .add_plugin("holochain"),
        );

        println!("Opened app {}", app_id);
        Ok(())
    }
}

// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the holochain APIs.
pub trait HolochainExt<R: Runtime> {
    fn holochain(&self) -> &HolochainPlugin<R>;
}

impl<R: Runtime, T: Manager<R>> crate::HolochainExt<R> for T {
    fn holochain(&self) -> &HolochainPlugin<R> {
        self.state::<HolochainPlugin<R>>().inner()
    }
}

pub struct TauriPluginHolochainConfig {
    pub initial_apps: HashMap<String, WebAppBundle>,
    pub subfolder: PathBuf,
}

/// Initializes the plugin.
pub fn init<R: Runtime>(config: TauriPluginHolochainConfig) -> TauriPlugin<R> {
    Builder::new("holochain")
        .invoke_handler(tauri::generate_handler![
            commands::sign_zome_call::sign_zome_call,
            commands::get_locales::get_locales,
            commands::get_runtime_info::get_runtime_info
        ])
        .register_uri_scheme_protocol("happ", |app_handle, request| {
            if request.uri().to_string().starts_with("happ://ping") {
                return response::Builder::new()
                    .status(StatusCode::ACCEPTED)
                    .header("Content-Type", "text/html;charset=utf-8")
                    .body(pong_iframe().as_bytes().to_vec())
                    .unwrap();
            }
            // prepare our response
            tauri::async_runtime::block_on(async move {
                // let mutex = app_handle.state::<Mutex<AdminWebsocket>>();
                // let mut admin_ws = mutex.lock().await;

                let uri_without_protocol = request
                    .uri()
                    .to_string()
                    .split("://")
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .get(1)
                    .unwrap()
                    .clone();
                let uri_without_querystring: String = uri_without_protocol
                    .split("?")
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .get(0)
                    .unwrap()
                    .clone();
                let uri_components: Vec<String> = uri_without_querystring
                    .split("/")
                    .map(|s| s.to_string())
                    .collect();
                let lowercase_app_id = uri_components.get(0).unwrap();
                let mut asset_file = PathBuf::new();
                for i in 1..uri_components.len() {
                    asset_file = asset_file.join(uri_components[i].clone());
                }

                let r = match read_asset(
                    &app_handle.holochain().filesystem,
                    lowercase_app_id,
                    asset_file.as_os_str().to_str().unwrap().to_string(),
                )
                .await
                {
                    Ok(Some((asset, mime_type))) => {
                        println!("Got asset for app with id: {}", lowercase_app_id);
                        let mut response = response::Builder::new().status(StatusCode::ACCEPTED);
                        if let Some(mime_type) = mime_type {
                            response = response
                                .header("Content-Type", format!("{};charset=utf-8", mime_type))
                        } else {
                            response = response.header("Content-Type", "charset=utf-8")
                        }

                        return response.body(asset).unwrap();
                    }
                    Ok(None) => response::Builder::new()
                        .status(StatusCode::NOT_FOUND)
                        .body(vec![])
                        .unwrap(),
                    Err(e) => response::Builder::new()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(format!("{:?}", e).as_bytes().to_vec())
                        .unwrap(),
                };

                // admin_ws.close();
                r
            })
        })
        .setup(move |app: &AppHandle<R>, api| {
            let filesystem = FileSystem::new(app, &config.subfolder)?;
            let admin_port = portpicker::pick_unused_port().expect("No ports free");
            let app_port = portpicker::pick_unused_port().expect("No ports free");
            let http_server_port = portpicker::pick_unused_port().expect("No ports free");
            let (lair_client, admin_ws) = tauri::async_runtime::block_on(async {
                #[cfg(mobile)]
                mobile::init(app, api).await?;
                #[cfg(desktop)]
                desktop::init(app, api).await?;

                let gossip_arc_clamping = if cfg!(mobile) {
                    Some(String::from("empty"))
                } else {
                    None
                };

                let lair_client =
                    launch(&filesystem, admin_port, app_port, gossip_arc_clamping).await?;

                let mut retry_count = 0;
                let mut admin_ws = loop {
                    if let Ok(ws) =
                        AdminWebsocket::connect(format!("ws://localhost:{}", admin_port))
                            .await
                            .map_err(|err| {
                                crate::Error::AdminWebsocketError(format!(
                                    "Could not connect to the admin interface: {}",
                                    err
                                ))
                            })
                    {
                        break ws;
                    }
                    async_std::task::sleep(Duration::from_secs(1)).await;

                    retry_count += 1;
                    if retry_count == 80 {
                        panic!("Could not connect to holochain");
                    }
                };

                install_initial_apps_if_necessary(&mut admin_ws, &filesystem, config.initial_apps)
                    .await?;

                let r: crate::Result<(MetaLairClient, AdminWebsocket)> =
                    Ok((lair_client, admin_ws));
                r
            })?;

            http_server::start_http_server(app.clone(), http_server_port);

            // manage state so it is accessible by the commands
            app.manage(HolochainPlugin::<R> {
                app_handle: app.clone(),
                lair_client,
                runtime_info: HolochainRuntimeInfo {
                    http_server_port,
                    app_port,
                    admin_port,
                },
                filesystem,
            });

            Ok(())
        })
        .build()
}

async fn install_initial_apps_if_necessary(
    admin_ws: &mut AdminWebsocket,
    fs: &FileSystem,
    initial_apps: HashMap<String, WebAppBundle>,
) -> crate::Result<()> {
    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    if apps.len() == 0 {
        println!("Installing apps");
        for (app_id, app_bundle) in initial_apps {
            install_web_app(admin_ws, fs, app_bundle, app_id, HashMap::new(), None).await?;
        }
    }
    Ok(())
}
