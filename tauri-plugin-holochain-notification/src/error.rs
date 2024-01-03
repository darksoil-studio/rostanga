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
    #[error("ConductorApiError: `{0:?}`")]
    ConductorApiError(ConductorApiError),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
