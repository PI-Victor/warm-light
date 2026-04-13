mod commands;
mod monitor;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::list_monitors,
            commands::set_monitor_feature,
            commands::transition_monitor_feature,
            commands::apply_color_scene
        ])
        .run(tauri::generate_context!())
        .expect("error while running WarmLight");
}
