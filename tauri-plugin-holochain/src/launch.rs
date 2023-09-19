use std::{io::Write, sync::Arc};

use holochain::{
    conductor::{state::AppInterfaceId, Conductor},
    prelude::kitsune_p2p::dependencies::kitsune_p2p_types::dependencies::lair_keystore_api::{
        dependencies::sodoken::{BufRead, BufWrite},
        prelude::{LairServerConfig, LairServerConfigInner},
    },
};
use holochain_keystore::{lair_keystore::spawn_lair_keystore_in_proc, MetaLairClient};

use crate::filesystem::FileSystem;

pub fn vec_to_locked(mut pass_tmp: Vec<u8>) -> std::io::Result<BufRead> {
    match BufWrite::new_mem_locked(pass_tmp.len()) {
        Err(e) => {
            pass_tmp.fill(0);
            Err(e.into())
        }
        Ok(p) => {
            {
                let mut lock = p.write_lock();
                lock.copy_from_slice(&pass_tmp);
                pass_tmp.fill(0);
            }
            Ok(p.to_read())
        }
    }
}

pub async fn launch(
    fs: &FileSystem,
    admin_port: u16,
    app_port: u16,
    override_gossip_arc_clamping: Option<String>,
) -> crate::Result<MetaLairClient> {
    let passphrase = vec_to_locked(vec![]).expect("Can't build passphrase");
    let fs = fs.clone();

    let lair_client = spawn_lair_keystore_in_proc(fs.keystore_config_path(), passphrase.clone())
        .await
        .map_err(|err| crate::Error::LairError(err))?;
    let config = read_config(&fs.keystore_config_path())?;

    let connection_url = config.connection_url.clone();

    tauri::async_runtime::spawn(async move {
        let config = crate::config::conductor_config(
            &fs,
            admin_port,
            connection_url.into(),
            override_gossip_arc_clamping,
        );

        let conductor = Conductor::builder()
            .config(config)
            .passphrase(Some(passphrase))
            .build()
            .await
            .expect("Can't build the conductor");

        let p: either::Either<u16, AppInterfaceId> = either::Either::Left(app_port);
        conductor
            .clone()
            .add_app_interface(p)
            .await
            .expect("Can't add app interface");
    });

    Ok(lair_client)
}

fn read_config(config_path: &std::path::Path) -> crate::Result<LairServerConfig> {
    let bytes = std::fs::read(config_path)?;

    let config =
        LairServerConfigInner::from_bytes(&bytes).map_err(|err| crate::Error::LairError(err))?;

    Ok(Arc::new(config))
}
