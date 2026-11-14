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
    let mut roundtrip = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
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
                       -V, --version  Print version\n  \
                       --roundtrip OUTPUT.hnb FILE.hnb  Open, edit and save an effect in a hidden window",
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_DESCRIPTION"),
                );
                std::process::exit(0);
            }
            Some("--roundtrip") => {
                roundtrip = Some(std::path::PathBuf::from(
                    args.next().expect("--roundtrip needs an output file"),
                ));
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

    let automated = roundtrip.is_some();
    if automated {
        assert_eq!(documents.len(), 1, "--roundtrip needs one input graph");
        assert_ne!(
            documents.first(),
            roundtrip.as_ref(),
            "--roundtrip needs a separate output file"
        );
    }
    let mut window = Window {
        title: "Hanabi Workshop".into(),
        visible: !automated,
        focused: !automated,
        ..default()
    };
    if automated {
        window.set_minimized(true);
    }
    let mut app = App::new();
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(window),
            close_when_requested: false,
            ..default()
        })
        .set(AssetPlugin {
            file_path: assets_dir.to_string_lossy().into_owned(),
            unapproved_path_mode: UnapprovedPathMode::Deny,
            ..default()
        });
    if automated {
        app.add_plugins(
            plugins.disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>(),
        );
    } else {
        app.add_plugins(plugins);
    }
    app.add_plugins(plugins::EditorPlugins).add_systems(
        Startup,
        move |mut open: MessageWriter<app_commands::AppCommand>| {
            for path in &documents {
                open.write(app_commands::AppCommand::OpenFile(path.clone()));
            }
        },
    );
    if let Some(path) = roundtrip {
        app.insert_resource(bevy::winit::WinitSettings::continuous());
        app.add_systems(PostStartup, use_offscreen_editor_camera)
            .add_systems(Last, move |mut probe: Roundtrip| probe.tick(&path));
    }
    app.run();
}

fn use_offscreen_editor_camera(
    mut camera: Single<&mut bevy::camera::RenderTarget, With<bevy_egui::PrimaryEguiContext>>,
    mut images: ResMut<Assets<Image>>,
) {
    let image = Image::new_target_texture(
        1280,
        720,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        None,
    );
    **camera = bevy::camera::RenderTarget::Image(images.add(image).into());
}

#[derive(bevy::ecs::system::SystemParam)]
struct Roundtrip<'w, 's> {
    time: Res<'w, Time<Real>>,
    active: Res<'w, document::ActiveDocument>,
    docs: Query<
        'w,
        's,
        (
            &'static document::DocumentContent,
            &'static plugins::shader_errors::ShaderErrors,
        ),
    >,
    preview: Query<
        'w,
        's,
        (
            &'static bevy_hanabi::CompiledParticleEffect,
            &'static bevy_hanabi::EffectSpawner,
        ),
        With<document::SceneEmitter>,
    >,
    edits: MessageWriter<'w, edits::EditRequest>,
    files: MessageWriter<'w, app_commands::AppCommand>,
    exits: MessageWriter<'w, AppExit>,
    edited: Local<
        's,
        Option<(
            hanabi_effect_graph::model::EmitterId,
            hanabi_effect_graph::model::NodeId,
            bevy_hanabi::Value,
        )>,
    >,
    saving: Local<'s, bool>,
}

impl Roundtrip<'_, '_> {
    fn tick(&mut self, path: &std::path::Path) {
        use bevy_hanabi::Value;
        use hanabi_effect_graph::model::{ExprNode, NodePayload};

        assert!(
            self.time.elapsed_secs() < 20.,
            "Workshop round trip timed out"
        );
        let Some(entity) = self.active.0 else {
            return;
        };
        let Ok((content, shaders)) = self.docs.get(entity) else {
            return;
        };
        assert!(
            content.bake_errors().is_empty(),
            "{:?}",
            content.bake_errors()
        );
        assert!(shaders.0.is_empty(), "{:?}", shaders.0);
        if let Some((emitter, node, new)) = *self.edited {
            let actual = &content
                .effect_graph()
                .emitter(emitter)
                .unwrap()
                .node(node)
                .unwrap()
                .payload;
            if *actual != NodePayload::Expr(ExprNode::Literal(new)) {
                return;
            }
            if !*self.saving {
                self.files.write(app_commands::AppCommand::SaveDocumentAs(
                    entity,
                    path.to_owned(),
                ));
                *self.saving = true;
                return;
            }
            if content.path() != Some(path) {
                return;
            }
            let saved = hanabi_effect_graph::from_ron_bytes(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(&saved.graph, content.effect_graph());
            assert_eq!(
                saved
                    .graph
                    .emitter(emitter)
                    .unwrap()
                    .node(node)
                    .unwrap()
                    .payload,
                NodePayload::Expr(ExprNode::Literal(new))
            );
            info!(
                "Workshop round trip passed: {} emitters; edited node {}, saved and read back",
                saved.graph.emitters.len(),
                node.get()
            );
            self.exits.write(AppExit::Success);
            return;
        }
        if !self
            .preview
            .iter()
            .any(|(effect, spawner)| effect.is_ready() && spawner.completed_cycle_count() > 0)
        {
            return;
        }
        let (emitter, node, old) = content
            .effect_graph()
            .emitters
            .iter()
            .find_map(|emitter| {
                emitter.nodes.iter().find_map(|node| match &node.payload {
                    NodePayload::Expr(ExprNode::Literal(Value::Scalar(
                        bevy_hanabi::ScalarValue::Float(old),
                    ))) if *old > 0.
                        && emitter.links.iter().any(|link| link.from.node == node.id) =>
                    {
                        Some((emitter.id, node.id, *old))
                    }
                    _ => None,
                })
            })
            .expect("linked editable scalar node");
        let new = Value::from(old * 1.25);
        self.edits.write(edits::EditRequest::new(
            entity,
            edits::EditKind::SetLiteralValue { emitter, node, new },
        ));
        *self.edited = Some((emitter, node, new));
    }
}
