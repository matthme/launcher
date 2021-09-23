use std::fs;

use holochain_conductor_client::{AdminWebsocket, AppStatusFilter};
use tauri::api::process::Command;

use crate::{
  setup::config::{admin_url, caddyfile_path, DEFAULT_ADMIN_PORT, DEFAULT_APP_PORT},
  uis::port_mapping::app_ui_folder_path,
};

use super::port_mapping::PortMapping;

const LAUNCHER_ENV_URL: &str = ".launcher-env.json";

fn caddyfile_config_for_an_app(port: u16, app_id: String) -> String {
  format!(
    r#"
:{} {{
    respond /{} 200 {{
		    body `{{
  "APP_INTERFACE_PORT": {},
  "ADMIN_INTERFACE_PORT": {},
  "INSTALLED_APP_ID": "{}"
}}`
		    close
	  }}
    
    header Cache-Control no-cache, no-store
    
    root * "{}"
    file_server
}}
        "#,
    port,
    LAUNCHER_ENV_URL,
    DEFAULT_APP_PORT,
    DEFAULT_ADMIN_PORT,
    app_id.clone(),
    app_ui_folder_path(app_id)
      .into_os_string()
      .to_str()
      .unwrap(),
  )
}

fn build_caddyfile_contents(
  active_apps_ids: Vec<String>,
  port_mapping: PortMapping,
) -> Result<String, String> {
  let mut config_vec = active_apps_ids
    .into_iter()
    .map(|app_id| {
      let port = port_mapping
        .get_ui_port_for_app(&app_id)
        .ok_or(String::from("This app has no assigned port"))?;

      Ok(caddyfile_config_for_an_app(port, app_id))
    })
    .collect::<Result<Vec<String>, String>>()?;

  let empty_line = r#"
    "#;

  config_vec.push(empty_line.into());

  Ok(config_vec.join(empty_line))
}

/// Connects to the conductor, requests the list of running apps, and writes the Caddyfile with the appropriate port mapping
async fn refresh_caddyfile() -> Result<(), String> {
  log::info!("Refreshing caddyfile");
  let mut ws = AdminWebsocket::connect(admin_url())
    .await
    .or(Err(String::from("Could not connect to conductor")))?;

  let active_apps = ws
    .list_apps(Some(AppStatusFilter::Running))
    .await
    .or(Err("Could not get the currently active apps"))?;

  let port_mapping = PortMapping::read_port_mapping()?;

  let active_app_ids = active_apps
    .into_iter()
    .map(|a| a.installed_app_id)
    .collect();

  let caddyfile_contents = build_caddyfile_contents(active_app_ids, port_mapping)?;

  fs::write(caddyfile_path(), caddyfile_contents)
    .map_err(|err| format!("Error writing Caddyfile: {:?}", err))?;

  Ok(())
}

/// Refreshes the running apps and reloads caddy to be consistent with them
/// Execute this when there has been some change in the status of an app (enabled, disabled, uninstalled...)
pub async fn reload_caddy() -> Result<(), String> {
  refresh_caddyfile().await?;

  log::info!("Reloading Caddy");

  Command::new_sidecar("caddy")
    .or(Err(String::from("Can't find caddy binary")))?
    .args(&[
      "reload",
      "--config",
      caddyfile_path().as_os_str().to_str().unwrap(),
    ])
    .spawn()
    .map_err(|err| format!("Error reloading caddy {:?}", err))?;

  Ok(())
}

/// Builds the Caddyfile from the list of running apps and launches caddy
/// Execute only on launcher start
pub async fn launch_caddy() -> Result<(), String> {
  refresh_caddyfile().await?;

  Command::new_sidecar("caddy")
    .or(Err(String::from("Can't find caddy binary")))?
    .args(&[
      "run",
      "--config",
      caddyfile_path().as_os_str().to_str().unwrap(),
    ])
    .spawn()
    .map_err(|err| format!("Error running caddy {:?}", err))?;

  Ok(())
}
