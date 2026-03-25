//! `scheng-tauri` — Tauri 2 shell embedding the scheng wgpu runtime.

mod commands;
mod engine;
mod preview;
mod render_loop;

pub use engine::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    // Create AppState here so we can clone it into the render thread
    // without needing Manager::state() inside the setup closure.
    let app_state = AppState::new();
    let state_for_thread = app_state.clone();

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_params,
            commands::set_param,
            commands::set_output_mode,
            commands::start_recording,
            commands::stop_recording,
            commands::get_engine_status,
            commands::load_graph_json,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let state      = state_for_thread.clone();

            std::thread::Builder::new()
                .name("scheng-render".into())
                .spawn(move || render_loop::run(app_handle, state))
                .expect("failed to spawn render thread");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running scheng tauri app");
}
