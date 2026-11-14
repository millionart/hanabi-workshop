//! Exposes Bevy's scene depth as a Hanabi image slot for transparent effects.

use bevy::{
    asset::{AssetEventSystems, AssetId, RenderAssetUsages},
    camera::CameraUpdateSystems,
    core_pipeline::{
        Core3dSystems, FullscreenShader,
        core_3d::{AlphaMask3d, Opaque3d, main_opaque_pass_3d, main_transparent_pass_3d},
        prepass::ViewPrepassTextures,
        schedule::Core3d,
    },
    image::ImageSampler,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_phase::ViewBinnedRenderPhases,
        render_resource::{
            binding_types::{texture_2d, texture_2d_multisampled},
            *,
        },
        renderer::{RenderContext, ViewQuery},
        texture::GpuImage,
        view::{ViewDepthStencilTexture, ViewTarget},
    },
    shader::{Shader, ShaderDefVal},
};
use bevy_hanabi::{EffectMaterial, EffectSystems};

pub const SCENE_DEPTH_SLOT: &str = "scene_depth";
pub const SCENE_COLOR_SLOT: &str = "scene_color";

/// Add to 3D cameras that render effects using the `scene_depth` slot.
#[derive(Component, Clone, Copy, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct SceneDepthCamera;

/// Image handle to bind to `scene_depth` in an effect's material.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct SceneDepth(pub Handle<Image>);

/// An opaque-scene snapshot for screen-space distortion effects.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct SceneColor(pub Handle<Image>);

pub struct SceneDepthPlugin;

