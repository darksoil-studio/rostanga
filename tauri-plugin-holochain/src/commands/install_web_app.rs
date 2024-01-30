use std::collections::{BTreeMap, HashMap};

use holochain::prelude::{
    AppBundle, AppBundleError, AppBundleSource, AppManifest, DnaBundle, DnaError, DnaFile, DnaHash,
    MembraneProof, NetworkSeed, RoleName, ZomeError, ZomeName,
};
use holochain_client::{
    AdminWebsocket, AppInfo, ConductorApiError, InstallAppPayload, InstalledAppId,
};
use holochain_conductor_api::CellInfo;
use holochain_types::web_app::WebAppBundle;
use mr_bundle::{error::MrBundleError, Bundle};

use crate::filesystem::{FileSystem, FileSystemError};

pub async fn install_web_app(
    admin_ws: &mut AdminWebsocket,
    fs: &FileSystem,
    app_id: String,
    bundle: WebAppBundle,
    membrane_proofs: HashMap<RoleName, MembraneProof>,
    network_seed: Option<NetworkSeed>,
) -> crate::Result<AppInfo> {
    let app_info = install_app(
        admin_ws,
        app_id.clone(),
        bundle.happ_bundle().await?,
        membrane_proofs,
        network_seed,
    )
    .await?;

    fs.ui_store().extract_and_store_ui(&app_id, &bundle).await?;
    log::info!("Installed web-app's ui {app_id:?}");

    Ok(app_info)
}

pub async fn install_app(
    admin_ws: &mut AdminWebsocket,
    app_id: String,
    bundle: AppBundle,
    membrane_proofs: HashMap<RoleName, MembraneProof>,
    network_seed: Option<NetworkSeed>,
) -> crate::Result<AppInfo> {
    log::info!("Installing app {}", app_id);

    let agent_key = admin_ws
        .generate_agent_pub_key()
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    let app_info = admin_ws
        .install_app(InstallAppPayload {
            agent_key,
            membrane_proofs,
            network_seed,
            source: AppBundleSource::Bundle(bundle),
            installed_app_id: Some(app_id.clone()),
        })
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;
    log::info!("Installed app {app_info:?}");

    let response = admin_ws
        .enable_app(app_id.clone())
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    log::info!("Enabled app {app_id:?}");

    Ok(response.app)
}

pub async fn update_web_app(
    admin_ws: &mut AdminWebsocket,
    fs: &FileSystem,
    app_id: String,
    bundle: WebAppBundle,
) -> Result<(), UpdateAppError> {
    // let app_info = update_app(
    //     admin_ws,
    //     app_id.clone(),
    //     bundle.happ_bundle().await?,
    // )
    // .await?;

    fs.ui_store().extract_and_store_ui(&app_id, &bundle).await?;
    log::info!("Updated web-app's ui {app_id:?}");

    // Ok(app_info)
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateAppError {
    #[error(transparent)]
    AppBundleError(#[from] AppBundleError),

    #[error(transparent)]
    ZomeError(#[from] ZomeError),

    #[error(transparent)]
    MrBundleError(#[from] MrBundleError),

    #[error(transparent)]
    FileSystemError(#[from] FileSystemError),

    #[error(transparent)]
    DnaError(#[from] DnaError),

    #[error("ConductorApiError: `{0:?}`")]
    ConductorApiError(ConductorApiError),

    #[error("The given app was not found: {0}")]
    AppNotFound(String),

    #[error("The role {0} was not found the app {1}")]
    RoleNotFound(RoleName, InstalledAppId),
}

// TODO
pub async fn update_app(
    admin_ws: &mut AdminWebsocket,
    app_id: String,
    bundle: AppBundle,
) -> Result<AppInfo, UpdateAppError> {
    log::info!("Updating app {}", app_id);

    // Get the DNA def from the admin websocket
    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| UpdateAppError::ConductorApiError(err))?;

    let app = apps
        .into_iter()
        .find(|app| app.installed_app_id.eq(&app_id))
        .ok_or(UpdateAppError::AppNotFound(app_id))?;

    let new_dna_files = resolve_dna_files(bundle).await?;

    for (role_name, new_dna_file) in new_dna_files {
        let cells = app
            .cell_info
            .remove(&role_name)
            .ok_or(UpdateAppError::RoleNotFound(
                role_name,
                app.installed_app_id,
            ))?;

        if let Some(cell) = cells.first() {
            let dna_hash = match cell {
                CellInfo::Provisioned(c) => c.cell_id.dna_hash().clone(),
                CellInfo::Cloned(c) => c.cell_id.dna_hash().clone(),
                CellInfo::Stem(c) => c.original_dna_hash,
            };
            let old_dna_def = admin_ws
                .get_dna_definition(dna_hash)
                .await
                .map_err(|err| UpdateAppError::ConductorApiError(err))?;

            for (zome_name, coordinator_zome) in new_dna_file.dna_def().coordinator_zomes {
                let new_wasm_hash = coordinator_zome.wasm_hash(&zome_name)?;

                if let Some(old_zome_def) = old_dna_def
                    .coordinator_zomes
                    .iter()
                    .find(|(zome, _)| zome.eq(&zome_name))
                {
                } else {
                }

                // Bundle::new(, , )
            }
        }
    }

    log::info!("Updated app {app_id:?}");

    Ok(response.app)
}

async fn resolve_dna_files(
    app_bundle: AppBundle,
) -> Result<BTreeMap<RoleName, DnaFile>, UpdateAppError> {
    let mut dna_files: BTreeMap<RoleName, DnaFile> = BTreeMap::new();

    let bundle = app_bundle.into_inner();

    for app_role in bundle.manifest().app_roles() {
        if let Some(location) = app_role.dna.location {
            let (dna_def, _) = resolve_location(&bundle, &location).await?;

            dna_files.insert(app_role.name.clone(), dna_def);
        }
    }

    Ok(dna_files)
}

async fn resolve_location(
    app_bundle: &Bundle<AppManifest>,
    location: &mr_bundle::Location,
) -> Result<(DnaFile, DnaHash), UpdateAppError> {
    let bytes = app_bundle.resolve(location).await?;
    let dna_bundle: DnaBundle = mr_bundle::Bundle::decode(&bytes)?.into();
    let (dna_file, original_hash) = dna_bundle.into_dna_file(Default::default()).await?;
    Ok((dna_file, original_hash))
}
