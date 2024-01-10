use std::{collections::HashMap, path::PathBuf, time::Duration};

use http_server::{pong_iframe, read_asset};
use hyper::StatusCode;
use launch::RunningHolochainInfo;
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
    AdminWebsocket, AppAgentWebsocket, AppInfo, AppWebsocket, ConductorApiError,
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

use commands::install_web_app::{install_app, install_web_app};
pub use error::{Error, Result};
use filesystem::FileSystem;
pub use launch::launch;

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
    fn build_window(&self, app_id: String, query_args: Option<String>) -> Result<Window<R>> {
        let app_id_env_command = format!(r#"window.__APP_ID__ = "{}";"#, app_id);

        let query_args = query_args.unwrap_or_default();

        let mut window_builder = WindowBuilder::new(
            &self.app_handle,
            app_id.clone(),
            WindowUrl::External(
                url::Url::parse(
                    format!(
                        "http://localhost:{}?{query_args}",
                        self.runtime_info.http_server_port
                    )
                    .as_str(),
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

        Ok(window)
    }

    pub fn open_app(&self, app_id: String) -> crate::Result<()> {
        println!("Opening app {}", app_id);

        let _window = self.build_window(app_id.clone(), None)?;

        println!("Opened app {}", app_id);
        Ok(())
    }

    pub async fn open_hrl(&self, hrl: Hrl) -> crate::Result<()> {
        println!("Opening hrl {:?}", hrl);

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

        let _window =
            self.build_window(app_info.installed_app_id.clone(), Some(query_args.clone()))?;

        println!(
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
        let lair_client = self.lair_client.lair_client();

        let app_ws = AppAgentWebsocket::connect(
            format!("ws://localhost:{}", self.runtime_info.app_port),
            app_id,
            lair_client,
        )
        .await
        .map_err(|err| crate::Error::WebsocketConnectionError(format!("{err:?}")))?;

        Ok(app_ws)
    }

    async fn workaround_join_failed_all_apps(&self) -> crate::Result<()> {
        let mut admin_websocket = self.admin_websocket().await?;

        let apps = admin_websocket
            .list_apps(None)
            .await
            .map_err(|err| crate::Error::ConductorApiError(err))?;

        let futures: Vec<_> = apps
            .iter()
            .map(|app| async {
                let app = app.clone();
                self.workaround_join_failed(app).await
            })
            .collect();
        futures::future::join_all(futures).await;

        Ok(())
    }

    async fn workaround_join_failed(&self, app_info: AppInfo) -> crate::Result<()> {
        let app_id = app_info.installed_app_id;
        let mut app_agent_websocket = self.app_agent_websocket(app_id.clone()).await?;
        let mut admin_websocket = self.admin_websocket().await?;

        for (role, cells) in app_info.cell_info {
            for cell in cells {
                match cell {
                    CellInfo::Provisioned(cell_info) => {
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
                                    ExternIO::encode(()).unwrap(),
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
                                        ExternIO::encode(()).unwrap(),
                                    )
                                    .await;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

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

        Ok(app_info)
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

/// Initializes the plugin.
pub fn init<R: Runtime>(subfolder: PathBuf) -> TauriPlugin<R> {
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
            let r: crate::Result<()> = tauri::async_runtime::block_on(async move {
                let app_data_dir = app.path().app_data_dir()?.join(&subfolder);
                let app_config_dir = app.path().app_config_dir()?.join(&subfolder);

                let http_server_port = portpicker::pick_unused_port().expect("No ports free");
                #[cfg(mobile)]
                mobile::init(app, api).await?;
                #[cfg(desktop)]
                desktop::init(app, api).await?;

                let RunningHolochainInfo {
                    admin_port,
                    app_port,
                    lair_client,
                    filesystem,
                } = launch().await?;

                log::info!("Starting http server at port {http_server_port:?}");

                http_server::start_http_server(app.clone(), http_server_port);

                let p = HolochainPlugin::<R> {
                    app_handle: app.clone(),
                    lair_client,
                    runtime_info: HolochainRuntimeInfo {
                        http_server_port,
                        app_port,
                        admin_port,
                    },
                    filesystem,
                };

                p.workaround_join_failed_all_apps().await?;

                // manage state so it is accessible by the commands
                app.manage(p);

                Ok(())
            });
            println!("Finished holochain plugin setup: {r:?}");
            r?;

            Ok(())
        })
        .build()
}
