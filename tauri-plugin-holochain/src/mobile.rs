use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};
use holochain::{conductor::{Conductor, ConductorHandle, state::AppInterfaceId}, prelude::kitsune_p2p::dependencies::kitsune_p2p_types::dependencies::lair_keystore_api::dependencies::sodoken::{BufWrite, BufRead}};

use crate::{config, filesystem::FileSystem, PluginState};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "studio.darksoil.tauripluginholochain";

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_holochain);

// initializes the Kotlin or Swift plugin classes
#[cfg(target_os = "android")]
pub async fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<()> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "ExamplePlugin")?;
    let dir = app_dirs2::app_root(
        app_dirs2::AppDataType::UserCache,
        &app_dirs2::AppInfo {
            name: "studio.darksoil.rostangatest",
            author: "darksoil.studio",
        },
    )?;
    std::env::set_var("TMPDIR", dir);

    Ok(())
}

#[cfg(target_os = "ios")]
pub async fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<()> {
    let handle = api.register_ios_plugin(init_plugin_holochain)?;

    Ok(())
}

#[cfg(desktop)]
pub async fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<()> {
    Ok(())
}
