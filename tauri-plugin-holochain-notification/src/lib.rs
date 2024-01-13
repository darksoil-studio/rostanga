use std::{collections::HashMap, fs::canonicalize, path::PathBuf, sync::Mutex, time::Duration};

use fcm_v1::{
    android::AndroidConfig, apns::ApnsConfig, auth::Authenticator, message::Message, Client,
};

use hc_zome_notifications_provider_fcm_types::{NotifyAgentSignal, RegisterFCMTokenInput};
use holochain_client::{AppAgentWebsocket, AppInfo};
use holochain_types::{
    dna::{AnyDhtHash, AnyDhtHashB64},
    prelude::{AppBundle, ExternIO, FunctionName, RoleName, ZomeName},
    signal::Signal,
};
use hrl::Hrl;
use serde_json::{Map, Value};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[cfg(desktop)]
use tauri_plugin_cli::CliExt;

pub use models::*;

// #[cfg(desktop)]
// mod desktop;
// #[cfg(mobile)]
// mod mobile;

mod commands;
mod error;
mod models;
mod modify_push_notification;

pub use error::{Error, Result};

// #[cfg(desktop)]
// use desktop::HolochainNotification;
// #[cfg(mobile)]
// use mobile::HolochainNotification;
use tauri_plugin_holochain::HolochainExt;
use tauri_plugin_notification::{NotificationExt, PermissionState};
use yup_oauth2::ServiceAccountKey;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the holochain-notification APIs.
// pub trait HolochainNotificationExt<R: Runtime> {
//     fn holochain_notification(&self) -> &HolochainNotification<R>;
// }

// impl<R: Runtime, T: Manager<R>> crate::HolochainNotificationExt<R> for T {
//     fn holochain_notification(&self) -> &HolochainNotification<R> {
//         self.state::<HolochainNotification<R>>().inner()
//     }
// }

async fn install_app_if_not_present<R: Runtime>(
    app_handle: &AppHandle<R>,
    app_id: String,
    app_bundle: AppBundle,
) -> crate::Result<Option<AppInfo>> {
    let mut admin_ws = app_handle.holochain()?.admin_websocket().await?;

    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    match apps.iter().find(|app| app.installed_app_id.eq(&app_id)) {
        None => Ok(Some(
            app_handle
                .holochain()?
                .install_app(app_id, app_bundle, HashMap::new(), None)
                .await?,
        )),
        _ => Ok(None),
    }
}

async fn install_initial_apps<R: Runtime>(
    app: &AppHandle<R>,
    notifications_provider_app_id: String,
    notifications_provider_recipient_app_id: String,
) -> crate::Result<()> {
    // #[cfg(not(mobile))]
    {
        let provider_app_bundle = AppBundle::decode(include_bytes!(
            "../../workdir/notifications_provider_fcm.happ"
        ))
        .unwrap();
        if let Some(_app_info) = install_app_if_not_present(
            &app,
            notifications_provider_app_id.clone(),
            provider_app_bundle,
        )
        .await?
        {

            //     let mut app_agent_websocket = app.holochain().app_agent_websocket(notifications_provider_app_id).await?;
            //     app_agent_websocket.call_zome(
            //     "notifications".into(),
            //     ZomeName::from("notifications"),
            //     FunctionName::from("announce_as_provider"),
            //      ExternIO::encode(()).unwrap()
            // ).await.map_err(|err| crate::Error::ConductorApiError(err))?;
        }
    }

    #[cfg(mobile)]
    {
        let recipient_app_bundle = AppBundle::decode(include_bytes!(
            "../../workdir/notifications_fcm_recipient.happ"
        ))
        .unwrap();
        install_app_if_not_present(
            &app,
            notifications_provider_recipient_app_id,
            recipient_app_bundle,
        )
        .await?;
    }

    Ok(())
}

// /// Initializes the plugin.
// pub fn init<R: Runtime>() -> TauriPlugin<R> {
//     Builder::new("holochain-notification").build()
// }

