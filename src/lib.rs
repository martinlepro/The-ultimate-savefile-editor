//! SaveStudio : interface, codecs et définitions de jeux.
//! Le binaire de bureau est `src/main.rs` ; le point d'entrée Android est le crate `android/`.
mod app;
mod def;
mod detect;
mod doc;
mod gd;
mod gfx;
mod hk;
mod nbt;
mod theme;
mod xml;

use std::path::PathBuf;

/// Lance l'application. `open` : sauvegarde à ouvrir directement, `page` : catégorie affichée.
pub fn run(open: Option<PathBuf>, page: usize, opts: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native("SaveStudio", opts, Box::new(move |cc| Ok(Box::new(Shell(app::App::new(cc, open.clone(), page))))))
}

struct Shell(app::App);

impl eframe::App for Shell {
    fn update(&mut self, ctx: &eframe::egui::Context, f: &mut eframe::Frame) {
        self.0.frame(ctx, f);
    }
}
