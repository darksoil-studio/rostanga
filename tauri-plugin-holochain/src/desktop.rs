use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

pub async fn init<R: Runtime>(app: &AppHandle<R>) -> crate::Result<()> {
    Ok(())
}
