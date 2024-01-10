use std::sync::Arc;

use holochain::{
    conductor::{
        config::{AdminInterfaceConfig, ConductorConfig, KeystoreConfig},
        interface::InterfaceDriver,
    },
    prelude::dependencies::kitsune_p2p_types::config::{
        tuning_params_struct::KitsuneP2pTuningParams, KitsuneP2pConfig, ProxyConfig,
        TransportConfig,
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
    config.data_root_path = Some(fs.conductor_dir().into());
    config.keystore = KeystoreConfig::LairServer { connection_url };

    let mut network_config = KitsuneP2pConfig::default();

    let mut tuning_params = KitsuneP2pTuningParams::default();

    if let Some(c) = override_gossip_arc_clamping {
        tuning_params.gossip_arc_clamping = c;
    }

    network_config.tuning_params = Arc::new(tuning_params);

    network_config.bootstrap_service = Some(url2::url2!("https://bootstrap.holo.host"));

    //tx2
    // network_config.transport_pool.push(TransportConfig::Proxy {
    //     sub_transport: Box::new(TransportConfig::Quic {
    //         bind_to: None,
    //         override_host: None,
    //         override_port: None,
    //     }),
    //     proxy_config: ProxyConfig::RemoteProxyClient {
    //         proxy_url: url2::url2!("kitsune-proxy://f3gH2VMkJ4qvZJOXx0ccL_Zo5n-s_CnBjSzAsEHHDCA/kitsune-quic/h/137.184.142.208/p/5788/--")
    //     },
    // });
    // tx5
    network_config.transport_pool.push(TransportConfig::WebRTC {
        signal_url: String::from("wss://signal.holo.host"),
    });

    config.network = Some(network_config);

    config.admin_interfaces = Some(vec![AdminInterfaceConfig {
        driver: InterfaceDriver::Websocket { port: admin_port },
    }]);

    config
}
