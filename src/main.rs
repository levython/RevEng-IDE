//! RevEng-IDE — A professional Android reverse-engineering IDE.
//!
//! Entry point: initializes logging, the tokio runtime, and launches the eframe window.

mod app;
mod engine;
mod native;
mod runtime;
mod ui;

use anyhow::Result;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    log::info!("RevEng-IDE v{} starting", env!("CARGO_PKG_VERSION"));

    let app_icon = eframe::icon_data::from_png_bytes(include_bytes!("../icon.png"))
        .map(std::sync::Arc::new)
        .unwrap_or_else(|_| std::sync::Arc::new(egui::IconData::default()));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RevEng-IDE")
            .with_icon(app_icon)
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_fullscreen(false)
            .with_maximized(false),
        ..Default::default()
    };

    eframe::run_native(
        "RevEng-IDE",
        native_options,
        Box::new(|cc| Ok(Box::new(app::RevEngApp::new(cc)?))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}
