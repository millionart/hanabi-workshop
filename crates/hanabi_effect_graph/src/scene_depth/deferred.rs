use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::{
    asset::uuid::Uuid,
    core_pipeline::{
        core_3d::AlphaMask3d,
        deferred::{
            AlphaMask3dDeferred, DEFERRED_LIGHTING_PASS_ID_FORMAT, DEFERRED_PREPASS_FORMAT,
        },
    },
    platform::collections::HashMap,
    prelude::*,
    render::{
        render_phase::{
            BinnedPhaseItem, BinnedRenderPhaseType, Draw, DrawError, DrawFunctionId, DrawFunctions,
            InputUniformIndex, PhaseItemExtraIndex, TrackedRenderPass, ViewBinnedRenderPhases,
        },
        render_resource::*,
        view::RetainedViewEntity,
    },
    shader::{Shader, ShaderDefVal},
};

#[derive(Default, Resource)]
pub(super) struct DeferredPipelines {
    shaders: HashMap<AssetId<Shader>, Handle<Shader>>,
    pipelines: HashMap<CachedRenderPipelineId, CachedRenderPipelineId>,
    draw_functions: HashMap<Entity, DrawFunctionId>,
    moved: HashMap<RetainedViewEntity, Vec<AlphaMask3d>>,
}

impl DeferredPipelines {
    pub(super) fn moved(&self, view: RetainedViewEntity) -> &[AlphaMask3d] {
        self.moved.get(&view).map_or(&[], Vec::as_slice)
    }

    pub(super) fn remove_shader(&mut self, original: AssetId<Shader>, cache: &mut PipelineCache) {
        if let Some(shader) = self.shaders.remove(&original) {
            cache.remove_shader(shader.id());
        }
    }

