use std::path::PathBuf;
use std::{fs, io::Write};

use holochain::prelude::*;
use holochain_types::web_app::WebAppBundle;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Clone)]
pub struct FileSystem {
    pub app_data_dir: PathBuf,
    pub app_config_dir: PathBuf,
}
/// Returns a string considering the relevant part of the version regarding breaking changes
/// Examples:
/// 3.2.0 becomes 3.x.x
/// 0.2.2 becomes 0.2.x
/// 0.0.5 becomes 0.0.5
/// 0.2.3-alpha.2 remains 0.2.3-alpha.2 --> pre-releases always get their own storage location since we have to assume breaking changes
pub fn breaking_app_version<R: Runtime>(app_handle: &AppHandle<R>) -> String {
    let app_version = app_handle.package_info().version.clone();

    if app_version.pre.is_empty() == false {
        return app_version.to_string();
    }

    match app_version.major {
        0 => match app_version.minor {
            0 => format!("0.0.{}", app_version.patch),
            _ => format!("0.{}.x", app_version.minor),
        },
        _ => format!("{}.x.x", app_version.major),
    }
}

impl FileSystem {
    pub fn new<R: Runtime>(
        app_handle: &AppHandle<R>,
        subfolder: &PathBuf,
    ) -> crate::Result<FileSystem> {
        let version_folder = breaking_app_version(app_handle);

        let app_data_dir = app_handle
            .path()
            .app_data_dir()?
            .join(&version_folder)
            .join(subfolder);
        let app_config_dir = app_handle
            .path()
            .app_config_dir()?
            .join(&version_folder)
            .join(subfolder);

        fs::create_dir_all(app_data_dir.join("webhapps"))?;
        fs::create_dir_all(app_data_dir.join("icons"))?;

        Ok(FileSystem {
            app_data_dir,
            app_config_dir,
        })
    }

    pub fn keystore_path(&self) -> PathBuf {
        self.app_data_dir.join("keystore")
    }

    pub fn keystore_config_path(&self) -> PathBuf {
        self.keystore_path().join("lair-keystore-config.yaml")
    }

    pub fn conductor_path(&self) -> PathBuf {
        self.app_data_dir.join("conductor")
    }

    pub fn webapp_store(&self) -> WebAppStore {
        WebAppStore {
            path: self.app_data_dir.join("webhapps"),
        }
    }

    pub fn icon_store(&self) -> IconStore {
        IconStore {
            path: self.app_data_dir.join("icons"),
        }
    }

    pub fn ui_store(&self) -> UiStore {
        UiStore {
            path: self.app_data_dir.join("uis"),
        }
    }
}

pub struct UiStore {
    path: PathBuf,
}

impl UiStore {
    pub fn ui_path(&self, installed_app_id: &InstalledAppId) -> PathBuf {
        self.path.join(installed_app_id)
    }

    pub async fn extract_and_store_ui(
        &self,
        installed_app_id: &InstalledAppId,
        web_app: &WebAppBundle,
    ) -> crate::Result<()> {
        let ui_bytes = web_app.web_ui_zip_bytes().await?;

        let ui_folder_path = self.ui_path(installed_app_id);

        fs::create_dir_all(&ui_folder_path)?;

        let ui_zip_path = self.path.join("ui.zip");

        fs::write(ui_zip_path.clone(), ui_bytes.into_owned().into_inner())?;

        let file = std::fs::File::open(ui_zip_path.clone())?;
        unzip_file(file, ui_folder_path)?;

        fs::remove_file(ui_zip_path)?;

        Ok(())
    }
}

pub struct WebAppStore {
    path: PathBuf,
}

impl WebAppStore {
    fn webhapp_path(&self, web_app_entry_hash: &EntryHash) -> PathBuf {
        let web_app_entry_hash_b64 = EntryHashB64::from(web_app_entry_hash.clone()).to_string();
        self.path.join(web_app_entry_hash_b64)
    }

    pub fn webhapp_package_path(&self, web_app_entry_hash: &EntryHash) -> PathBuf {
        self.webhapp_path(web_app_entry_hash)
            .join("package.webhapp")
    }

    pub fn get_webapp(
        &self,
        web_app_entry_hash: &EntryHash,
    ) -> crate::Result<Option<WebAppBundle>> {
        let path = self.webhapp_path(web_app_entry_hash);

        if path.exists() {
            let bytes = fs::read(self.webhapp_package_path(&web_app_entry_hash))?;
            let web_app = WebAppBundle::decode(bytes.as_slice())?;

            return Ok(Some(web_app));
        } else {
            return Ok(None);
        }
    }

    pub async fn store_webapp(
        &self,
        web_app_entry_hash: &EntryHash,
        web_app: &WebAppBundle,
    ) -> crate::Result<()> {
        let bytes = web_app.encode()?;

        let path = self.webhapp_path(web_app_entry_hash);

        fs::create_dir_all(path.clone())?;

        let mut file = std::fs::File::create(self.webhapp_package_path(web_app_entry_hash))?;
        file.write_all(bytes.as_slice())?;

        Ok(())
    }
}

pub struct IconStore {
    path: PathBuf,
}

impl IconStore {
    fn icon_path(&self, app_entry_hash: &ActionHash) -> PathBuf {
        self.path
            .join(ActionHashB64::from(app_entry_hash.clone()).to_string())
    }

    pub fn store_icon(&self, app_entry_hash: &ActionHash, icon_src: String) -> crate::Result<()> {
        fs::write(self.icon_path(app_entry_hash), icon_src.as_bytes())?;

        Ok(())
    }

    pub fn get_icon(&self, app_entry_hash: &ActionHash) -> crate::Result<Option<String>> {
        let icon_path = self.icon_path(app_entry_hash);
        if icon_path.exists() {
            let icon = fs::read_to_string(icon_path)?;
            return Ok(Some(icon));
        } else {
            return Ok(None);
        }
    }
}

pub fn unzip_file(reader: std::fs::File, outpath: PathBuf) -> crate::Result<()> {
    let mut archive = zip::ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let outpath = match file.enclosed_name() {
            Some(path) => outpath.join(path).to_owned(),
            None => continue,
        };

        if (&*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = fs::File::create(&outpath).unwrap();
            std::io::copy(&mut file, &mut outfile).unwrap();
        }
    }

    Ok(())
}
