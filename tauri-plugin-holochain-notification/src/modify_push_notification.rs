use std::collections::HashMap;

use hc_zome_trait_pending_notifications::{GetNotificationInput, Notification};
use holochain_client::{sign_zome_call_with_client, AdminWebsocket, AppWebsocket};
use holochain_conductor_api::CellInfo;
use holochain_nonce::fresh_nonce;
use holochain_types::{
    prelude::{
        AnyDhtHash, AnyDhtHashB64, AppBundle, CellId, DnaHash, DnaHashB64, ExternIO, FunctionName,
        Timestamp, ZomeCallUnsigned, ZomeName,
    },
    web_app::WebAppBundle,
};
use hrl::Hrl;

use tauri_plugin_holochain::{launch, RunningHolochainInfo};
use tauri_plugin_notification::*;

use jni::objects::JClass;
use jni::JNIEnv;

#[tauri_plugin_notification::modify_push_notification]
pub fn modify_push_notification(notification: NotificationData) -> NotificationData {
    tauri::async_runtime::block_on(async move {
        let body = notification.body.expect("EMPTY NOTIFICATION BODY");

        let hrl_body: Hrl =
            serde_json::from_str(body.as_str()).expect("Malformed notification body");

        let info = launch().await.expect("Failed to launch holochain");

        let mut admin_ws = AdminWebsocket::connect(format!("ws://localhost:{}", info.admin_port))
            .await
            .expect("Could not connect to admin interface");
        let mut app_ws = AppWebsocket::connect(format!("ws://localhost:{}", info.app_port))
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

        let notification_hash = AnyDhtHash::from(hrl_body.resource_hash);

        let input = GetNotificationInput {
            notification_hash: notification_hash.clone(),
            locale: String::from("sv"),
        };

        let mut maybe_pending_notification =
            get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone()).await;

        match maybe_pending_notification {
            Ok(Some(_)) => {}
            _ => {
                std::thread::sleep(std::time::Duration::from_secs(1));

                maybe_pending_notification =
                    get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone())
                        .await;
            }
        }
        match maybe_pending_notification {
            Ok(Some(_)) => {}
            _ => {
                std::thread::sleep(std::time::Duration::from_secs(1));

                maybe_pending_notification =
                    get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone())
                        .await;
            }
        }
        match maybe_pending_notification {
            Ok(Some(_)) => {}
            _ => {
                std::thread::sleep(std::time::Duration::from_secs(1));

                maybe_pending_notification =
                    get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone())
                        .await;
            }
        }
        match maybe_pending_notification {
            Ok(Some(_)) => {}
            _ => {
                std::thread::sleep(std::time::Duration::from_secs(1));

                maybe_pending_notification =
                    get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone())
                        .await;
            }
        }
        match maybe_pending_notification {
            Ok(Some(_)) => {}
            _ => {
                std::thread::sleep(std::time::Duration::from_secs(1));

                maybe_pending_notification =
                    get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone())
                        .await;
            }
        }

        let pending_notification = maybe_pending_notification
            .expect("Error getting the pending notification")
            .expect("Pending notification is none");

        log::info!("Pending notification {pending_notification:?}");

        let mut notification = NotificationData::default();

        notification.title = Some(pending_notification.title);
        notification.body = Some(pending_notification.body);

        let hrl: String = pending_notification.hrl_to_navigate_to_on_click.hrl.into();

        let attachment = Attachment::new(
            AnyDhtHashB64::from(notification_hash).to_string(),
            url::Url::parse(hrl.as_str()).expect("Could not parse hrl as url"),
        );

        notification.attachments.push(attachment);

        // TODO: why does putting things in the extra not work? Bug report:
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: java.lang.Error: v1.a: Unrecognized field "hrl" (class app.tauri.plugin.JSObject), not marked as ignorable (0 known properties: ])
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime:  at [Source: REDACTED (`StreamReadFeature.INCLUDE_SOURCE_IN_LOCATION` disabled); line: 1, column: 289] (through reference chain: app.tauri.notification.Notification["extra"]->app.tauri.plugin.JSObject["hrl"])
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: 	at java.util.concurrent.ThreadPoolExecutor.runWorker(ThreadPoolExecutor.java:1173)
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: 	at java.util.concurrent.ThreadPoolExecutor$Worker.run(ThreadPoolExecutor.java:641)
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: 	at o2.o.run(SourceFile:26)
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: 	at java.lang.Thread.run(Thread.java:919)
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: Caused by: v1.a: Unrecognized field "hrl" (class app.tauri.plugin.JSObject), not marked as ignorable (0 known properties: ])
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime:  at [Source: REDACTED (`StreamReadFeature.INCLUDE_SOURCE_IN_LOCATION` disabled); line: 1, column: 289] (through reference chain: app.tauri.notification.Notification["extra"]->app.tauri.plugin.JSObject["hrl"])
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: 	at s1.e.D0(SourceFile:91)
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: 	at s1.e.E0(Unknown Source:28)
        // 01-15 16:08:52.147  5779  6119 E AndroidRuntime: 	at s1.d.R0(Unknown Source:41)
        // let mut map: HashMap<String, serde_json::Value> = HashMap::new();
        // map.insert(
        //     String::from("hrl"),
        //     serde_json::Value::String(pending_notification.hrl_to_navigate_to_on_click.hrl.into()),
        // );

        // let notification_hash_b64 = AnyDhtHashB64::from(notification_hash);
        // map.insert(
        //     String::from("notification"),
        //     serde_json::Value::String(notification_hash_b64.to_string()),
        // );

        // notification.extra = map;

        notification
    })
}

async fn get_pending_notification(
    info: &RunningHolochainInfo,
    app_ws: &mut AppWebsocket,
    cell_id: CellId,
    input: GetNotificationInput,
) -> crate::Result<Option<Notification>> {
    let (nonce, expires_at) = fresh_nonce(Timestamp::now()).expect("Could not create nonce");

    let zome_call_unsigned = ZomeCallUnsigned {
        provenance: cell_id.agent_pubkey().clone(),
        cell_id,
        zome_name: ZomeName::from("gather"), // TODO: remove hardcoded zome name
        fn_name: FunctionName::from("get_notification"),
        cap_secret: None,
        payload: ExternIO::encode(input).expect("Could not encode get notification input"),
        nonce,
        expires_at,
    };

    let zome_call = sign_zome_call_with_client(zome_call_unsigned, &info.lair_client.lair_client())
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
    Ok(maybe_pending_notification)
}