    pub(super) fn extract_shader(
        &mut self,
        original: AssetId<Shader>,
        shader: &Shader,
        cache: &mut PipelineCache,
    ) {
        let source = shader.source.as_str();
        if !source.contains(crate::particle_material::DEFERRED_MARKER) {
            self.remove_shader(original, cache);
            return;
        }
        // Replace only the Hanabi 0.20 fragment entry point. Vertex and particle bindings are shared.
        const FRAGMENT: &str = "fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {";
        const VIEW: &str = "@group(0) @binding(0) var<uniform> view: View;";
        let Some((before, after)) = source.split_once(FRAGMENT) else {
            return;
        };
        let Some((body, end)) = after.rsplit_once("return color;") else {
            return;
        };
        if !before.contains(VIEW) {
            return;
        }
        let before = before.replacen(VIEW, "", 1);
        let variant = format!(
            r#"
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT;
import bevy_pbr::render::pbr_types::pbr_input_new;
import bevy_pbr::deferred::functions::deferred_gbuffer_from_pbr_input;
struct HanabiDeferredOutput {{ @location(2) surface: vec4<u32>, @location(3) pass_id: u32 }}
{before}
fn fragment(in: VertexOutput) -> HanabiDeferredOutput {{
    var hanabi_pbr = pbr_input_new();
    {body}
    return HanabiDeferredOutput(deferred_gbuffer_from_pbr_input(hanabi_pbr), 1u);
{end}
"#
        );
        let handle = self.shaders.entry(original).or_insert_with(|| {
            let mut hash = DefaultHasher::new();
            original.hash(&mut hash);
            let uuid =
                Uuid::from_u128(0x17a18c4abb59e10f0000000000000000 | u128::from(hash.finish()));
            Handle::Uuid(uuid, Default::default())
        });
        let path = format!("{}-deferred.wesl", shader.path.trim_end_matches(".wesl"));
        cache.set_shader(handle.id(), Shader::from_wesl(variant, path));
    }
}

pub(super) struct DrawDeferredParticle;

impl Draw<AlphaMask3dDeferred> for DrawDeferredParticle {
    fn draw<'w>(
        &mut self,
        world: &'w World,
        pass: &mut TrackedRenderPass<'w>,
        view: Entity,
        item: &AlphaMask3dDeferred,
    ) -> Result<(), DrawError> {
        let Some(&source) = world
            .resource::<DeferredPipelines>()
            .draw_functions
            .get(&item.representative_entity.0)
        else {
            return Ok(());
        };
        let mut batch = item.batch_set_key.clone();
        batch.draw_function = source;
        let item = AlphaMask3d::new(
            batch,
            item.bin_key.clone(),
            item.representative_entity,
            item.batch_range.clone(),
            PhaseItemExtraIndex::None,
        );
        world
            .resource::<DrawFunctions<AlphaMask3d>>()
            .write()
            .get_mut(source)
            .expect("Hanabi registered the original mask draw")
            .draw(world, pass, view, &item)
    }
}

pub(super) fn clear_deferred(
    mut phases: ResMut<ViewBinnedRenderPhases<AlphaMask3dDeferred>>,
    mut deferred: ResMut<DeferredPipelines>,
) {
    for (view, items) in deferred.moved.drain() {
        if let Some(phase) = phases.get_mut(&view) {
            for item in items {
                phase.remove(item.representative_entity.1);
            }
        }
    }
    deferred.draw_functions.clear();
}

pub(super) fn queue_deferred(
    mut forward: ResMut<ViewBinnedRenderPhases<AlphaMask3d>>,
    mut lighting: ResMut<ViewBinnedRenderPhases<AlphaMask3dDeferred>>,
    mut deferred: ResMut<DeferredPipelines>,
    cache: Res<PipelineCache>,
    draw_functions: Res<DrawFunctions<AlphaMask3dDeferred>>,
) {
    if deferred.shaders.is_empty() {
        return;
    }
    let draw_function = draw_functions.read().id::<DrawDeferredParticle>();
    for (view, phase) in forward.iter_mut() {
        let Some(target) = lighting.get_mut(view) else {
            continue;
        };
        let mut moved = Vec::new();
        for ((batch, bin), members) in &phase.non_mesh_items {
            let source = batch.pipeline;
            let pipeline = if let Some(&pipeline) = deferred.pipelines.get(&source) {
                pipeline
            } else {
                if matches!(
                    cache.get_render_pipeline_state(source),
                    CachedPipelineState::Queued
                ) {
                    continue;
                }
                let desc = cache.get_render_pipeline_descriptor(source);
                let Some(fragment) = desc.fragment.as_ref() else {
                    continue;
                };
                let Some(shader) = deferred.shaders.get(&fragment.shader.id()).cloned() else {
                    continue;
                };
                let mut desc = desc.clone();
                desc.label = Some("Hanabi deferred surface".into());
                desc.multisample = MultisampleState::default();
                desc.vertex.shader = shader.clone();
                let defs = ShaderDefVal::Bool("HANABI_DEFERRED".into(), true);
                desc.vertex.shader_defs.push(defs.clone());
                let fragment = desc.fragment.as_mut().unwrap();
                fragment.shader = shader;
                fragment.shader_defs.push(defs);
                fragment.targets = vec![
                    None,
                    None,
                    Some(DEFERRED_PREPASS_FORMAT.into()),
                    Some(DEFERRED_LIGHTING_PASS_ID_FORMAT.into()),
                ];
                let pipeline = cache.queue_render_pipeline(desc);
                deferred.pipelines.insert(source, pipeline);
                pipeline
            };
            if cache.get_render_pipeline(pipeline).is_none() {
                continue;
            }
            let mut shaded = batch.clone();
            shaded.pipeline = pipeline;
            shaded.draw_function = draw_function;
            for (&main, &render) in &members.entities {
                deferred.draw_functions.insert(render, batch.draw_function);
                target.add(
                    shaded.clone(),
                    bin.clone(),
                    (render, main),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
                moved.push(AlphaMask3d::new(
                    batch.clone(),
                    bin.clone(),
                    (render, main),
                    0..1,
                    PhaseItemExtraIndex::None,
                ));
            }
        }
        for item in &moved {
            phase.remove(item.representative_entity.1);
        }
        if !moved.is_empty() {
            debug!(
                draws = moved.len(),
                "Hanabi mask geometry queued for Bevy deferred lighting"
            );
            deferred.moved.insert(*view, moved);
        }
    }
}
