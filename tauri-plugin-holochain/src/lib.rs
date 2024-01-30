use std::{collections::HashMap, path::PathBuf, time::Duration};

use http_server::{pong_iframe, read_asset};
use hyper::StatusCode;
use lair_keystore_api::LairClient;
pub use launch::RunningHolochainInfo;
use serde::{Deserialize, Serialize};
use tauri::{
    http::response,
    plugin::{Builder, TauriPlugin},
    scope::ipc::RemoteDomainAccessScope,
    AppHandle, Manager, Runtime, Window, WindowBuilder, WindowUrl,
};

use holochain::prelude::{
    holochain_serial, AnyDhtHash, AppBundle, DnaHash, ExternIO, MembraneProof, NetworkSeed,
    RoleName, SerializedBytes,
};
use holochain_client::{
    AdminWebsocket, AppAgentWebsocket, AppInfo, AppWebsocket, ConductorApiError, InstallAppPayload,
};
use holochain_conductor_api::CellInfo;
use holochain_keystore::MetaLairClient;
use holochain_types::web_app::WebAppBundle;
use hrl::Hrl;

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

use commands::install_web_app::{install_app, install_web_app, update_web_app};
pub use error::{Error, Result};
use filesystem::FileSystem;
pub use launch::launch;

use crate::launch::wait_until_app_ws_is_available;

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
    pub lair_client: LairClient,
}

