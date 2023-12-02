use tauri::{command, AppHandle, Manager, Runtime};

use crate::{HolochainExt, HolochainRuntimeInfo};

#[command]
pub(crate) fn get_runtime_info<R: Runtime>(
    app_handle: AppHandle<R>,
) -> crate::Result<HolochainRuntimeInfo> {
    let info = &app_handle.holochain().runtime_info;

    Ok(info.clone())
}
