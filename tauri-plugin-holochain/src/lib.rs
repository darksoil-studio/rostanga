use commands::install_web_app::install_web_app;
use filesystem::FileSystem;
use holochain::conductor::ConductorHandle;
use holochain_client::AdminWebsocket;
use holochain_keystore::MetaLairClient;
use holochain_types::web_app::WebAppBundle;
use launch::launch;
use tauri::{
    plugin::{Builder, TauriPlugin},
    scope::ipc::RemoteDomainAccessScope,
    AppHandle, Manager, Runtime, WindowBuilder, WindowUrl,
};

use std::{collections::HashMap, path::PathBuf, sync::Mutex, time::Duration};

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

struct PluginState {
    http_server_port: u16,
    filesystem: FileSystem,
    app_port: u16,
    admin_port: u16,
}

// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the holochain APIs.
pub trait HolochainExt<R: Runtime> {
    fn open_app(&self, app_id: String) -> Result<()>;
}

impl<R: Runtime, T: Manager<R>> crate::HolochainExt<R> for T {
    fn open_app(&self, app_id: String) -> Result<()> {
        println!("Opening app {}", app_id);
        let state = self.state::<PluginState>();
        self.ipc_scope().configure_remote_access(
            RemoteDomainAccessScope::new(format!("{}.localhost", app_id))
                .add_window(app_id.clone())
                .add_plugins(["holochain"]),
        );

        let launcher_env_command = format!(
            r#"window.__HC_LAUNCHER_ENV__ = {{ "APP_INTERFACE_PORT": {}, "ADMIN_INTERFACE_PORT": {}, "INSTALLED_APP_ID": "{}", "HTTP_SERVER_PORT": {} }};"#,
            state.app_port, state.admin_port, app_id, state.http_server_port
        );

        WindowBuilder::new(
            self,
            app_id.clone(),
            WindowUrl::App(PathBuf::from("index.html")),
            // WindowUrl::External(
            //     url::Url::parse(
            //         format!("http://{}.localhost:{}", app_id, state.http_server_port,).as_str(),
            //     )
            //     .expect("Cannot parse app_id"),
            // ),
        )
        // .initialization_script("console.error('hey');")
        .initialization_script(launcher_env_command.as_str())
        // .initialization_script("console.error(JSON.stringify(window.__HC_LAUNCHER_ENV__))")
        .build()?;
        // window.eval(launcher_env_command.as_str())?;
        // window.eval("console.error(JSON.stringify(window.__HC_LAUNCHER_ENV__))")?;

        println!("Opened app {}", app_id);
        Ok(())
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
            commands::get_locales::get_locales
        ])
        .setup(move |app: &AppHandle<R>, api| {
            let fs = FileSystem::new(app, &config.subfolder)?;
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

                let lair_client = launch(&fs, admin_port, app_port, gossip_arc_clamping).await?;

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

                install_initial_apps_if_necessary(&mut admin_ws, &fs, config.initial_apps).await?;

                let r: crate::Result<(MetaLairClient, AdminWebsocket)> =
                    Ok((lair_client, admin_ws));
                r
            })?;

            http_server::start_http_server(app.clone(), http_server_port);

            // manage state so it is accessible by the commands
            app.manage(lair_client);

            app.manage(PluginState {
                http_server_port,
                app_port,
                admin_port,
                filesystem: fs,
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