impl Plugin for SceneDepthPlugin {
    fn build(&self, app: &mut App) {
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render.add_systems(
            Render,
            clear_hanabi_bins
                .in_set(RenderSystems::Queue)
                .before(EffectSystems::QueueEffects),
        );
        render.add_systems(RenderStartup, prepare).add_systems(
            Core3d,
            copy_depth
                .in_set(Core3dSystems::MainPass)
                .after(main_opaque_pass_3d)
                .before(main_transparent_pass_3d),
        );
        render.add_systems(
            Core3d,
            copy_color
                .in_set(Core3dSystems::MainPass)
                .after(main_opaque_pass_3d)
                .before(main_transparent_pass_3d),
        );
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let [depth, color] = std::array::from_fn(|_| {
            let mut image = Image::new_uninit(
                Extent3d::default(),
                TextureDimension::D2,
                TextureFormat::Rgba16Float,
                RenderAssetUsages::default(),
            );
            image.texture_descriptor.usage =
                TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
            image.sampler = ImageSampler::linear();
            images.add(image)
        });
        app.insert_resource(SceneDepth(depth))
            .insert_resource(SceneColor(color))
            .init_resource::<SceneInputsInUse>()
            .add_plugins((
                ExtractResourcePlugin::<SceneDepth>::default(),
                ExtractResourcePlugin::<SceneColor>::default(),
                ExtractResourcePlugin::<SceneInputsInUse>::default(),
                ExtractComponentPlugin::<SceneDepthCamera>::default(),
            ))
            .add_systems(
                PostUpdate,
                enable_depth_sampling.before(CameraUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                resize_scene_images
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

fn clear_hanabi_bins(
    mut opaque: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    mut masked: ResMut<ViewBinnedRenderPhases<AlphaMask3d>>,
) {
    // Hanabi 0.20.0 bins temporary draw entities which expire at the end of each render frame.
    let id = AssetId::<Mesh>::Uuid {
        uuid: bevy::asset::uuid::uuid!("a36f954d-a51d-484c-bddb-4569f270076a"),
    }
    .untyped();
    for phase in opaque.values_mut() {
        let old: Vec<_> = phase
            .non_mesh_items
            .iter()
            .filter(|((_, bin), _)| bin.asset_id == id)
            .flat_map(|(_, bin)| bin.entities.keys().copied())
            .collect();
        for entity in old {
            phase.remove(entity);
        }
        phase
            .non_mesh_items
            .retain(|(_, bin), _| bin.asset_id != id);
    }
    for phase in masked.values_mut() {
        let old: Vec<_> = phase
            .non_mesh_items
            .iter()
            .filter(|((_, bin), _)| bin.asset_id == id)
            .flat_map(|(_, bin)| bin.entities.keys().copied())
            .collect();
        for entity in old {
            phase.remove(entity);
        }
        phase
            .non_mesh_items
            .retain(|(_, bin), _| bin.asset_id != id);
    }
}

fn enable_depth_sampling(mut cameras: Query<&mut Camera3d, Added<SceneDepthCamera>>) {
    for mut camera in &mut cameras {
        camera.depth_texture_usages = (TextureUsages::from(camera.depth_texture_usages)
            | TextureUsages::TEXTURE_BINDING)
            .into();
    }
}

fn resize_scene_images(
    cameras: Query<&Camera, With<SceneDepthCamera>>,
    image: Res<SceneDepth>,
    color: Res<SceneColor>,
    materials: Query<&EffectMaterial>,
    mut in_use: ResMut<SceneInputsInUse>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut used = SceneInputsInUse::default();
    for material in &materials {
        used.depth |= material.images.contains(&image.0);
        used.color |= material.images.contains(&color.0);
        if used.depth && used.color {
            break;
        }
    }
    in_use.set_if_neq(used);
    let mut size = Extent3d::default();
    for extent in cameras
        .iter()
        .filter(|camera| camera.is_active)
        .filter_map(Camera::physical_target_size)
    {
        size.width = size.width.max(extent.x);
        size.height = size.height.max(extent.y);
    }
    for (handle, active) in [(&image.0, used.depth), (&color.0, used.color)] {
        let size = if active { size } else { Extent3d::default() };
        if images
            .get(handle)
            .is_some_and(|image| image.texture_descriptor.size != size)
        {
            images.get_mut(handle).unwrap().resize(size);
        }
    }
}

#[derive(Default, Resource, Clone, Copy, PartialEq, ExtractResource)]
#[extract_app(RenderApp)]
struct SceneInputsInUse {
    depth: bool,
    color: bool,
}

#[derive(Resource)]
struct DepthShader(Handle<Shader>);

#[derive(Resource)]
struct DepthPipelines([(BindGroupLayoutDescriptor, CachedRenderPipelineId); 2]);

#[derive(Resource)]
struct ColorPipeline(BindGroupLayoutDescriptor, CachedRenderPipelineId);

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
    let layout = BindGroupLayoutDescriptor::new(
        "hanabi_scene_color",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (texture_2d(TextureSampleType::Float { filterable: false }),),
        ),
    );
    let id = cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("hanabi_scene_color".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: shader.0.clone(),
            shader_defs: vec![ShaderDefVal::Bool("COPY_COLOR".into(), true)],
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    });
    commands.insert_resource(ColorPipeline(layout, id));
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
    in_use: Res<SceneInputsInUse>,
    mut context: RenderContext,
) {
    if !in_use.depth {
        return;
    }
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
    let (layout, id) = &pipeline.0[usize::from(*msaa != Msaa::Off)];
    let Some(pipeline) = cache.get_render_pipeline(*id) else {
        return;
    };
    copy_view(
        &mut context,
        &cache.get_bind_group_layout(layout),
        pipeline,
        depth_view,
        image,
        size,
    );
}

fn copy_color(
    view: ViewQuery<&ViewTarget, With<SceneDepthCamera>>,
    pipeline: Res<ColorPipeline>,
    cache: Res<PipelineCache>,
    images: Res<RenderAssets<GpuImage>>,
    target: Res<SceneColor>,
    in_use: Res<SceneInputsInUse>,
    mut context: RenderContext,
) {
    if !in_use.color {
        return;
    }
    let view = view.into_inner();
    let (Some(image), Some(render_pipeline)) =
        (images.get(&target.0), cache.get_render_pipeline(pipeline.1))
    else {
        return;
    };
    copy_view(
        &mut context,
        &cache.get_bind_group_layout(&pipeline.0),
        render_pipeline,
        view.main_texture_view(),
        image,
        view.main_texture().size(),
    );
}

fn copy_view(
    context: &mut RenderContext,
    layout: &BindGroupLayout,
    pipeline: &RenderPipeline,
    source: &TextureView,
    image: &GpuImage,
    size: Extent3d,
) {
    if size.width > image.texture_descriptor.size.width
        || size.height > image.texture_descriptor.size.height
    {
        return;
    }
    let group = context.render_device().create_bind_group(
        "hanabi_scene_input",
        layout,
        &BindGroupEntries::single(source),
    );
    let mut pass = context
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("hanabi_scene_input"),
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
