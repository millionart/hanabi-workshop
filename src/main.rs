use bevy::{asset::UnapprovedPathMode, prelude::*};

#[allow(non_snake_case, dead_code)]
mod IconsFontAwesome7;
mod app_commands;
mod asset_library;
mod confirm;
mod document;
mod edits;
mod effect_graph;
mod effect_library;
mod history;
mod playback;
mod plugins;
mod proxy;
mod resource_paths;
mod texture_preview;
mod thumbnail;
mod ui;

use hanabi_effect_graph::modifier_registry;

fn main() {
    // Handle --version / -V and --help / -h before Bevy initialises anything,
    // so CI can smoke-test the binary on a headless machine.
    let mut documents = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("--version" | "-V") => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            Some("--help" | "-h") => {
                println!(
                    "Usage: {} [OPTIONS] [FILE.hnb ...]\n\n\
                     {}\n\n\
                     Options:\n  \
                       -h, --help     Print help\n  \
                       -V, --version  Print version",
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_DESCRIPTION"),
                );
                std::process::exit(0);
            }
            _ => {
                let path = std::path::PathBuf::from(arg);
                if path.extension().is_some_and(|ext| ext == "hnb") {
                    documents.push(path);
                }
            }
        }
    }

    // Resolve the bundled `assets/` directory once before Bevy starts so that
    // the AssetPlugin, texture catalog, and example browser all share a single
    // consistent root that does not depend on the launch working directory.
    let assets_dir = resource_paths::resolve_bundled_root()
        .map(|root| root.join("assets"))
        .unwrap_or_else(|| std::path::PathBuf::from("assets"));

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Hanabi Workshop".into(),
                        ..default()
                    }),
                    // The unsaved-changes guard (`crate::confirm`) intercepts
                    // the OS window-close request, so Bevy must not close the
                    // window itself.
                    close_when_requested: false,
                    ..default()
                })
                // Set an absolute file_path so Bevy's FileAssetReader resolves
                // textures relative to the bundled assets directory regardless
                // of the launch working directory. Texture bindings for
                // user-selected files are absolute paths outside `assets/`;
                // Deny (rather than the default Forbid) lets those specific
                // loads opt in via `AssetServer::load_override` while every
                // other unapproved path stays rejected.
                .set(AssetPlugin {
                    file_path: assets_dir.to_string_lossy().into_owned(),
                    unapproved_path_mode: UnapprovedPathMode::Deny,
                    ..default()
                }),
        )
        .add_plugins(plugins::EditorPlugins)
        .add_systems(Startup, move |mut open: MessageWriter<app_commands::AppCommand>| {
            for path in &documents {
                open.write(app_commands::AppCommand::OpenFile(path.clone()));
            }
        })
        .run();
}
