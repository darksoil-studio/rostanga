use std::sync::Arc;

use holochain::{
    conductor::{
        config::{AdminInterfaceConfig, ConductorConfig, KeystoreConfig},
        interface::InterfaceDriver,
    },
    prelude::{
        kitsune_p2p::dependencies::kitsune_p2p_types::config::tuning_params_struct::KitsuneP2pTuningParams,
        KitsuneP2pConfig, TransportConfig,
    },
};

use crate::filesystem::FileSystem;

pub fn conductor_config(
    fs: &FileSystem,
    admin_port: u16,
    connection_url: url2::Url2,
    override_gossip_arc_clamping: Option<String>,
) -> ConductorConfig {
    let mut config = ConductorConfig::default();
    config.environment_path = fs.conductor_path().into();
    config.keystore = KeystoreConfig::LairServer { connection_url };

    let mut network_config = KitsuneP2pConfig::default();

    let mut tuning_params = KitsuneP2pTuningParams::default();

    if let Some(c) = override_gossip_arc_clamping {
        tuning_params.gossip_arc_clamping = c;
    }

    network_config.tuning_params = Arc::new(tuning_params);

    network_config.bootstrap_service = Some(url2::url2!("https://bootstrap.holo.host"));

    network_config.transport_pool.push(TransportConfig::WebRTC {
        signal_url: String::from("wss://signal.holo.host"),
    });

    config.network = Some(network_config);

    config.admin_interfaces = Some(vec![AdminInterfaceConfig {
        driver: InterfaceDriver::Websocket { port: admin_port },
    }]);

    config
}
