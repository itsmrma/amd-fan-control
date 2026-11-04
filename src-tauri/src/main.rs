#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod worker;

use fan_core::config::Config;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use worker::{Action, Snapshot, Worker};

const MAX_IMPORT_BYTES: u64 = 1024 * 1024;

#[tauri::command]
fn get_status(worker: tauri::State<'_, Worker>) -> Snapshot {
    worker.snapshot()
}

async fn action(app: tauri::AppHandle, command: Action) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Worker>().request(command))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn save_config(app: tauri::AppHandle, config: Config) -> Result<(), String> {
    action(app, Action::Save(config)).await
}
#[tauri::command]
async fn apply_curve(app: tauri::AppHandle) -> Result<(), String> {
    action(app, Action::Apply).await
}
#[tauri::command]
async fn restore_automatic(app: tauri::AppHandle) -> Result<(), String> {
    action(app, Action::Automatic).await
}
#[tauri::command]
async fn export_config(
    app: tauri::AppHandle,
    mut config: Config,
) -> Result<Option<String>, String> {
    config.validate().map_err(|e| format!("{e:#}"))?;
    // The switch is a runtime choice tied to this machine, not a portable setting.
    config.custom_control_enabled = false;
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Export fan settings")
        .add_filter("JSON settings", &["json"])
        .set_file_name("amd-fan-control-settings.json")
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    config.save(&path).map_err(|e| format!("{e:#}"))?;
    Ok(Some(path.display().to_string()))
}

fn read_import(path: &std::path::Path) -> Result<Config, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Reading settings file: {e}"))?
        .len();
    if size > MAX_IMPORT_BYTES {
        return Err("Settings file is too large (maximum 1 MB)".into());
    }
    let data = std::fs::read(path).map_err(|e| format!("Reading settings file: {e}"))?;
    let config: Config =
        serde_json::from_slice(&data).map_err(|e| format!("Invalid settings JSON: {e}"))?;
    config.validate().map_err(|e| format!("{e:#}"))?;
    Ok(config)
}

#[tauri::command]
async fn import_config(app: tauri::AppHandle) -> Result<Option<Config>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Import fan settings")
        .add_filter("JSON settings", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    read_import(&path).map(Some)
}

#[tauri::command]
fn autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}
#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(|e| e.to_string())
}
fn show(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            save_config,
            export_config,
            import_config,
            apply_curve,
            restore_automatic,
            autostart_enabled,
            set_autostart
        ])
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "Open AMD Fan Control", true, None::<&str>)?;
            let automatic = MenuItem::with_id(
                app,
                "automatic",
                "Use driver default fan settings",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &automatic, &quit])?;
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("App icon").clone())
                .tooltip("AMD Fan Control · Initializing")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show(app),
                    "automatic" | "quit" => {
                        let app = app.clone();
                        let quitting = event.id.as_ref() == "quit";
                        tauri::async_runtime::spawn(async move {
                            let result = action(
                                app.clone(),
                                if quitting {
                                    Action::Stop
                                } else {
                                    Action::Automatic
                                },
                            )
                            .await;
                            if result.is_ok() && quitting {
                                app.exit(0);
                            } else if result.is_err() {
                                show(&app);
                            }
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        show(tray.app_handle());
                    }
                })
                .build(app)?;
            let path = dirs::config_dir()
                .ok_or("Configuration directory unavailable")?
                .join("amd-fan-control/config.json");
            let start_in_tray = Config::load(&path)
                .map(|c| c.start_in_tray)
                .unwrap_or(false);
            app.manage(Worker::start(app.handle().clone(), path));
            if !start_in_tray {
                show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("Starting AMD Fan Control");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_reads_valid_settings_and_rejects_corrupt_or_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        Config::default().save(&path).unwrap();
        assert_eq!(
            read_import(&path).unwrap().polling_ms,
            Config::default().polling_ms
        );

        std::fs::write(&path, "broken").unwrap();
        assert!(read_import(&path)
            .unwrap_err()
            .contains("Invalid settings JSON"));

        std::fs::write(&path, vec![b'x'; (MAX_IMPORT_BYTES + 1) as usize]).unwrap();
        assert!(read_import(&path).unwrap_err().contains("too large"));
    }
}
