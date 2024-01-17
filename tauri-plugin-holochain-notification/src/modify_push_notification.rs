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

use serde::{Deserialize, Serialize};
use tauri_plugin_holochain::{launch, RunningHolochainInfo};
use tauri_plugin_notification::*;

use jni::objects::JClass;
use jni::JNIEnv;

#[derive(Serialize, Deserialize, Debug)]
pub struct NotificationWithHash {
    pub notification_hash: AnyDhtHash,
    pub hrl_to_navigate_to: Hrl,
}

#[tauri_plugin_notification::modify_push_notification]
pub fn modify_push_notification(notification: NotificationData) -> NotificationData {
    tauri::async_runtime::block_on(async move {
        match modify(notification).await {
            Ok(n) => n,
            Err(err) => {
                log::error!("Error modifying the push notification {err:?}");
                let mut n = NotificationData::default();
                n.title = Some(String::from("Error fetching notification"));
                n.large_body = Some(format!("{err:?}"));
                n
            }
        }
    })
}

async fn modify(notification: NotificationData) -> crate::Result<NotificationData> {
    let body = notification
        .body
        .ok_or(crate::Error::ModifyNotificationError(
            "Empty notification body".into(),
        ))?;

    let hrl_body: Hrl = serde_json::from_str(body.as_str()).map_err(|err| {
        crate::Error::ModifyNotificationError(String::from("Malformed notification body"))
    })?;

    let info = launch().await.map_err(|err| {
        crate::Error::ModifyNotificationError(String::from("Failed to run holochain"))
    })?;

    let mut admin_ws = AdminWebsocket::connect(format!("ws://localhost:{}", info.admin_port))
        .await
        .map_err(|err| {
            crate::Error::ModifyNotificationError(String::from(
                "Could not connect to admin interface",
            ))
        })?;
    let mut app_ws = AppWebsocket::connect(format!("ws://localhost:{}", info.app_port))
        .await
        .map_err(|err| {
            crate::Error::ModifyNotificationError("Could not connect to app interface".into())
        })?;

    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| crate::Error::ModifyNotificationError("Failed to list apps".into()))?;

    let dna_hash = DnaHash::from(hrl_body.dna_hash);

    let cell_id = apps
        .into_iter()
        .find_map(|app_info| {
            app_info.cell_info.values().find_map(|cells| {
                cells.iter().find_map(|cell_info| match cell_info {
                    CellInfo::Provisioned(cell) => match cell.cell_id.dna_hash().eq(&dna_hash) {
                        true => Some(cell.cell_id.clone()),
                        false => None,
                    },
                    CellInfo::Cloned(cell) => match cell.cell_id.dna_hash().eq(&dna_hash) {
                        true => Some(cell.cell_id.clone()),
                        false => None,
                    },
                    _ => None,
                })
            })
        })
        .ok_or(crate::Error::ModifyNotificationError(
            "No app with this dna hash".into(),
        ))?;

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
                get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone()).await;
        }
    }
    match maybe_pending_notification {
        Ok(Some(_)) => {}
        _ => {
            std::thread::sleep(std::time::Duration::from_secs(1));

            maybe_pending_notification =
                get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone()).await;
        }
    }
    match maybe_pending_notification {
        Ok(Some(_)) => {}
        _ => {
            std::thread::sleep(std::time::Duration::from_secs(1));

            maybe_pending_notification =
                get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone()).await;
        }
    }
    match maybe_pending_notification {
        Ok(Some(_)) => {}
        _ => {
            std::thread::sleep(std::time::Duration::from_secs(1));

            maybe_pending_notification =
                get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone()).await;
        }
    }
    match maybe_pending_notification {
        Ok(Some(_)) => {}
        _ => {
            std::thread::sleep(std::time::Duration::from_secs(1));

            maybe_pending_notification =
                get_pending_notification(&info, &mut app_ws, cell_id.clone(), input.clone()).await;
        }
    }

    let pending_notification = maybe_pending_notification
        .map_err(|err| {
            crate::Error::ModifyNotificationError("Error getting the pending notification".into())
        })?
        .ok_or(crate::Error::ModifyNotificationError(
            "Pending notification is none".into(),
        ))?;

    log::info!("Pending notification {pending_notification:?}");

    let mut notification = NotificationData::default();

    notification.title = Some(pending_notification.title);
    notification.summary = Some(pending_notification.body.clone());
    notification.large_body = Some(pending_notification.body.clone());
    // notification.body = Some(pending_notification.body);

    let nwithhash = NotificationWithHash {
        notification_hash,
        hrl_to_navigate_to: pending_notification.hrl_to_navigate_to_on_click.hrl,
    };

    let hrl: String = serde_json::to_string(&nwithhash).map_err(|err| {
        crate::Error::ModifyNotificationError("could not serialize notification with hash".into())
    })?;

    notification.body = Some(hrl);

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

    Ok(notification)
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

    let zome_call = sign_zome_call_with_client(zome_call_unsigned, &info.lair_client.clone())
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
