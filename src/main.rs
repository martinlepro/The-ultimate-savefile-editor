// pas de console noire derrière la fenêtre sous Windows (build release)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut launch = savestudio::Launch::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--open" if i + 1 < args.len() => {
                launch.open = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--page" if i + 1 < args.len() => {
                launch.page = args[i + 1].parse().unwrap_or(0);
                i += 1;
            }
            "--pack" if i + 1 < args.len() => {
                launch.pack = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            a if !a.starts_with("--") => launch.open = Some(std::path::PathBuf::from(a)),
            _ => {}
        }
        i += 1;
    }
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([900.0, 1040.0]).with_min_inner_size([720.0, 620.0]).with_title("SaveStudio"),
        ..Default::default()
    };
    savestudio::run(launch, opts)
}
