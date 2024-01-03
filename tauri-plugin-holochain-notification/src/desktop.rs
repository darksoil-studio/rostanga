use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<HolochainNotification<R>> {
    Ok(HolochainNotification(app.clone()))
}

/// Access to the holochain-notification APIs.
pub struct HolochainNotification<R: Runtime>(AppHandle<R>);

impl<R: Runtime> HolochainNotification<R> {
    // pub fn ping(&self, payload: PingRequest) -> crate::Result<PingResponse> {
    //   Ok(PingResponse {
    //     value: payload.value,
    //   })
    // }
}
