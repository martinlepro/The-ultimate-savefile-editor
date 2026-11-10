//! SaveStudio : l'interface. Les éditeurs de jeux (définitions, thèmes, icônes, polices, emplacements) sont téléchargés
//! depuis le dépôt « The-ultimate-savefile-editor-games ». Le binaire de bureau est `src/main.rs` ;
//! le point d'entrée Android est le crate `android/`.
mod app;
mod def;
mod detect;
mod doc;
mod fonts;
mod gd;
mod gfx;
mod hk;
mod nbt;
mod pack;
mod settings;
mod theme;
mod updater;
mod xml;

use std::path::PathBuf;

/// Options de lancement.
#[derive(Default, Clone)]
pub struct Launch {
    /// sauvegarde à ouvrir directement
    pub open: Option<PathBuf>,
    /// catégorie affichée à l'ouverture
    pub page: usize,
    /// pack local à utiliser à la place du pack téléchargé (aucune mise à jour proposée)
    pub pack: Option<PathBuf>,
}

/// Lance l'application.
pub fn run(launch: Launch, opts: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native("SaveStudio", opts, Box::new(move |cc| Ok(Box::new(Shell(app::App::new(cc, launch.clone()))))))
}

struct Shell(app::App);

impl eframe::App for Shell {
    fn update(&mut self, ctx: &eframe::egui::Context, f: &mut eframe::Frame) {
        self.0.frame(ctx, f);
    }
}
