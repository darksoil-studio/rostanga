use std::{io::Write, sync::Arc};

use holochain::{
    conductor::{state::AppInterfaceId, Conductor},
    prelude::kitsune_p2p::dependencies::kitsune_p2p_types::dependencies::{lair_keystore_api::{
        dependencies::{sodoken::{BufRead, BufWrite, self}, one_err},
        prelude::{LairServerConfig, LairServerConfigInner},
    }, tokio::{self, io::AsyncWriteExt}},
};
use holochain_keystore::{ MetaLairClient, lair_keystore::spawn_lair_keystore, LairResult};
use lair_keystore::server::StandaloneServer;

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

    if let Err(e) = std::fs::read(config.clone().pid_file) { // Workaround xcode different containers
        std::fs::remove_dir_all(config_path.parent().unwrap())?;
        std::fs::create_dir_all(config_path.parent().unwrap())?;
        return Err(e)?;
    }

    Ok(Arc::new(config))
}

/// Spawn an in-process keystore backed by lair_keystore.
pub async fn spawn_lair_keystore_in_proc(
    config_path: std::path::PathBuf,
    passphrase: sodoken::BufRead,
) -> LairResult<MetaLairClient> {
    let config = get_config(&config_path, passphrase.clone()).await?;
    let connection_url = config.connection_url.clone();

    // rather than using the in-proc server directly,
    // use the actual standalone server so we get the pid-checks, etc
    let mut server = StandaloneServer::new(config).await?;

    server.run(passphrase.clone()).await?;

    // just incase a Drop gets impld at some point...
    std::mem::forget(server);

    // now, just connect to it : )
    spawn_lair_keystore(connection_url.into(), passphrase).await
}

async fn get_config(
    config_path: &std::path::Path,
    passphrase: sodoken::BufRead,
) -> LairResult<LairServerConfig> {
    match read_config(config_path) {
        Ok(config) => Ok(config),
        Err(_) => write_config(config_path, passphrase).await,
    }
}

async fn write_config(
    config_path: &std::path::Path,
    passphrase: sodoken::BufRead,
) -> LairResult<LairServerConfig> {
    let lair_root = config_path
        .parent()
        .ok_or_else(|| one_err::OneErr::from("InvalidLairConfigDir"))?;

    tokio::fs::DirBuilder::new()
        .recursive(true)
        .create(&lair_root)
        .await?;

    let config = LairServerConfigInner::new(lair_root, passphrase).await?;

    let mut config_f = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config_path)
        .await?;

    config_f.write_all(config.to_string().as_bytes()).await?;
    config_f.shutdown().await?;
    drop(config_f);

    Ok(Arc::new(config))
}
