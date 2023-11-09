use sys_locale::get_locales as get_locales_native;
use tauri::command;

#[command]
pub(crate) fn get_locales() -> Vec<String> {
    get_locales_native().collect()
}
