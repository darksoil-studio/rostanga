use holochain_types::prelude::SerializedBytesError;
use serde::{ser::Serializer, Serialize};

use holochain_client::ConductorApiError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
    #[error(transparent)]
    TauriPluginHolochainError(#[from] tauri_plugin_holochain::Error),
    #[error(transparent)]
    TauriPluginNotificationError(#[from] tauri_plugin_notification::Error),
    #[error(transparent)]
    TauriPluginCliError(#[from] tauri_plugin_cli::Error),
    #[error(transparent)]
    TauriError(#[from] tauri::Error),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    SerializedBytesError(#[from] SerializedBytesError),
    #[error(transparent)]
    FcmError(#[from] fcm_v1::Error),
    #[error("ConductorApiError: `{0:?}`")]
    ConductorApiError(ConductorApiError),
    #[error("Error modifying notification: `{0:?}`")]
    ModifyNotificationError(String),
    #[error("Error creating a nonce: `{0:?}`")]
    NonceError(String),
    #[error("Error signing zome call: `{0:?}`")]
    SignZomeCallError(String),
    #[error("Error setting up a signal listener: `{0:?}`")]
    SignalSetupError(String),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
