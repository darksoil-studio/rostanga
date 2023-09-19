use std::collections::HashMap;
use std::path::PathBuf;

use holochain_types::web_app::WebAppBundle;
use serde_json::Value;
use tauri::{WindowBuilder, WindowUrl};
#[cfg(desktop)]
use tauri_plugin_cli::CliExt;
use tauri_plugin_holochain::HolochainExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut initial_apps: HashMap<String, WebAppBundle> = HashMap::new();

    initial_apps.insert(
        String::from("gather"),
        WebAppBundle::decode(include_bytes!("../../gather.webhapp")).unwrap(),
    );

    tauri::Builder::default()
        .setup(|app| {
            let mut subfolder = PathBuf::from("holochain");

            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_cli::init())?;
            #[cfg(desktop)]
            if let Some(m) = app
                .cli()
                .matches()
                .expect("Can't get matches")
                .args
                .get("profile")
            {
                if let Value::String(s) = m.value.clone() {
                    subfolder = PathBuf::from(s);
                }
            }

            let config = tauri_plugin_holochain::TauriPluginHolochainConfig {
                initial_apps,
                subfolder,
            };
            // WindowBuilder::new(
            //     app.handle(),
            //     String::from("main"),
            //     WindowUrl::App(PathBuf::from("index.html")),
            // )
            // .build()?;

            // WindowBuilder::new(
            //     app.handle(),
            //     String::from("main2"),
            //     WindowUrl::App(PathBuf::from("index.html")),
            // )
            // .build()?;

            app.handle().plugin(tauri_plugin_holochain::init(config))?;
            app.open_app(String::from("gather")).unwrap();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
