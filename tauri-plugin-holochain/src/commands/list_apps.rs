use crate::HolochainExt;
use holochain_client::AppInfo;
use tauri::{command, AppHandle, Runtime};

#[command]
pub(crate) async fn list_apps<R: Runtime>(app: AppHandle<R>) -> crate::Result<Vec<AppInfo>> {
    let mut admin_ws = app.holochain()?.admin_websocket().await?;

    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    Ok(apps)
}