pub async fn setup_notifications<R: Runtime>(
    app: AppHandle<R>,
    fcm_project_id: String,
    notifications_provider_app_id: String,
    notifications_provider_recipient_app_id: String,
) -> crate::Result<()> {
    setup_tauri_notifications(&app)?;
    install_initial_apps(
        &app,
        notifications_provider_app_id.clone(),
        notifications_provider_recipient_app_id.clone(),
    )
    .await?;

    let provider_app_id = notifications_provider_app_id.clone();
    let recipient_app_id = notifications_provider_recipient_app_id.clone();

    #[cfg(desktop)]
    {
        let args = app.cli().matches().expect("Can't get matches").args; // TODO: fix this so that the app doesn't have to configure it

        // Get service account key argument
        // Publish to fcm notifications provider app
        if let Some(m) = args.get("service-account-key") {
            if let Value::String(s) = m.value.clone() {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let mut app_agent_ws = app
                        .holochain()
                        .expect("Holochain was not yet initialized")
                        .app_agent_websocket(provider_app_id)
                        .await
                        .expect("Failed to connect with holochain");

                    match publish_service_account_key(&mut app_agent_ws, PathBuf::from(s)).await {
                        Ok(_) => {
                            log::info!("Successfully uploaded new service account key");
                            std::process::exit(0);
                        }
                        Err(err) => {
                            log::error!("Failed to upload new service account key: {err:?}");
                            std::process::exit(1);
                        }
                    }
                });
            }
        }
        let provider_app_id = notifications_provider_app_id.clone();

        let h = app.clone();
        let mut app_agent_websocket = h
            .holochain()?
            .app_agent_websocket(provider_app_id)
            .await
            .expect("Failed to connect to holochain");

        app_agent_websocket
            .on_signal(move |signal| {
                let Signal::App { signal, .. } = signal else {
                    return ();
                };

                let Ok(notify_agent_signal) = signal.into_inner().decode::<NotifyAgentSignal>()
                else {
                    return ();
                };

                let fcm_project_id = fcm_project_id.clone();
                tauri::async_runtime::spawn(async move {
                    let service_account_key = into(notify_agent_signal.service_account_key);

                    let body = Hrl::try_from(notify_agent_signal.notification)
                        .expect("Could not deserialize hrl");

                    let str_body = serde_json::to_string(&body).expect("Could not serialize body");

                    send_push_notification(
                        fcm_project_id,
                        service_account_key,
                        notify_agent_signal.token,
                        String::from(""),
                        str_body,
                    )
                    .await
                    .expect("Failed to send push notification")
                });
            })
            .await
            .expect("Failed to set up on signal");
    }

    #[cfg(mobile)]
    {
        let h = app.app_handle().clone();

        app.listen_global("notification-action-performed", move |event| {
            if let Ok(notification_action_performed_payload) = serde_json::from_str::<
                tauri_plugin_notification::NotificationActionPerformedPayload,
            >(event.payload())
            {
                let h = h.clone();
                tauri::async_runtime::spawn(async move {
                    log::info!(
                        "Notification action performed: {:?}",
                        notification_action_performed_payload
                    );

                    let notification_data = notification_action_performed_payload.notification;

                    let extra = notification_data.extra;

                    if let Some(serde_json::Value::String(notification_hash_b64)) =
                        extra.get("notification")
                    {
                        // TODO: remove this hardcoded stuff
                        let mut app_agent_ws = h
                            .holochain()
                            .expect("Holochain was not yet initialized")
                            .app_agent_websocket("gather".into())
                            .await
                            .expect("Failed to connect to holochain");

                        let notification_hash = AnyDhtHash::from(
                            AnyDhtHashB64::from_b64_str(notification_hash_b64.as_str())
                                .expect("Could not convert notification hash"),
                        );

                        let _response = app_agent_ws
                            .call_zome(
                                "gather".into(),
                                ZomeName::from("gather"),
                                FunctionName::from("mark_notification_as_read"),
                                ExternIO::encode(notification_hash)
                                    .expect("Could not encode notification hash"),
                            )
                            .await
                            .expect("Failed to call zome");
                    }

                    if let Some(serde_json::Value::String(hrl)) = extra.get("hrl") {
                        if let Ok(hrl) = Hrl::try_from(hrl.clone()) {
                            h.holochain()
                                .expect("Holochain was not yet initialized")
                                .open_hrl(hrl)
                                .await
                                .expect("Could not open Hrl");
                        }
                    }
                });
            }
        });

        let provider_app_id = notifications_provider_app_id.clone();
        let recipient_app_id = notifications_provider_recipient_app_id.clone();
        let h = app.app_handle().clone();
        app.listen_global("new-fcm-token", move |event| {
            let recipient_app_id = recipient_app_id.clone();
            let provider_app_id = provider_app_id.clone();
            let h = h.clone();
            if let Ok(token) = serde_json::from_str::<String>(event.payload()) {
                tauri::async_runtime::spawn(async move {
                    log::info!("new-fcm-token {:?}", token);
                    shortcut_publish_new_fcm_token(h, provider_app_id, token)
                        .await
                        .expect("Failed to publish new fcm token");
                });
            }
        });

        let recipient_app_id = notifications_provider_recipient_app_id.clone();
        let provider_app_id = notifications_provider_app_id.clone();
        let h = app.app_handle().clone();
        let token = app
            .notification()
            .register_for_push_notifications()
            .expect("Could not register for push notifications");
        shortcut_publish_new_fcm_token(h, provider_app_id, token)
            .await
            .expect("Failed to publish new fcm token");
        app.emit("holochain-notifications-setup-complete", ())?;
    }

    Ok(())
}

fn setup_tauri_notifications<R: Runtime>(
    app_handle: &AppHandle<R>,
) -> tauri_plugin_notification::Result<()> {
    // let app_handle = app_handle.clone();

    // let permissions_state = app_handle.notification().permission_state()?;
    // if let PermissionState::Unknown = permissions_state {
    //     app_handle.notification().request_permission()?;
    // }
    // let h = app_handle.clone();

    // if let PermissionState::Granted = permissions_state {
    //     // h.notification()
    //     //     .create_channel(tauri_plugin_notification::Channel::builder("test", "test").build())
    //     //     .expect("Failed to create channel");
    //     // let r = app.background_tasks().schedule_background_task(
    //     //     ScheduleBackgroundTaskRequest {
    //     //         label: String::from("hi"),
    //     //         interval: 1,
    //     //     },
    //     //     move || {
    //     //         h.notification()
    //     //             .builder()
    //     //             .channel_id("test")
    //     //             .title("Hey!")
    //     //             .show()
    //     //             .expect("Failed to send notification");
    //     //     },
    //     // )?;
    // }

    Ok(())
}

