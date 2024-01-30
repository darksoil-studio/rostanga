use app_dirs2::AppDirsError;
use holochain::{
    conductor::error::ConductorError,
    prelude::{AppBundleError, DnaError, RoleName, SerializedBytesError, ZomeError},
};
use holochain_client::{ConductorApiError, InstalledAppId};
use mr_bundle::error::MrBundleError;
use one_err::OneErr;
use serde::{ser::Serializer, Serialize};
use zip::result::ZipError;

use crate::filesystem::FileSystemError;

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
    FileSystemError(#[from] FileSystemError),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error("ConductorApiError: `{0:?}`")]
    ConductorApiError(ConductorApiError),

    #[error("Http server error: {0}")]
    HttpServerError(String),

    #[error("Filesystem error: {0}")]
    FilesystemError(String),

    #[error("Sign zome call error: {0}")]
    SignZomeCallError(String),

    #[error("Admin websocket error: {0}")]
    AdminWebsocketError(String),

    #[error("Error connecting websocket: {0}")]
    WebsocketConnectionError(String),

    #[error("Error opening app: {0}")]
    OpenAppError(String),

    #[error("Holochain has not been initialized yet")]
    HolochainNotInitialized,
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
