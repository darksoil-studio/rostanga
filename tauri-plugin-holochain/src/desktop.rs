use holochain::{conductor::{Conductor, ConductorHandle, state::AppInterfaceId}, prelude::kitsune_p2p::dependencies::kitsune_p2p_types::dependencies::lair_keystore_api::dependencies::sodoken::{BufWrite, BufRead}};
use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::{filesystem::FileSystem, PluginState};

pub async fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<()> {
    Ok(())
}