impl<R: Runtime> HolochainPlugin<R> {
    fn build_window(
        &self,
        app_id: String,
        label: String,
        query_args: Option<String>,
    ) -> Result<Window<R>> {
        let app_id_env_command = format!(r#"window.__APP_ID__ = "{}";"#, app_id);

        let query_args = query_args.unwrap_or_default();

        let mut window_builder = WindowBuilder::new(
            &self.app_handle,
            label.clone(),
            WindowUrl::External(url::Url::parse(
                format!(
                    "http://localhost:{}?{query_args}",
                    self.runtime_info.http_server_port
                )
                .as_str(),
            )?),
        )
        .initialization_script(app_id_env_command.as_str());

        #[cfg(desktop)]
        {
            window_builder = window_builder
                .min_inner_size(1000.0, 800.0)
                .title(app_id.clone());
        }
        let window = window_builder.build()?;

        self.app_handle.ipc_scope().configure_remote_access(
            RemoteDomainAccessScope::new("localhost")
                .add_window(window.label())
                .add_plugin("holochain"),
        );

        Ok(window)
    }

    pub async fn open_app(&self, app_id: String) -> crate::Result<()> {
        log::info!("Opening app {}", app_id);

        wait_until_app_ws_is_available(self.runtime_info.app_port).await?;
        log::info!("AppWebsocket is available");

        let _window = self.build_window(app_id.clone(), app_id.clone(), None)?;

        log::info!("Opened app {}", app_id);
        Ok(())
    }

    pub async fn open_hrl(&self, hrl: Hrl) -> crate::Result<()> {
        log::info!("Opening hrl {:?}", hrl);

        let mut admin_ws = self.admin_websocket().await?;

        let apps = admin_ws.list_apps(None).await.expect("Failed to list apps");

        let dna_hash = &hrl.dna_hash;

        let app_info = apps
            .into_iter()
            .find_map(|app_info| {
                app_info.cell_info.values().find_map(|cells| {
                    cells.iter().find_map(|cell_info| match cell_info {
                        CellInfo::Provisioned(cell) => match cell.cell_id.dna_hash().eq(dna_hash) {
                            true => Some(app_info.clone()),
                            false => None,
                        },
                        CellInfo::Cloned(cell) => match cell.cell_id.dna_hash().eq(dna_hash) {
                            true => Some(app_info.clone()),
                            false => None,
                        },
                        _ => None,
                    })
                })
            })
            .ok_or(crate::Error::OpenAppError(format!(
                "Could not find any app for this hrl: {hrl:?}"
            )))?;

        let query_args = format!("hrl={hrl:?}");

        let uid = nanoid::nanoid!(5);
        let label = format!("{}_{uid}", app_info.installed_app_id);

        // let _window = self.build_window(
        //     app_info.installed_app_id.clone(),
        //     label,
        //     // Some(query_args.clone()),
        //     None,
        // )?;

        log::info!(
            "Opened app {} with query_args {query_args}",
            app_info.installed_app_id
        );
        Ok(())
    }

    pub async fn admin_websocket(&self) -> crate::Result<AdminWebsocket> {
        let admin_ws =
            AdminWebsocket::connect(format!("ws://localhost:{}", self.runtime_info.admin_port))
                .await
                .map_err(|err| crate::Error::WebsocketConnectionError(format!("{err:?}")))?;
        Ok(admin_ws)
    }

    pub async fn app_websocket(&self) -> crate::Result<AppWebsocket> {
        let app_ws =
            AppWebsocket::connect(format!("ws://localhost:{}", self.runtime_info.app_port))
                .await
                .map_err(|err| crate::Error::WebsocketConnectionError(format!("{err:?}")))?;
        Ok(app_ws)
    }

    pub async fn app_agent_websocket(&self, app_id: String) -> crate::Result<AppAgentWebsocket> {
        let app_ws = AppAgentWebsocket::connect(
            format!("ws://localhost:{}", self.runtime_info.app_port),
            app_id,
            self.lair_client.clone(),
        )
        .await
        .map_err(|err| crate::Error::WebsocketConnectionError(format!("{err:?}")))?;

        Ok(app_ws)
    }

    // async fn workaround_join_failed_all_apps(&self) -> crate::Result<()> {
    //     let mut admin_websocket = self.admin_websocket().await?;

    //     let apps = admin_websocket
    //         .list_apps(None)
    //         .await
    //         .map_err(|err| crate::Error::ConductorApiError(err))?;

    //     let futures: Vec<_> = apps
    //         .iter()
    //         .map(|app| async {
    //             let app = app.clone();
    //             self.workaround_join_failed(app).await
    //         })
    //         .collect();
    //     futures::future::join_all(futures).await;

    //     Ok(())
    // }

    async fn workaround_join_failed(&self, app_info: AppInfo) -> crate::Result<()> {
        let app_id = app_info.installed_app_id.clone();

        let app_id = &app_id;

        futures::future::join_all(app_info.clone().cell_info.into_iter().map(
            |(role, cells)| async move {
                for cell in cells {
                    match cell {
                        CellInfo::Provisioned(cell_info) => {
                            let mut app_agent_websocket =
                                self.app_agent_websocket(app_id.clone()).await?;
                            let mut admin_websocket = self.admin_websocket().await?;
                            let dna_def = admin_websocket
                                .get_dna_definition(cell_info.cell_id.dna_hash().clone())
                                .await
                                .map_err(|err| crate::Error::ConductorApiError(err))?;

                            log::info!("Called dna def {dna_def:?}");

                            if let Some((zome_name, _)) = dna_def.integrity_zomes.first() {
                                let mut result = app_agent_websocket
                                    .call_zome(
                                        role.clone(),
                                        zome_name.clone(),
                                        "entry_defs".into(),
                                        ExternIO::encode(()).expect("Failed to encode payload 1"),
                                    )
                                    .await;
                                log::info!("Called entry_defs {result:?}");

                                fn is_pending_join_error(
                                    result: &std::result::Result<ExternIO, ConductorApiError>,
                                ) -> bool {
                                    if let Err(err) = result {
                                        !format!("{err:?}").contains(
                                            "Attempted to call a zome function that doesn't exist",
                                        )
                                    } else {
                                        false
                                    }
                                }

                                while is_pending_join_error(&result) {
                                    log::error!("Error calling entry_defs {result:?}");
                                    std::thread::sleep(std::time::Duration::from_millis(400));
                                    admin_websocket
                                        .disable_app(app_id.clone())
                                        .await
                                        .map_err(|err| crate::Error::ConductorApiError(err))?;
                                    admin_websocket
                                        .enable_app(app_id.clone())
                                        .await
                                        .map_err(|err| crate::Error::ConductorApiError(err))?;
                                    result = app_agent_websocket
                                        .call_zome(
                                            role.clone(),
                                            zome_name.clone(),
                                            "entry_defs".into(),
                                            ExternIO::encode(())
                                                .expect("Failed to encode payload 1"),
                                        )
                                        .await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            },
        ))
        .await
        .into_iter()
        .collect::<crate::Result<Vec<_>>>()?;

        Ok(())
    }

    pub async fn install_web_app(
        &self,
        app_id: String,
        web_app_bundle: WebAppBundle,
        membrane_proofs: HashMap<RoleName, MembraneProof>,
        network_seed: Option<NetworkSeed>,
    ) -> crate::Result<AppInfo> {
        let mut admin_ws = self.admin_websocket().await?;
        let app_info = install_web_app(
            &mut admin_ws,
            &self.filesystem,
            app_id.clone(),
            web_app_bundle,
            membrane_proofs,
            network_seed,
        )
        .await?;

        self.workaround_join_failed(app_info.clone()).await?;
        self.app_handle.emit("app-installed", app_id)?;

        Ok(app_info)
    }

    pub async fn install_app(
        &self,
        app_id: String,
        app_bundle: AppBundle,
        membrane_proofs: HashMap<RoleName, MembraneProof>,
        network_seed: Option<NetworkSeed>,
    ) -> crate::Result<AppInfo> {
        let mut admin_ws = self.admin_websocket().await?;
        let app_info = install_app(
            &mut admin_ws,
            app_id.clone(),
            app_bundle,
            membrane_proofs,
            network_seed,
        )
        .await?;

        self.workaround_join_failed(app_info.clone()).await?;

        self.app_handle.emit("app-installed", app_id)?;
        Ok(app_info)
    }

    pub async fn update_web_app(
        &self,
        app_id: String,
        web_app_bundle: WebAppBundle,
    ) -> crate::Result<()> {
        let mut admin_ws = self.admin_websocket().await?;
        let app_info = update_web_app(
            &mut admin_ws,
            &self.filesystem,
            app_id.clone(),
            web_app_bundle,
        )
        .await?;

        self.app_handle.emit("app-updated", app_id)?;

        Ok(())
    }

    pub async fn update_app(
        &self,
        app_id: String,
        app_bundle: AppBundle,
    ) -> crate::Result<AppInfo> {
        let mut admin_ws = self.admin_websocket().await?;
        let app_info = update_app(&mut admin_ws, app_id.clone(), app_bundle).await?;

        self.workaround_join_failed(app_info.clone()).await?;

        self.app_handle.emit("app-updated", app_id)?;
        Ok(app_info)
    }
}

// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the holochain APIs.
pub trait HolochainExt<R: Runtime> {
    fn holochain(&self) -> crate::Result<&HolochainPlugin<R>>;
}

impl<R: Runtime, T: Manager<R>> crate::HolochainExt<R> for T {
    fn holochain(&self) -> crate::Result<&HolochainPlugin<R>> {
        let s = self
            .try_state::<HolochainPlugin<R>>()
            .ok_or(crate::Error::HolochainNotInitialized)?;

        Ok(s.inner())
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>(subfolder: PathBuf) -> TauriPlugin<R> {
    Builder::new("holochain")
        .invoke_handler(tauri::generate_handler![
            commands::sign_zome_call::sign_zome_call,
            commands::get_locales::get_locales,
            commands::open_app::open_app,
            commands::list_apps::list_apps,
            commands::get_runtime_info::get_runtime_info,
            commands::get_runtime_info::is_holochain_ready
        ])
        .register_uri_scheme_protocol("happ", |app_handle, request| {
            log::info!("Received request {}", request.uri().to_string());
            if request.uri().to_string().starts_with("happ://ping") {
                return response::Builder::new()
                    .status(StatusCode::ACCEPTED)
                    .header("Content-Type", "text/html;charset=utf-8")
                    .body(pong_iframe().as_bytes().to_vec())
                    .expect("Failed to build body of accepted response");
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
                    .expect("Malformed request: not enough items")
                    .clone();
                let uri_without_querystring: String = uri_without_protocol
                    .split("?")
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .get(0)
                    .expect("Malformed request: not enough items 2")
                    .clone();
                let uri_components: Vec<String> = uri_without_querystring
                    .split("/")
                    .map(|s| s.to_string())
                    .collect();
                let lowercase_app_id = uri_components
                    .get(0)
                    .expect("Malformed request: not enough items 3");
                let mut asset_file = PathBuf::new();
                for i in 1..uri_components.len() {
                    asset_file = asset_file.join(uri_components[i].clone());
                }

                let Ok(holochain) = app_handle.holochain() else {
                    return response::Builder::new()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(
                            format!("Called http UI before initializing holochain")
                                .as_bytes()
                                .to_vec(),
                        )
                        .expect("Failed to build asset with not internal server error");
                };

                let r = match read_asset(
                    &holochain.filesystem,
                    lowercase_app_id,
                    asset_file
                        .as_os_str()
                        .to_str()
                        .expect("Malformed request: not enough items 4")
                        .to_string(),
                )
                .await
                {
                    Ok(Some((asset, mime_type))) => {
                        log::info!("Got asset for app with id: {}", lowercase_app_id);
                        let mut response = response::Builder::new().status(StatusCode::ACCEPTED);
                        if let Some(mime_type) = mime_type {
                            response = response
                                .header("Content-Type", format!("{};charset=utf-8", mime_type))
                        } else {
                            response = response.header("Content-Type", "charset=utf-8")
                        }

                        return response
                            .body(asset)
                            .expect("Failed to build response with asset");
                    }
                    Ok(None) => response::Builder::new()
                        .status(StatusCode::NOT_FOUND)
                        .body(vec![])
                        .expect("Failed to build asset with not found"),
                    Err(e) => response::Builder::new()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(format!("{:?}", e).as_bytes().to_vec())
                        .expect("Failed to build asset with not internal server error"),
                };

                // admin_ws.close();
                r
            })
        })
        .build()
}

pub async fn setup_holochain<R: Runtime>(app_handle: AppHandle<R>) -> crate::Result<()> {
    // let app_data_dir = app.path().app_data_dir()?.join(&subfolder);
    // let app_config_dir = app.path().app_config_dir()?.join(&subfolder);

    let http_server_port = portpicker::pick_unused_port().expect("No ports free");
    #[cfg(mobile)]
    mobile::init(&app_handle)
        .await
        .expect("Could not init plugin");
    #[cfg(desktop)]
    desktop::init(&app_handle)
        .await
        .expect("Could not init plugin");

    let RunningHolochainInfo {
        admin_port,
        app_port,
        lair_client,
        filesystem,
    } = launch().await?;

    log::info!("Starting http server at port {http_server_port:?}");

    http_server::start_http_server(app_handle.clone(), http_server_port);

    let p = HolochainPlugin::<R> {
        app_handle: app_handle.clone(),
        lair_client,
        runtime_info: HolochainRuntimeInfo {
            http_server_port,
            app_port,
            admin_port,
        },
        filesystem,
    };

    // manage state so it is accessible by the commands
    app_handle.manage(p);

    app_handle.emit("holochain-ready", ())?;

    Ok(())
}
