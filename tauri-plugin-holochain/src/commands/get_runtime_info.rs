use tauri::{command, AppHandle, Manager, Runtime};

use crate::HolochainRuntimeInfo;

#[command]
pub(crate) fn get_runtime_info<R: Runtime>(
    app_handle: AppHandle<R>,
) -> crate::Result<HolochainRuntimeInfo> {
    let info = app_handle.state::<HolochainRuntimeInfo>();

    let info_ref: &HolochainRuntimeInfo = &info;

    Ok(info_ref.clone())
}
