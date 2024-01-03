use hc_zome_trait_pending_notifications::{GetNotificationInput, Notification};
use holochain_client::{sign_zome_call_with_client, AdminWebsocket, AppWebsocket};
use holochain_conductor_api::CellInfo;
use holochain_state::nonce::fresh_nonce;
use holochain_types::{
    prelude::{
        AnyDhtHash, AnyDhtHashB64, AppBundle, DnaHash, DnaHashB64, ExternIO, FunctionName,
        Timestamp, ZomeCallUnsigned, ZomeName,
    },
    web_app::WebAppBundle,
};
use tauri_plugin_holochain::{launch_in_background, HolochainExt};

use tauri_plugin_notification::*;

use crate::HrlBody;

#[tauri_plugin_notification::modify_push_notification]
pub fn modify_push_notification(mut notification: NotificationData) -> NotificationData {
    tauri::async_runtime::block_on(async move {
        let body = notification.body.expect("EMPTY NOTIFICATION BODY");

        let hrl_body: HrlBody =
            serde_json::from_str(body.as_str()).expect("Malformed notification body");

        let admin_port = portpicker::pick_unused_port().expect("No ports free");
        let app_port = portpicker::pick_unused_port().expect("No ports free");
        let meta_lair_client = launch_in_background(admin_port, app_port)
            .await
            .expect("Failed to launch holochain");

        let mut admin_ws = AdminWebsocket::connect(format!("ws://localhost:{}", admin_port))
            .await
            .expect("Could not connect to admin interface");
        let mut app_ws = AppWebsocket::connect(format!("ws://localhost:{}", app_port))
            .await
            .expect("Could not connect to app interface");

        let apps = admin_ws.list_apps(None).await.expect("Failed to list apps");

        let dna_hash = DnaHash::from(hrl_body.dna_hash);

        let cell_id = apps
            .into_iter()
            .find_map(|app_info| {
                app_info.cell_info.values().find_map(|cells| {
                    cells.iter().find_map(|cell_info| match cell_info {
                        CellInfo::Provisioned(cell) => {
                            match cell.cell_id.dna_hash().eq(&dna_hash) {
                                true => Some(cell.cell_id.clone()),
                                false => None,
                            }
                        }
                        CellInfo::Cloned(cell) => match cell.cell_id.dna_hash().eq(&dna_hash) {
                            true => Some(cell.cell_id.clone()),
                            false => None,
                        },
                        _ => None,
                    })
                })
            })
            .expect("No app with this dna hash");

        let input = GetNotificationInput {
            notification_hash: AnyDhtHash::from(hrl_body.dht_hash),
            locale: String::from("sv"),
        };
        let (nonce, expires_at) = fresh_nonce(Timestamp::now()).expect("Could not create nonce");

        let zome_call_unsigned = ZomeCallUnsigned {
            provenance: cell_id.agent_pubkey().clone(),
            cell_id,
            zome_name: ZomeName::from("alerts"), // TODO: remove hardcoded zome name
            fn_name: FunctionName::from("get_notification"),
            cap_secret: None,
            payload: ExternIO::encode(input).expect("Could not encode get notification input"),
            nonce,
            expires_at,
        };

        let zome_call =
            sign_zome_call_with_client(zome_call_unsigned, &meta_lair_client.lair_client())
                .await
                .expect("Could not sign zome call");

        // Hardcoded zome name
        // TODO: add HRL resolver to get which zome to call fetch_notification on?
        let response = app_ws
            .call_zome(zome_call)
            .await
            .expect("Failed to call zome");
        let maybe_pending_notification: Option<Notification> = response
            .decode()
            .expect("Failed to decode zome call response");

        let pending_notification =
            maybe_pending_notification.expect("Pending notification is none");

        let mut notification = NotificationData::default();

        notification.title = Some(pending_notification.title);
        //  {

        //     title,
        //     body,
        //     extra: hrl,
        // };

        notification
    })
}
