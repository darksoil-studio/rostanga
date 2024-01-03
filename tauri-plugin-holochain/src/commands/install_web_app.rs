use std::collections::HashMap;

use holochain::prelude::{AppBundle, AppBundleSource, MembraneProof, NetworkSeed, RoleName};
use holochain_client::{AdminWebsocket, InstallAppPayload};
use holochain_types::web_app::WebAppBundle;

use crate::{filesystem::FileSystem, Result};

pub async fn install_web_app(
    admin_ws: &mut AdminWebsocket,
    fs: &FileSystem,
    app_id: String,
    bundle: WebAppBundle,
    membrane_proofs: HashMap<RoleName, MembraneProof>,
    network_seed: Option<NetworkSeed>,
) -> Result<()> {
    install_app(
        admin_ws,
        app_id.clone(),
        bundle.happ_bundle().await?,
        membrane_proofs,
        network_seed,
    )
    .await?;

    fs.ui_store().extract_and_store_ui(&app_id, &bundle).await?;
    println!("Installed app {}", app_id);

    Ok(())
}

pub async fn install_app(
    admin_ws: &mut AdminWebsocket,
    app_id: String,
    bundle: AppBundle,
    membrane_proofs: HashMap<RoleName, MembraneProof>,
    network_seed: Option<NetworkSeed>,
) -> Result<()> {
    println!("Installing app {}", app_id);
    let agent_key = admin_ws
        .generate_agent_pub_key()
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    admin_ws
        .install_app(InstallAppPayload {
            agent_key,
            membrane_proofs,
            network_seed,
            source: AppBundleSource::Bundle(bundle),
            installed_app_id: Some(app_id.clone()),
        })
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;
    admin_ws
        .enable_app(app_id.clone())
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    println!("Installed app {}", app_id);

    Ok(())
}
