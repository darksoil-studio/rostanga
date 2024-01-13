use hyper::{
    service::{make_service_fn, service_fn},
    Body, Response, Server, StatusCode,
};
use std::net::SocketAddr;
use tauri::{AppHandle, Manager, Runtime};

use crate::{
    error::{Error, Result},
    filesystem::FileSystem,
    HolochainExt, HolochainRuntimeInfo,
};

pub fn pong_iframe() -> String {
    format!("<html><head></head><body><script>window.onload = () => window.parent.postMessage('pong', '*') </script></body></html>")
}

pub fn window_html() -> String {
    include_str!("../ui/dist/index.html").into()
}

pub fn start_http_server<R: Runtime>(app_handle: AppHandle<R>, ui_server_port: u16) -> () {
    tauri::async_runtime::spawn(async move {
        let addr: SocketAddr = ([127, 0, 0, 1], ui_server_port).into();
        let app_handle = app_handle.clone();
        // The closure inside `make_service_fn` is run for each connection,
        // creating a 'service' to handle requests for that specific connection.
        let make_service = make_service_fn(move |_| {
            let app_handle = app_handle.clone();
            // While the state was moved into the make_service closure,
            // we need to clone it here because this closure is called
            // once for every connection.
            //
            // Each connection could send multiple requests, so
            // the `Service` needs a clone to handle later requests.
            async move {
                // This is the `Service` that will handle the connection.
                // `service_fn` is a helper to convert a function that
                // returns a Response into a `Service`.
                let app_handle = app_handle.clone();
                Ok::<_, hyper::Error>(service_fn(move |request| {
                    let app_handle = app_handle.clone();
                    async move {
                        let app_handle = app_handle.clone();
                        let host = request
                            .headers()
                            .get("host")
                            .ok_or(Error::HttpServerError(String::from("URI has no host")))?
                            .clone()
                            .to_str()
                            .map_err(|err| Error::HttpServerError(format!("{:?}", err)))?
                            .to_string();

                        if host.starts_with("ping.localhost") {
                            let r: Result<Response<Body>> = Ok(Response::builder()
                                .status(202)
                                .header("content-type", "text/html")
                                .body(pong_iframe().into())
                                .map_err(|err| Error::HttpServerError(format!("{:?}", err)))?);
                            return r;
                        }
                        if host.starts_with("localhost") {
                            let r: Result<Response<Body>> = Ok(Response::builder()
                                .status(202)
                                .header("content-type", "text/html")
                                .body(window_html().into())
                                .map_err(|err| Error::HttpServerError(format!("{:?}", err)))?);
                            return r;
                        }

                        let split_host: Vec<String> =
                            host.split(".").into_iter().map(|s| s.to_string()).collect();
                        let lowercase_app_id = split_host.get(0).expect("Failed to get the app id");

                        let file_name = request.uri().path();

                        let Ok(holochain) = app_handle.holochain() else {
                            return Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(
                                    format!("Called http UI before initializing holochain")
                                        .as_bytes()
                                        .to_vec()
                                        .into(),
                                )
                                .expect("Failed to return error"));
                        };

                        let r: Result<Response<Body>> = match read_asset(
                            &holochain.filesystem,
                            &lowercase_app_id,
                            file_name.to_string(),
                        )
                        .await
                        {
                            Ok(Some((asset, mime_type))) => {
                                let mut response_builder = Response::builder().status(202);
                                if let Some(mime_type) = mime_type {
                                    response_builder =
                                        response_builder.header("content-type", mime_type);
                                }

                                Ok(response_builder
                                    .body(asset.into())
                                    .expect("Failed to build body of resposne"))
                            }
                            Ok(None) => Ok(Response::builder()
                                .status(404)
                                .body(vec![].into())
                                .map_err(|err| Error::HttpServerError(format!("{:?}", err)))?),
                            Err(e) => Ok(Response::builder()
                                .status(500)
                                .body(format!("{:?}", e).into())
                                .expect("Failed to build body of error response")),
                        };
                        // admin_ws.close();
                        r
                    }
                }))
            }
        });

        // let app_handle = &app_handle;
        // let make_svc = make_service_fn(|_conn| async {
        //     // service_fn converts our function into a `Service`
        //     Ok::<_, Infallible>(service_fn(|request: Request<Body>| async move {
        //         }
        // })
        // });

        let server = Server::bind(&addr).serve(make_service);
        if let Err(err) = server.await {
            println!("Error serving connection: {:?}", err);
        }
    });
}

pub fn app_id_from_applet_id(applet_id: &String) -> String {
    format!("applet#{}", applet_id)
}

// pub fn applet_id_from_app_id(installed_app_id: &String) -> WeResult<String> {
//     match installed_app_id.strip_prefix("applet#") {
//         Some(id) => Ok(id.to_string()),
//         None => Err(Error::CustomError(String::from(
//             "Failed to convert installed_app_id to applet id.",
//         ))),
//     }
// }

// async fn get_applet_id_from_lowercase(
//     lowercase_applet_id: &String,
//     admin_ws: &mut AdminWebsocket,
// ) -> WeResult<String> {
//     let apps = admin_ws.list_apps(None).await?;

//     let app = apps
//         .into_iter()
//         .find(|app| {
//             app.installed_app_id
//                 .eq(&app_id_from_applet_id(lowercase_applet_id))
//                 || app
//                     .installed_app_id
//                     .to_lowercase()
//                     .eq(&app_id_from_applet_id(lowercase_applet_id))
//         })
//         .ok_or(Error::AdminWebsocketError(String::from(
//             "Applet is not installed",
//         )))?;
//     applet_id_from_app_id(&app.installed_app_id)
// }

pub async fn read_asset(
    fs: &FileSystem,
    app_id: &String,
    mut asset_name: String,
) -> Result<Option<(Vec<u8>, Option<String>)>> {
    // println!("Reading asset from filesystem. Asset name: {}", asset_name);
    if asset_name.starts_with("/") {
        asset_name = asset_name
            .strip_prefix("/")
            .expect("Failed to strip prefix")
            .to_string();
    }
    if asset_name == "" {
        asset_name = String::from("index.html");
    }

    let assets_path = fs.ui_store().ui_path(&app_id);
    let asset_file = assets_path.join(asset_name);

    let mime_guess = mime_guess::from_path(asset_file.clone());

    let mime_type = match mime_guess.first() {
        Some(mime) => Some(mime.essence_str().to_string()),
        None => {
            // log::info!("Could not determine MIME Type of file '{:?}'", asset_file);
            // println!("\n### ERROR ### Could not determine MIME Type of file '{:?}'\n", asset_file);
            None
        }
    };

    match std::fs::read(asset_file.clone()) {
        Ok(asset) => Ok(Some((asset, mime_type))),
        Err(_e) => Ok(None), // {
    }
}
