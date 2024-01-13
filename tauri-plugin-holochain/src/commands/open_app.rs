use crate::HolochainExt;
use tauri::{command, AppHandle, Runtime};

#[command]
pub(crate) fn open_app<R: Runtime>(app: AppHandle<R>, app_id: String) -> crate::Result<()> {
    app.holochain()?.open_app(app_id)
}