async fn publish_new_fcm_token<R: Runtime>(
    app_handle: AppHandle<R>,
    recipient_app_id: String,
    token: String,
) -> crate::Result<()> {
    let mut app_agent_ws = app_handle
        .holochain()?
        .app_agent_websocket(recipient_app_id)
        .await?;

    let payload = ExternIO::encode(token).expect("Failed to encode token");

    app_agent_ws
        .call_zome(
            RoleName::from("notifications"),
            ZomeName::from("notifications_provider_fcm_recipient"),
            FunctionName::from("register_new_fcm_token"),
            payload,
        )
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    Ok(())
}

async fn shortcut_publish_new_fcm_token<R: Runtime>(
    app_handle: AppHandle<R>,
    provider_app_id: String,
    token: String,
) -> crate::Result<()> {
    let mut app_agent_ws = app_handle
        .holochain()?
        .app_agent_websocket(provider_app_id)
        .await?;

    let mut admin_ws = app_handle.holochain()?.admin_websocket().await?;
    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    let gather = apps
        .into_iter()
        .find(|app| app.installed_app_id.as_str() == "gather")
        .expect("Could not find gather");

    let payload = ExternIO::encode(RegisterFCMTokenInput {
        token,
        agent: gather.agent_pub_key.clone(),
    })
    .expect("Failed to encode token");

    app_agent_ws
        .call_zome(
            RoleName::from("notifications_provider_fcm"),
            ZomeName::from("notifications_provider_fcm"),
            FunctionName::from("register_fcm_token_for_agent"),
            payload,
        )
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    log::info!("Successfully published new fcm token");

    Ok(())
}

fn into(key: hc_zome_notifications_provider_fcm_types::ServiceAccountKey) -> ServiceAccountKey {
    ServiceAccountKey {
        key_type: key.key_type,
        project_id: key.project_id,
        private_key_id: key.private_key_id,
        private_key: key.private_key,
        client_email: key.client_email,
        client_id: key.client_id,
        auth_uri: key.auth_uri,
        token_uri: key.token_uri,
        auth_provider_x509_cert_url: key.auth_provider_x509_cert_url,
        client_x509_cert_url: key.client_x509_cert_url,
    }
}

async fn send_push_notification(
    fcm_project_id: String,
    service_account_key: ServiceAccountKey,
    token: String,
    title: String,
    body: String,
) -> Result<()> {
    let auth = Authenticator::service_account::<String>(service_account_key)
        .await
        .expect("Failed to read service account");

    let client = Client::new(auth, fcm_project_id, false, Duration::from_secs(2));

    let mut message = Message::default();

    let mut map = HashMap::new();
    map.insert("title".to_string(), Value::String(title.clone()));
    map.insert("body".to_string(), Value::String(body.clone()));
    message.data = Some(map.clone());
    let mut apns_config = ApnsConfig::default();

    let mut alert_data = Map::new();
    alert_data.insert("title".to_string(), Value::String(title.clone()));
    alert_data.insert("body".to_string(), Value::String(body.clone()));

    let mut aps_data = Map::new();
    aps_data.insert("alert".to_string(), Value::Object(alert_data.clone()));
    aps_data.insert("mutable-content".to_string(), Value::Number(1.into()));
    let mut apns_data = HashMap::new();
    apns_data.insert("aps".to_string(), Value::Object(aps_data));
    apns_config.payload = Some(apns_data);

    message.apns = Some(apns_config);

    let mut android_config = AndroidConfig::default();
    android_config.data = Some(map);

    message.android = Some(android_config);
    message.token = Some(token);

    client.send(&message).await.expect("Failed to send message");

    Ok(())
}

async fn publish_service_account_key(
    app_agent_ws: &mut AppAgentWebsocket,
    service_account_key_path: PathBuf,
) -> crate::Result<()> {
    println!(
        "before {:?}",
        std::env::current_dir()?.join(&service_account_key_path.clone())
    );
    let absolute_path =
        canonicalize(std::env::current_dir()?.join(&service_account_key_path.clone()))
            .expect("Could not canonicalize path");
    println!("Reading service account key: {absolute_path:?}");
    let service_account_key = yup_oauth2::read_service_account_key(absolute_path)
        .await
        .expect("Failed to read service account key");

    let payload =
        ExternIO::encode(service_account_key).expect("Could not encode service account key");

    app_agent_ws
        .call_zome(
            RoleName::from("notifications_provider_fcm"),
            ZomeName::from("notifications_provider_fcm"),
            FunctionName::from("publish_new_service_account_key"),
            payload,
        )
        .await
        .expect("Failed to upload the service account key");

    Ok(())
}
