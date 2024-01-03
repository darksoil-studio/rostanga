use std::{collections::HashMap, path::PathBuf, sync::Mutex, time::Duration};

use fcm_v1::{
    android::{AndroidConfig, AndroidNotification},
    apns::ApnsConfig,
    auth::Authenticator,
    message::{Message, Notification},
    Client,
};
use hc_zome_notifications_provider_fcm_types::NotifyAgentSignal;
use holochain_client::AppInfo;
use holochain_types::{
    prelude::{AppBundle, ExternIO, FunctionName, RoleName, ZomeName},
    signal::Signal,
};
use serde_json::{Map, Value};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};
use tauri_plugin_cli::CliExt;

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;
mod modify_push_notification;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::HolochainNotification;
#[cfg(mobile)]
use mobile::HolochainNotification;
use tauri_plugin_holochain::HolochainExt;
use yup_oauth2::ServiceAccountKey;

#[derive(Default)]
struct MyState(Mutex<HashMap<String, String>>);

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the holochain-notification APIs.
pub trait HolochainNotificationExt<R: Runtime> {
    fn holochain_notification(&self) -> &HolochainNotification<R>;
}

impl<R: Runtime, T: Manager<R>> crate::HolochainNotificationExt<R> for T {
    fn holochain_notification(&self) -> &HolochainNotification<R> {
        self.state::<HolochainNotification<R>>().inner()
    }
}

async fn install_app_if_not_present<R: Runtime>(
    app_handle: &AppHandle<R>,
    app_id: String,
    app_bundle: AppBundle,
) -> crate::Result<()> {
    let mut admin_ws = app_handle.holochain().admin_websocket().await?;

    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|err| crate::Error::ConductorApiError(err))?;

    if let None = apps.iter().find(|app| app.installed_app_id.eq(&app_id)) {
        app_handle
            .holochain()
            .install_app(app_id, app_bundle, HashMap::new(), None)
            .await?;
    }
    Ok(())
}

async fn install_initial_apps<R: Runtime>(
    app: &AppHandle<R>,
    notifications_provider_app_id: String,
    notifications_provider_recipient_app_id: String,
) -> crate::Result<()> {
    #[cfg(not(mobile))]
    {
        let provider_app_bundle = AppBundle::decode(include_bytes!(
            "../../workdir/notifications_provider_fcm.happ"
        ))
        .unwrap();
        install_app_if_not_present(&app, notifications_provider_app_id, provider_app_bundle)
            .await?;
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

/// Initializes the plugin.
pub fn init<R: Runtime>(
    fcm_project_id: String,
    notifications_provider_app_id: String,
    notifications_provider_recipient_app_id: String,
) -> TauriPlugin<R> {
    Builder::new("holochain-notification")
        .invoke_handler(tauri::generate_handler![commands::execute])
        .setup(|app, api| {
            #[cfg(mobile)]
            let holochain_notification = mobile::init(app, api)?;
            #[cfg(desktop)]
            let holochain_notification = desktop::init(app, api)?;
            app.manage(holochain_notification);

            let recipient_app_id = notifications_provider_recipient_app_id.clone();
            let provider_app_id = notifications_provider_app_id.clone();

            tauri::async_runtime::block_on(async move {
                install_initial_apps(&app, notifications_provider_app_id, 
                    recipient_app_id).await
            })?;

            #[cfg(desktop)]
            {
                let args = app.cli().matches().expect("Can't get matches").args;

                // Get service account key argument
                // Publish to fcm notifications provider app
                if let Some(m) = args.get("service-account-key") {
                    if let Value::String(s) = m.value.clone() {
                        let result: crate::Result<()> =
                            tauri::async_runtime::block_on(async move {
                                let service_account_key_path = PathBuf::from(s);
                                let service_account_key = yup_oauth2::read_service_account_key(
                                    service_account_key_path.clone(),
                                )
                                .await
                                .expect("Failed to read service account key");

                                let mut app_agent_ws = app
                                    .holochain()
                                    .app_agent_websocket(notifications_provider_recipient_app_id)
                                    .await?;

                                let payload = ExternIO::encode(service_account_key)
                                    .expect("Could not encode service account key");

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
                            });
                        result.expect("Failed to upload the service account key");
                        println!("Successfully uploaded new service account key");
                        std::process::exit(0);
                    }
                }

                let h = app.clone();
                tauri::async_runtime::spawn(async move {
                    let mut app_agent_websocket = h
                        .holochain()
                        .app_agent_websocket(provider_app_id)
                        .await
                        .expect("Failed to connect to holochain");

                    app_agent_websocket.on_signal(move |signal| {
let Signal::App {  signal , ..} = signal else {
                            return ();
                        };

                        let Ok(notify_agent_signal) = 
                            signal.into_inner().decode::<NotifyAgentSignal>() else {
                            return ();
                        };

                        let fcm_project_id = fcm_project_id.clone();
                tauri::async_runtime::block_on(async move {
                        let service_account_key = into(notify_agent_signal.service_account_key);

                            let body = HrlBody::
                                try_from(notify_agent_signal.notification).expect("Could not deserialize ");

                            let str_body = serde_json::to_string(&body).expect("Could not serialize body");

                            send_push_notification(
                                fcm_project_id, 
                                service_account_key, 
                                notify_agent_signal.token, 
                                String::from(""),  str_body)
                                .await.expect("Failed to send push notification")

                    });

                }).await.expect("Failed to set up on signal");
                });
            }

            #[cfg(mobile)]
            {
                let h = app.app_handle().clone();

                app.listen_global("notification-action-performed", move |event| {
                    if let Ok(notification_action_performed_payload) =
                        serde_json::from_str::<
                            tauri_plugin_notification::NotificationActionPerformedPayload,
                        >(event.payload())
                    {
                        log!(
                            "Notification action performed: {:?}",
                            notification_action_performed_payload
                        );
                    }
                });
            }

            #[cfg(mobile)]
            {
                let h = app.app_handle().clone();

                app.listen_global("new-fcm-token", move |event| {
                    if let Ok(token) = serde_json::from_str::<String>(event.payload()) {
                        tauri::async_runtime::block_on(async move {
                            log!("new-fcm-token {:?}", token);

                            let lair_client = h.holochain().lair_client.lair_client();

                            let app_ws = AppAgentWebsocket::connect(
                                format!("ws://localhost:{}", h.holochain().runtime_info.app_port),
                                notifications_recipient_app_id,
                                lair_client,
                            )
                            .await
                            .expect("Could not connect to holochain");

                            let payload =
                                ExternIO::encode(token).expect("Could not encode FCM token");

                            app_ws
                                .call_zome_fn(
                                    RoleName::from("notifications"),
                                    ZomeName::from("notifications_provider_fcm_recipient"),
                                    FunctionName::from("register_new_fcm_token"),
                                    payload,
                                )
                                .await
                                .expect("Failed to register new FCM token");
                        });
                    }
                });
            }

            // manage state so it is accessible by the commands
            app.manage(MyState::default());
            Ok(())
        })
        .build()
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
