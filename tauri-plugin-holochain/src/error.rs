use holochain::{prelude::{SerializedBytesError, kitsune_p2p::dependencies::kitsune_p2p_types::dependencies::lair_keystore_api::dependencies::one_err::OneErr}, conductor::error::ConductorError};
use holochain_client::ConductorApiError;
use serde::{ser::Serializer, Serialize};
use mr_bundle::error::MrBundleError;
use zip::result::ZipError;
use app_dirs2::AppDirsError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),

    #[error(transparent)]
    LairError(OneErr),

    #[error(transparent)]
    ConductorError(#[from] ConductorError),

    #[error(transparent)]
    SerializedBytesError(#[from] SerializedBytesError),

    #[error(transparent)]
    MrBundleError(#[from] MrBundleError),

    #[error(transparent)]
    AppDirsError(#[from] AppDirsError),

    #[error(transparent)]
    TauriError(#[from] tauri::Error),

    #[error(transparent)]
    ZipError(#[from] ZipError),

    #[error("ConductorApiError: `{0:?}`")]
    ConductorApiError(ConductorApiError),

    #[error("Http server error: {0}")]
    HttpServerError(String),

    #[error("Filesystem error: {0}")]
    FilesystemError(String),

    #[error("Admin websocket error: {0}")]
    AdminWebsocketError(String),

    #[error("Error connecting websocket: {0}")]
    WebsocketConnectionError(String),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
