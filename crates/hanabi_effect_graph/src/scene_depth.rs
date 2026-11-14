//! Exposes Bevy's scene depth as a Hanabi image slot for transparent effects.

use bevy::{
    asset::{AssetEventSystems, RenderAssetUsages},
    camera::CameraUpdateSystems,
    core_pipeline::{
        Core3dSystems, FullscreenShader,
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d},
        prepass::ViewPrepassTextures,
        schedule::Core3d,
    },
    prelude::*,
    render::{
        RenderApp, RenderStartup,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{texture_2d, texture_2d_multisampled},
            *,
        },
        renderer::{RenderContext, ViewQuery},
        texture::GpuImage,
        view::ViewDepthStencilTexture,
    },
    shader::{Shader, ShaderDefVal},
};

pub const SCENE_DEPTH_SLOT: &str = "scene_depth";

/// Add to 3D cameras that render effects using the `scene_depth` slot.
#[derive(Component, Clone, Copy, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct SceneDepthCamera;

/// Image handle to bind to `scene_depth` in an effect's material.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct SceneDepth(pub Handle<Image>);

pub struct SceneDepthPlugin;

impl Plugin for SceneDepthPlugin {
    fn build(&self, app: &mut App) {
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render.add_systems(RenderStartup, prepare).add_systems(
            Core3d,
            copy_depth
                .in_set(Core3dSystems::MainPass)
                .after(main_opaque_pass_3d)
                .before(main_transparent_pass_3d),
        );
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let mut image = Image::new_uninit(
            Extent3d::default(),
            TextureDimension::D2,
            TextureFormat::Rgba16Float,
            RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage =
            TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
        let output = images.add(image);
        app.insert_resource(SceneDepth(output))
            .add_plugins((
                ExtractResourcePlugin::<SceneDepth>::default(),
                ExtractComponentPlugin::<SceneDepthCamera>::default(),
            ))
            .add_systems(
                PostUpdate,
                enable_depth_sampling.before(CameraUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                resize_depth_image
                    .after(CameraUpdateSystems)
                    .before(AssetEventSystems),
            );
        let shader = Shader::from_wesl(
            include_str!("scene_depth.wesl"),
            "hanabi_workshop/scene_depth.wesl",
        );
        let handle = app.world_mut().resource_mut::<Assets<Shader>>().add(shader);
        app.sub_app_mut(RenderApp)
            .insert_resource(DepthShader(handle));
    }
}

fn enable_depth_sampling(mut cameras: Query<&mut Camera3d, Added<SceneDepthCamera>>) {
    for mut camera in &mut cameras {
        camera.depth_texture_usages = (TextureUsages::from(camera.depth_texture_usages)
            | TextureUsages::TEXTURE_BINDING)
            .into();
    }
}

fn resize_depth_image(
    cameras: Query<&Camera, With<SceneDepthCamera>>,
    image: Res<SceneDepth>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut size = Extent3d::default();
    for extent in cameras
        .iter()
        .filter(|camera| camera.is_active)
        .filter_map(Camera::physical_target_size)
    {
        size.width = size.width.max(extent.x);
        size.height = size.height.max(extent.y);
    }
    if images
        .get(&image.0)
        .is_some_and(|image| image.texture_descriptor.size != size)
    {
        images.get_mut(&image.0).unwrap().resize(size);
    }
}

#[derive(Resource)]
struct DepthShader(Handle<Shader>);

#[derive(Resource)]
struct DepthPipelines([(BindGroupLayoutDescriptor, CachedRenderPipelineId); 2]);

fn prepare(
    mut commands: Commands,
    shader: Res<DepthShader>,
    cache: Res<PipelineCache>,
    fullscreen: Res<FullscreenShader>,
) {
    let pipelines = [false, true].map(|multisampled| {
        let depth = if multisampled {
            texture_2d_multisampled(TextureSampleType::Depth)
        } else {
            texture_2d(TextureSampleType::Depth)
        };
        let layout = BindGroupLayoutDescriptor::new(
            "hanabi_scene_depth",
            &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (depth,)),
        );
        let id = cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("hanabi_scene_depth".into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader.0.clone(),
                shader_defs: vec![ShaderDefVal::Bool("MULTISAMPLED".into(), multisampled)],
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        });
        (layout, id)
    });
    commands.insert_resource(DepthPipelines(pipelines));
}

fn copy_depth(
    view: ViewQuery<
        (
            &ViewDepthStencilTexture,
            Option<&ViewPrepassTextures>,
            &Msaa,
        ),
        With<SceneDepthCamera>,
    >,
    pipeline: Res<DepthPipelines>,
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    target: Res<SceneDepth>,
    mut context: RenderContext,
) {
    let (main_depth, prepass, msaa) = view.into_inner();
    let depth = prepass
        .and_then(|prepass| prepass.depth.as_ref())
        .unwrap_or(&main_depth.attachment);
    let Some(image) = images.get(&target.0) else {
        return;
    };
    let Some(depth_view) = depth.depth_stencil_views().depth_only_view() else {
        return;
    };
    let size = depth.texture.texture.size();
    if size.width > image.texture_descriptor.size.width
        || size.height > image.texture_descriptor.size.height
    {
        return;
    }
    let (layout, id) = &pipeline.0[usize::from(*msaa != Msaa::Off)];
    let Some(pipeline) = cache.get_render_pipeline(*id) else {
        return;
    };
    let group = context.render_device().create_bind_group(
        "hanabi_scene_depth",
        &cache.get_bind_group_layout(layout),
        &BindGroupEntries::single(depth_view),
    );
    let mut pass = context
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("hanabi_scene_depth"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &image.texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Default::default()),
                    store: StoreOp::Store,
                },
            })],
            ..default()
        });
    pass.set_viewport(0., 0., size.width as f32, size.height as f32, 0., 1.);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.draw(0..3, 0..1);
}
