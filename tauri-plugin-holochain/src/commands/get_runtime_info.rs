use tauri::{command, AppHandle, Manager, Runtime};

use crate::{HolochainExt, HolochainPlugin, HolochainRuntimeInfo};

#[command]
pub(crate) fn get_runtime_info<R: Runtime>(
    app_handle: AppHandle<R>,
) -> crate::Result<HolochainRuntimeInfo> {
    let info = &app_handle.holochain()?.runtime_info;

    Ok(info.clone())
}

#[command]
pub(crate) fn is_holochain_ready<R: Runtime>(app_handle: AppHandle<R>) -> bool {
    app_handle.try_state::<HolochainPlugin<R>>().is_some()
}
