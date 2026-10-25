use photo_sorter::app::{PhotoSorterApp, default_state_path};

fn main() -> eframe::Result {
    env_logger::init();
    // Some VMs and old GPUs lack DX12/Vulkan/Metal: retry once on OpenGL. PHOTO_SORTER_RENDERER=glow forces it.
    let forced_glow = std::env::var("PHOTO_SORTER_RENDERER").is_ok_and(|r| r.eq_ignore_ascii_case("glow"));
    if forced_glow {
        return run(eframe::Renderer::Glow);
    }
    run(eframe::Renderer::Wgpu).or_else(|error| {
        log::warn!("wgpu renderer failed ({error}); falling back to glow");
        run(eframe::Renderer::Glow)
    })
}

fn run(renderer: eframe::Renderer) -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("Photo Sorter")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("bundled icon is a valid PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Photo Sorter",
        options,
        Box::new(|creation_ctx| {
            let state_path = default_state_path().ok_or("no home directory to store app data in")?;
            Ok(Box::new(PhotoSorterApp::new(&creation_ctx.egui_ctx, state_path)?))
        }),
    )
}
