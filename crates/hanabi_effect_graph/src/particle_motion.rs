use std::borrow::Cow;

use bevy::{
    math::{Vec3, Vec4},
    prelude::{AppTypeRegistry, Reflect},
};
use bevy_hanabi::{
    Attribute, BoxedModifier, EvalContext, ExprError, ExprHandle, Modifier, ModifierContext,
    Module, ShaderWriter, register_reflect_modifier,
};
use serde::{Deserialize, Serialize};

/// Additional velocity moves a particle without accumulating in its persistent velocity.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct VelocityOverLifetimeModifier {
    pub linear: ExprHandle,
    pub angular: ExprHandle,
    pub offset: ExprHandle,
    pub radial: ExprHandle,
    pub speed_factor: ExprHandle,
    pub world_space: bool,
    pub local_particles: bool,
    pub orbital: bool,
    #[serde(default)]
    pub noise: bool,
}

/// Apply world-space acceleration in either particle simulation space.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct WorldAccelerationModifier {
    pub acceleration: ExprHandle,
    pub local_particles: bool,
}

/// Apply particle drag using the current cross section and speed.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct ParticleDragModifier {
    pub drag: ExprHandle,
    pub multiply_by_size: bool,
    pub multiply_by_speed: bool,
}

/// Emit a GPU event on birth (0), trail spacing (1) or death (2).
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct ParticleSpawnEventModifier {
    pub count: ExprHandle,
    pub child_index: u32,
    pub mode: u32,
    pub distance: f32,
    pub probability: f32,
    #[serde(default)]
    pub suppress_snapshot: bool,
}

/// Emit from each parent particle at a constant rate after its own start delay.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct TimedSubEmitterModifier {
    pub particles_per_second: f32,
    pub start_delay_min: f32,
    pub start_delay_max: f32,
    pub duration: f32,
    pub looping: bool,
    pub probability: f32,
    pub child_index: u32,
}

/// Interpret the child's shape around the parent position already stored in world space.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct SubEmitterTransformModifier;

/// Draw position and direction from one editable emitter shape.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct EmissionShapeModifier {
    pub speed: ExprHandle,
    pub phase: ExprHandle,
    pub burst_size: ExprHandle,
    #[serde(default)]
    pub hierarchy_scale: Option<ExprHandle>,
    pub shape: u32,
    pub center: Vec3,
    pub scale: Vec3,
    pub orientation: Vec4,
    pub radius: f32,
    pub radius_thickness: f32,
    #[serde(default)]
    pub box_thickness: Vec3,
    pub minor_radius: f32,
    pub cone_angle: f32,
    pub cone_length: f32,
    pub arc_angle: f32,
    pub arc_mode: u32,
    pub arc_spread: f32,
    pub phase_speed: f32,
    pub random_direction: f32,
    pub random_position: f32,
    pub spherical_direction: f32,
    pub align_to_direction: bool,
    #[serde(default)]
    pub shape_data_slot: u32,
    #[serde(default)]
    pub triangle_count: u32,
    #[serde(default)]
    pub mesh_placement: u32,
    #[serde(default)]
    pub use_mesh_colors: bool,
}

/// Establish this emitter's simulation step and compensate Hanabi's default age integration.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct SimulationStepModifier {
    pub rate: ExprHandle,
}

/// Integrate persistent velocity with the emitter's simulation step.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct IntegrateMotionModifier;

/// Read the host's collision/prewarm snapshot when this emitter is driven by the game.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct ParticleSnapshotModifier {
    pub particle_state_slot: u32,
    pub use_snapshot: ExprHandle,
    #[serde(default)]
    pub flipbook_random: bool,
    #[serde(default)]
    pub mesh_variants: bool,
    #[serde(default)]
    pub trail_segments: bool,
    #[serde(default)]
    pub trail_triangles: bool,
}

/// Evaluate the authored trail curves at the two original endpoints.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct TrailAppearanceModifier {
    pub first_width: ExprHandle,
    pub width: ExprHandle,
    pub first_color: ExprHandle,
    pub color: ExprHandle,
    #[serde(default)]
    pub apply_to_preview: bool,
}

/// Attach scene lights to the live host particle samples.
#[derive(Clone, Copy, Debug, PartialEq, Reflect, Serialize, Deserialize)]
pub struct ParticleLightsModifier {
    pub color: Vec4,
    pub intensity: f32,
    pub range: f32,
    pub ratio: f32,
    pub max_lights: u32,
    pub random_distribution: bool,
    pub use_particle_color: bool,
    pub size_affects_range: bool,
    pub alpha_affects_intensity: bool,
}

impl ParticleLightsModifier {
    pub fn from_effect(asset: &bevy_hanabi::EffectAsset) -> Option<&Self> {
        asset
            .update_modifiers()
            .find_map(|modifier| modifier.as_reflect().downcast_ref::<Self>())
    }

    #[cfg(feature = "3d")]
    pub fn light(&self, color: Vec4, size: f32) -> bevy::prelude::PointLight {
        let color_multiplier = if self.use_particle_color {
            color
        } else {
            Vec4::ONE
        };
        let rgb = self.color * color_multiplier;
        let range = self.range * if self.size_affects_range { size } else { 1. };
        let intensity = self.intensity
            * if self.alpha_affects_intensity {
                color.w
            } else {
                1.
            };
        bevy::prelude::PointLight {
            color: bevy::prelude::Color::linear_rgba(rgb.x, rgb.y, rgb.z, 1.),
            intensity: intensity.max(0.) * (4. * std::f32::consts::PI),
            range: range.max(0.),
            ..Default::default()
        }
    }
}

impl Modifier for ParticleLightsModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, _: &mut ShaderWriter) -> Result<(), ExprError> {
        Ok(())
    }
}

pub const PARTICLE_STATE_SLOT: &str = "particle_state";
pub const TRAIL_STATE_SLOT: &str = "trail_state";
pub const PARTICLE_STATE_FIELDS: u32 = 7;

pub fn register(registry: &AppTypeRegistry) {
    registry.write().register::<VelocityOverLifetimeModifier>();
    registry.write().register::<WorldAccelerationModifier>();
    registry.write().register::<ParticleDragModifier>();
    registry.write().register::<CurlNoiseModifier>();
    registry.write().register::<ParticleSpawnEventModifier>();
    registry.write().register::<TimedSubEmitterModifier>();
    registry.write().register::<SubEmitterTransformModifier>();
    registry.write().register::<EmissionShapeModifier>();
    registry.write().register::<SimulationStepModifier>();
    registry.write().register::<IntegrateMotionModifier>();
    registry.write().register::<ParticleSnapshotModifier>();
    registry.write().register::<TrailAppearanceModifier>();
    registry.write().register::<ParticleLightsModifier>();
    register_reflect_modifier::<ParticleLightsModifier>(registry, |_| {
        Box::new(ParticleLightsModifier {
            color: Vec4::ONE,
            intensity: 1.,
            range: 1.,
            ratio: 1.,
            max_lights: 1,
            random_distribution: true,
            use_particle_color: false,
            size_affects_range: false,
            alpha_affects_intensity: false,
        })
    });
    register_reflect_modifier::<VelocityOverLifetimeModifier>(registry, |module| {
        let zero = module.lit(Vec3::ZERO);
        let radial = module.lit(0_f32);
        let speed_factor = module.lit(1_f32);
        Box::new(VelocityOverLifetimeModifier {
            linear: zero,
            angular: zero,
            offset: zero,
            radial,
            speed_factor,
            world_space: false,
            local_particles: true,
            orbital: false,
            noise: false,
        })
    });
    register_reflect_modifier::<WorldAccelerationModifier>(registry, |module| {
        Box::new(WorldAccelerationModifier {
            acceleration: module.lit(Vec3::Y),
            local_particles: true,
        })
    });
    register_reflect_modifier::<CurlNoiseModifier>(registry, |module| {
        let vector = module.lit(Vec3::ONE);
        let zero = module.lit(0_f32);
        Box::new(CurlNoiseModifier {
            strength: vector,
            seed_offset: module.lit(Vec3::ZERO),
            position_amount: module.lit(1_f32),
            rotation_amount: zero,
            size_amount: zero,
            scroll_speed: zero,
            frequency: 0.5,
            quality: 1,
            octaves: 1,
            octave_multiplier: 0.5,
            octave_scale: 2.,
            damping: true,
            local_particles: true,
            separate_size_axes: false,
            velocity_addition: false,
            size_from_lifetime: false,
            rotation_3d: false,
        })
    });
    register_reflect_modifier::<ParticleSpawnEventModifier>(registry, |module| {
        Box::new(ParticleSpawnEventModifier {
            count: module.lit(1_u32),
            child_index: 0,
            mode: 0,
            distance: 0.1,
            probability: 1.,
            suppress_snapshot: false,
        })
    });
    register_reflect_modifier::<ParticleDragModifier>(registry, |module| {
        Box::new(ParticleDragModifier {
            drag: module.lit(0.05_f32),
            multiply_by_size: true,
            multiply_by_speed: true,
        })
    });
    register_reflect_modifier::<TimedSubEmitterModifier>(registry, |_| {
        Box::new(TimedSubEmitterModifier {
            particles_per_second: 10.,
            start_delay_min: 0.,
            start_delay_max: 0.,
            duration: 1.,
            looping: false,
            probability: 1.,
            child_index: 0,
        })
    });
    register_reflect_modifier::<SubEmitterTransformModifier>(registry, |_| {
        Box::new(SubEmitterTransformModifier)
    });
    register_reflect_modifier::<EmissionShapeModifier>(registry, |module| {
        Box::new(EmissionShapeModifier {
            speed: module.lit(1_f32),
            phase: module.lit(-1_f32),
            burst_size: module.lit(1_u32),
            hierarchy_scale: None,
            shape: 0,
            center: Vec3::ZERO,
            scale: Vec3::ONE,
            orientation: Vec4::W,
            radius: 1.,
            radius_thickness: 1.,
            box_thickness: Vec3::ZERO,
            minor_radius: 0.2,
            cone_angle: 0.5,
            cone_length: 0.,
            arc_angle: std::f32::consts::TAU,
            arc_mode: 0,
            arc_spread: 0.,
            phase_speed: 1.,
            random_direction: 0.,
            random_position: 0.,
            spherical_direction: 0.,
            align_to_direction: false,
            shape_data_slot: u32::MAX,
            triangle_count: 0,
            mesh_placement: 2,
            use_mesh_colors: false,
        })
    });
    register_reflect_modifier::<SimulationStepModifier>(registry, |module| {
        Box::new(SimulationStepModifier {
            rate: module.lit(1_f32),
        })
    });
    register_reflect_modifier::<IntegrateMotionModifier>(registry, |_| {
        Box::new(IntegrateMotionModifier)
    });
    register_reflect_modifier::<ParticleSnapshotModifier>(registry, |module| {
        Box::new(ParticleSnapshotModifier {
            particle_state_slot: 0,
            use_snapshot: module.lit(0_u32),
            flipbook_random: false,
            mesh_variants: false,
            trail_segments: false,
            trail_triangles: false,
        })
    });
    register_reflect_modifier::<TrailAppearanceModifier>(registry, |module| {
        let width = module.lit(1_f32);
        let color = module.lit(Vec4::ONE);
        Box::new(TrailAppearanceModifier {
            first_width: width,
            width,
            first_color: color,
            color,
            apply_to_preview: false,
        })
    });
}

impl Modifier for TrailAppearanceModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[
            Attribute::SIZE3,
            Attribute::F32_7,
            Attribute::F32X2_2,
            Attribute::F32X2_3,
            Attribute::F32X4_0,
            Attribute::F32X4_1,
            Attribute::F32X4_2,
            Attribute::F32X4_3,
            Attribute::HDR_COLOR,
        ])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let first_width = context.eval(module, self.first_width)?;
        let width = context.eval(module, self.width)?;
        let first_color = context.eval(module, self.first_color)?;
        let color = context.eval(module, self.color)?;
        let preview = if self.apply_to_preview {
            format!("else {{ particle.size3 = vec3<f32>({width}); particle.hdr_color = {color}; }}")
        } else {
            String::new()
        };
        context.main_code.push_str(&format!(
            "
if particle.f32_7 > 0.0 {{
    particle.f32x2_3 = particle.size3.xy * vec2<f32>({first_width}, {width});
    particle.f32x4_2 = vec4<f32>(particle.f32x4_0.xyz, particle.f32x2_2.x) * ({first_color});
    particle.f32x4_3 = vec4<f32>(particle.f32x4_1.xyz, particle.f32x2_2.y) * ({color});
}} {preview}
"
        ));
        Ok(())
    }
}

impl Modifier for ParticleSnapshotModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        const ATTRIBUTES: &[Attribute] = &[
            Attribute::AGE,
            Attribute::LIFETIME,
            Attribute::POSITION,
            Attribute::VELOCITY,
            Attribute::SIZE3,
            Attribute::F32_0,
            Attribute::F32X2_0,
            Attribute::F32X2_1,
            Attribute::F32_6,
            Attribute::F32_7,
            Attribute::F32X3_2,
            Attribute::HDR_COLOR,
        ];
        if !self.flipbook_random
            && !self.mesh_variants
            && !self.trail_segments
            && !self.trail_triangles
        {
            return Cow::Borrowed(ATTRIBUTES);
        }
        let mut attributes = ATTRIBUTES.to_vec();
        if self.flipbook_random {
            attributes.extend([Attribute::F32_3, Attribute::F32_4]);
        }
        if self.mesh_variants {
            attributes.push(Attribute::F32_5);
        }
        if self.trail_segments {
            attributes.extend([
                Attribute::F32X3_0,
                Attribute::F32X3_1,
                Attribute::F32X4_0,
                Attribute::F32X4_1,
                Attribute::F32X2_2,
                Attribute::F32_1,
                Attribute::F32_2,
            ]);
        }
        if self.trail_triangles {
            attributes.extend([
                Attribute::AXIS_X,
                Attribute::AXIS_Y,
                Attribute::F32_1,
                Attribute::F32_2,
                Attribute::F32_3,
                Attribute::F32_4,
                Attribute::F32_5,
                Attribute::F32X2_2,
                Attribute::F32X3_0,
                Attribute::F32X3_1,
                Attribute::F32X3_3,
                Attribute::F32X4_0,
                Attribute::F32X4_1,
            ]);
        }
        Cow::Owned(attributes)
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let active = context.eval(module, self.use_snapshot)?;
        if !context.extra_code.contains("fn source_state_component(") {
            context.extra_code.push_str(&format!(
                "\nconst SNAPSHOT_FIELDS: u32 = {PARTICLE_STATE_FIELDS}u;\n"
            ));
            context
                .extra_code
                .push_str(include_str!("particle_snapshot.wesl"));
        }
        let texture = format!("material_texture_{}", self.particle_state_slot);
        if self.trail_triangles {
            context.main_code.push_str(&format!(
                "
if (({active}) != 0u) {{
    let a = source_state_component({texture}, particle_index, 0u);
    let b = source_state_component({texture}, particle_index, 1u);
    let c = source_state_component({texture}, particle_index, 2u);
    let first = source_state_component({texture}, particle_index, 3u);
    let second = source_state_component({texture}, particle_index, 4u);
    let first_normal = source_state_component({texture}, particle_index, 5u);
    let second_normal = source_state_component({texture}, particle_index, 6u);
    particle.position = first.xyz;
    particle.f32x3_0 = second.xyz;
    particle.f32x3_1 = a.xyz;
    particle.f32x3_2 = b.xyz;
    particle.f32x3_3 = c.xyz;
    particle.f32_3 = a.w;
    particle.f32_4 = b.w;
    particle.f32_5 = c.w;
    particle.f32_1 = first.w;
    particle.f32_2 = second.w;
    particle.axis_x = first_normal.xyz;
    particle.axis_y = second_normal.xyz;
    particle.f32x2_1 = vec2<f32>(first_normal.w, second_normal.w);
    particle.f32x2_2 = vec2<f32>(1.0);
    particle.f32x4_0 = vec4<f32>(1.0);
    particle.f32x4_1 = vec4<f32>(1.0);
    particle.size3 = vec3<f32>(1.0);
    particle.f32_7 = select(-1.0, 1.0, a.w >= 0.0);
    particle.age = 0.0;
    is_alive = true;
}}
"
            ));
            return Ok(());
        }
        if self.trail_segments {
            context.main_code.push_str(&format!(
                "
if (({active}) != 0u) {{
let first = source_state_component({texture}, particle_index, 0u);
let second = source_state_component({texture}, particle_index, 1u);
let first_tangent = source_state_component({texture}, particle_index, 2u);
let second_tangent = source_state_component({texture}, particle_index, 3u);
particle.position = first.xyz;
particle.f32x3_0 = second.xyz;
particle.f32x3_1 = first_tangent.xyz;
particle.f32x3_2 = second_tangent.xyz;
particle.f32x4_0 = source_state_component({texture}, particle_index, 4u);
particle.f32x4_1 = source_state_component({texture}, particle_index, 5u);
let alpha_random = source_state_component({texture}, particle_index, 6u);
particle.f32x2_2 = alpha_random.xy;
particle.f32x2_1 = vec2<f32>(first.w, second.w);
particle.f32_1 = alpha_random.z;
particle.f32_2 = alpha_random.w;
particle.size3 = vec3<f32>(first_tangent.w, second_tangent.w, 1.0);
particle.f32_7 = select(-1.0, 1.0, first.w >= 0.0);
particle.age = 0.0;
is_alive = true;
}}
"
            ));
            return Ok(());
        }
        let mut random = String::new();
        if self.flipbook_random || self.mesh_variants {
            random.push_str(&format!(
                "let extra = source_state_component({texture}, slot, 6u);\n"
            ));
        }
        if self.flipbook_random {
            random.push_str("particle.f32_3 = extra.x;\nparticle.f32_4 = extra.y;\n");
        }
        if self.mesh_variants {
            random.push_str("particle.f32_5 = extra.z;\n");
        }
        context.main_code.push_str(&format!("\nif (({active}) != 0u) {{\nlet slot = particle_index;\nlet posed = source_state_component({texture}, slot, 0u);\nlet age = posed.w;\nis_alive = true;\nparticle.f32_7 = select(-1.0, 1.0, age >= 0.0);\nparticle.position = posed.xyz;\nparticle.age = max(age, 0.0);\nlet movement = source_state_component({texture}, slot, 1u);\nparticle.velocity = movement.xyz;\nparticle.lifetime = movement.w;\nlet extent = source_state_component({texture}, slot, 2u);\nparticle.size3 = extent.xyz;\nparticle.f32_0 = extent.w;\nlet angle = source_state_component({texture}, slot, 3u);\nparticle.f32x2_0 = angle.xy;\nparticle.f32x2_1 = angle.zw;\nlet extras = source_state_component({texture}, slot, 4u);\nparticle.f32x3_2 = vec3<f32>(extras.xyz);\nparticle.f32_6 = extras.w;\nparticle.hdr_color = source_state_component({texture}, slot, 5u);\n{random}}}\n"));
        Ok(())
    }
}

impl Modifier for SimulationStepModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::AGE, Attribute::LIFETIME])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        if context.main_code.contains("let source_delta_time =") {
            return Err(ExprError::TypeError(
                "Only one simulation step node is allowed per emitter".into(),
            ));
        }
        let rate = context.eval(module, self.rate)?;
        context.main_code.push_str(&format!("\nlet source_delta_time = sim_params.delta_time * max({rate}, 0.0);\nparticle.age += source_delta_time - sim_params.delta_time;\nis_alive = was_alive && particle.age < particle.lifetime;\n"));
        Ok(())
    }
}

impl Modifier for IntegrateMotionModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let delta = simulation_delta(context);
        context.main_code.push_str(&format!(
            "\nparticle.position += particle.velocity * {delta};\n"
        ));
        Ok(())
    }
}

fn simulation_delta(context: &ShaderWriter) -> &'static str {
    if context.main_code.contains("let source_delta_time =") {
        "source_delta_time"
    } else {
        "sim_params.delta_time"
    }
}

impl Modifier for EmissionShapeModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Init
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        if self.align_to_direction || self.use_mesh_colors {
            Cow::Borrowed(&[
                Attribute::POSITION,
                Attribute::VELOCITY,
                Attribute::F32X2_0,
                Attribute::HDR_COLOR,
            ])
        } else {
            Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
        }
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let speed = context.eval(module, self.speed)?;
        let phase = context.eval(module, self.phase)?;
        let count = context.eval(module, self.burst_size)?;
        if !context.extra_code.contains("fn source_shape_sample(") {
            context
                .extra_code
                .push_str(include_str!("emission_shape.wesl"));
        }
        let vector = |v: Vec3| format!("vec3<f32>({:?}, {:?}, {:?})", v.x, v.y, v.z);
        let [x, y, z, w] = self.orientation.to_array();
        let center = vector(self.center);
        let scale = vector(self.scale);
        let hierarchy_scale = match self.hierarchy_scale {
            Some(scale) => context.eval(module, scale)?,
            None => "vec3<f32>(1.0)".into(),
        };
        let aligned = if self.align_to_direction {
            "let horizontal = dot(direction.xz, direction.xz);\nlet yaw = select(select(3.14159265359, -sign(direction.y) * 1.57079632679, direction.y != 0.0), atan2(direction.x, -direction.z), horizontal > 1e-30);\nparticle.f32x2_0 += vec2<f32>(-atan2(direction.y, sqrt(horizontal)), yaw);"
        } else {
            ""
        };
        let sample = if self.shape == 6 && self.triangle_count > 0 {
            if self.shape_data_slot == u32::MAX {
                return Err(ExprError::TypeError(
                    "Mesh emission requires a bound mesh data texture".into(),
                ));
            }
            let texture = format!("material_texture_{}", self.shape_data_slot);
            let color = if self.use_mesh_colors {
                format!(
                    "let tint = source_mesh_color({texture}, lookup);\nlet initial = vec4<u32>(floor(particle.hdr_color * 255.0 + 0.5));\nlet product = initial * tint + 128u;\nparticle.hdr_color = vec4<f32>((product + (product >> vec4<u32>(8u))) >> vec4<u32>(8u)) / 255.0;\n"
                )
            } else {
                String::new()
            };
            format!(
                "let lookup = source_mesh_lookup({texture}, {}u, {}u);\nlet sample = mat2x3<f32>(source_mesh_mix({texture}, lookup.origin, lookup.weights, 1).xyz, source_mesh_mix({texture}, lookup.origin, lookup.weights, 4).xyz);\n{color}",
                self.triangle_count, self.mesh_placement
            )
        } else {
            format!(
                "let phase = select(sim_params.time * {:?}, {phase}, {phase} >= 0.0);\nlet sample = source_shape_sample({}u, {}u, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, phase, particle_counter, {count}, {scale}, {});",
                self.phase_speed,
                self.shape,
                self.arc_mode,
                self.radius,
                self.radius_thickness,
                self.minor_radius,
                self.cone_angle,
                self.cone_length,
                self.arc_angle,
                self.arc_spread,
                self.random_direction,
                self.random_position,
                self.spherical_direction,
                vector(self.box_thickness),
            )
        };
        context.main_code.push_str(&format!("\n{{\n{sample}\nlet rotation = vec4<f32>({x:?}, {y:?}, {z:?}, {w:?});\nlet direction = source_shape_rotate(rotation, sample[1]);\nlet mirror = vec3<f32>(1.0, 1.0, -1.0);\nparticle.position = (source_shape_rotate(rotation, sample[0] * {scale}) + {center}) * mirror * {hierarchy_scale};\nparticle.velocity = direction * mirror * ({speed});\n{aligned}\n}}\n"));
        Ok(())
    }
}

impl Modifier for ParticleSpawnEventModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        if self.mode == 1 && self.suppress_snapshot {
            Cow::Borrowed(&[
                Attribute::AGE,
                Attribute::POSITION,
                Attribute::F32X3_3,
                Attribute::F32_7,
            ])
        } else if self.mode == 1 {
            Cow::Borrowed(&[Attribute::AGE, Attribute::POSITION, Attribute::F32X3_3])
        } else {
            Cow::Borrowed(&[Attribute::AGE])
        }
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        context.set_emits_gpu_spawn_events(true)?;
        let count = context.eval(module, self.count)?;
        let birth = format!("particle.age <= {}", simulation_delta(context));
        let condition = match self.mode {
            1 => format!(
                "is_alive && ({birth} || distance(particle.position, particle.f32x3_3) >= {:?})",
                self.distance.max(0.)
            ),
            2 => "was_alive && !is_alive".into(),
            _ => format!("is_alive && {birth}"),
        };
        let record = if self.mode == 1 {
            "particle.f32x3_3 = particle.position;"
        } else {
            ""
        };
        let chance = if self.probability >= 1. {
            String::new()
        } else {
            format!(" && frand() < {:?}", self.probability)
        };
        let active = if self.suppress_snapshot {
            " && particle.f32_7 == 0.0"
        } else {
            ""
        };
        context.main_code.push_str(&format!("\nif ({condition}{chance}{active}) {{ {record} append_spawn_events_{}((*effect_metadata).base_child_index, particle_index, {count}); }}\n", self.child_index));
        Ok(())
    }
}

impl Modifier for TimedSubEmitterModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::AGE, Attribute::LIFETIME, Attribute::U32_1])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        context.set_emits_gpu_spawn_events(true)?;
        if !context.extra_code.contains("fn sub_emitter_random(") {
            // UnityPlayer's first draw from the particle seed plus the caller's salt.
            context.extra_code.push_str(
                r#"
fn sub_emitter_random(x: u32) -> f32 {
    let w = x * 0x6ab51b9du + 0x714acb3fu;
    let t = x ^ (x << 11u);
    let value = w ^ (w >> 19u) ^ t ^ (t >> 8u);
    return f32(value & 0x7fffffu) * bitcast<f32>(0x34000001u);
}
"#,
            );
        }
        let dt = simulation_delta(context);
        let duration = if self.looping {
            "particle.lifetime".into()
        } else {
            format!("{:?}", self.duration.max(0.))
        };
        // shortcut: Hanabi's birth identity stands in for Unity's particle seed until the spawn seeds are shared.
        // shortcut: use the current parent position until GPU events can carry per-birth interpolated data.
        context.main_code.push_str(&format!(
            r#"
if (was_alive && {dt} > 0.0 && {probability:?} > 0.0) {{
    let parent_seed = pcg_hash(particle.u32_1 ^ (*spawner).seed);
    if (sub_emitter_random(parent_seed + 0x5aa47f98u) <= {probability:?}) {{
        let delay = mix({delay_min:?}, {delay_max:?}, sub_emitter_random(parent_seed + 0x34bfa39au));
        let elapsed = min(particle.age, particle.lifetime) - delay;
        let previous = clamp(particle.age - {dt} - delay, 0.0, {duration});
        let current = clamp(elapsed, previous, {duration});
        let count = u32(floor(current * {rate:?}) - floor(previous * {rate:?}));
        append_spawn_events_{child}((*effect_metadata).base_child_index, particle_index, count);
    }}
}}
"#,
            probability = self.probability.clamp(0., 1.),
            delay_min = self.start_delay_min.max(0.),
            delay_max = self.start_delay_max.max(0.),
            rate = self.particles_per_second.max(0.),
            child = self.child_index,
        ));
        Ok(())
    }
}

impl Modifier for SubEmitterTransformModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Init
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        context.main_code.push_str(
            "\nparticle.position = (transform * vec4<f32>(particle.position, 0.0)).xyz;\nparticle.velocity = (transform * vec4<f32>(particle.velocity, 0.0)).xyz;\n",
        );
        Ok(())
    }
}

impl Modifier for WorldAccelerationModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::VELOCITY])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let acceleration = context.eval(module, self.acceleration)?;
        let acceleration = if self.local_particles {
            format!(
                "(unpack_compressed_transform((*spawner).inverse_transform) * vec4<f32>({acceleration}, 0.0)).xyz"
            )
        } else {
            acceleration
        };
        let delta = simulation_delta(context);
        context.main_code.push_str(&format!(
            "\nparticle.velocity += ({acceleration}) * {delta};\n"
        ));
        Ok(())
    }
}

impl Modifier for ParticleDragModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[Attribute::VELOCITY, Attribute::SIZE3])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let drag = context.eval(module, self.drag)?;
        let dt = simulation_delta(context);
        let area = if self.multiply_by_size {
            "3.1415927 * radius * radius"
        } else {
            "1.0"
        };
        let speed_factor = if self.multiply_by_speed {
            "dot(particle.velocity, particle.velocity)"
        } else {
            "1.0"
        };
        context.main_code.push_str(&format!(
            r#"
{{
    let speed = length(particle.velocity);
    if (speed > 1e-15) {{
        let radius = 0.5 * max(particle.size3.x, max(particle.size3.y, particle.size3.z));
        let loss = {area} * max({drag}, 0.0) * {speed_factor} * {dt};
        particle.velocity = (particle.velocity / speed) * max(speed - loss, 0.0);
    }}
}}
"#
        ));
        Ok(())
    }
}

impl Modifier for VelocityOverLifetimeModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        if self.noise {
            Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY, Attribute::F32X3_1])
        } else {
            Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
        }
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }

    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let linear = context.eval(module, self.linear)?;
        let factor = context.eval(module, self.speed_factor)?;
        let delta = simulation_delta(context);
        let mut code = format!("{{\nlet dt = {delta} * ({factor});\n");
        if !self.local_particles || self.world_space {
            code.push_str("let frame = unpack_compressed_transform((*spawner).transform);\n");
        }
        if self.world_space && self.local_particles || self.orbital && !self.local_particles {
            code.push_str(
                "let inverse_frame = unpack_compressed_transform((*spawner).inverse_transform);\n",
            );
        }
        let linear = match (self.local_particles, self.world_space) {
            (true, false) => linear,
            (false, false) => format!("(frame * vec4<f32>({linear}, 0.0)).xyz"),
            (local, true) => {
                code.push_str("let emitter_scale = vec3<f32>(length(frame[0].xyz), length(frame[1].xyz), length(frame[2].xyz));\n");
                let scaled = format!("(({linear}) * emitter_scale)");
                if local {
                    format!("(inverse_frame * vec4<f32>({scaled}, 0.0)).xyz")
                } else {
                    scaled
                }
            }
        };
        code.push_str(&format!("var motion = particle.velocity + {linear};\n"));
        if self.noise {
            code.push_str("motion += particle.f32x3_1;\n");
        }
        if self.orbital {
            let angular = context.eval(module, self.angular)?;
            let offset = context.eval(module, self.offset)?;
            let radial = context.eval(module, self.radial)?;
            let local = if self.local_particles {
                "particle.position"
            } else {
                "(inverse_frame * vec4<f32>(particle.position, 1.0)).xyz"
            };
            let transform_vector = |name: &str| {
                if self.local_particles {
                    name.into()
                } else {
                    format!("(frame * vec4<f32>({name}, 0.0)).xyz")
                }
            };
            code.push_str(&format!(
                "let before = {local};\nlet center = {offset};\nlet theta = ({angular}) * dt;\n\
                 let c = cos(theta);\nlet s = sin(theta);\nlet q = before - center;\n\
                 let r_z = vec3<f32>(c.z*q.x - s.z*q.y, s.z*q.x + c.z*q.y, q.z);\n\
                 let r_x = vec3<f32>(r_z.x, c.x*r_z.y - s.x*r_z.z, s.x*r_z.y + c.x*r_z.z);\n\
                 let r_y = vec3<f32>(c.y*r_x.x + s.y*r_x.z, r_x.y, -s.y*r_x.x + c.y*r_x.z);\n\
                 let displacement = r_y - q;\nlet radial = r_y * (({radial}) / max(length(r_y), 1e-8));\n\
                 particle.position += {};\nmotion += {};\n",
                transform_vector("displacement"), transform_vector("radial"),
            ));
        }
        code.push_str("particle.position += motion * dt;\n}\n");
        context.main_code.push_str(&code);
        Ok(())
    }
}

/// Source curl noise changes this step's motion, roll and size.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct CurlNoiseModifier {
    pub strength: ExprHandle,
    pub seed_offset: ExprHandle,
    pub position_amount: ExprHandle,
    pub rotation_amount: ExprHandle,
    pub size_amount: ExprHandle,
    pub scroll_speed: ExprHandle,
    pub frequency: f32,
    pub quality: u32,
    pub octaves: u32,
    pub octave_multiplier: f32,
    pub octave_scale: f32,
    pub damping: bool,
    pub local_particles: bool,
    pub separate_size_axes: bool,
    pub velocity_addition: bool,
    #[serde(default)]
    pub size_from_lifetime: bool,
    #[serde(default)]
    pub rotation_3d: bool,
}

impl Modifier for CurlNoiseModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }

    fn attributes(&self) -> Cow<'_, [Attribute]> {
        let mut attributes = vec![
            Attribute::POSITION,
            Attribute::SIZE3,
            Attribute::F32_0,
            Attribute::F32X3_0,
        ];
        if self.velocity_addition {
            attributes.push(Attribute::F32X3_1);
        }
        if self.rotation_3d {
            attributes.push(Attribute::F32X2_0);
        }
        Cow::Owned(attributes)
    }

    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }

    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        let strength = context.eval(module, self.strength)?;
        let seed_offset = context.eval(module, self.seed_offset)?;
        let position = context.eval(module, self.position_amount)?;
        let rotation = context.eval(module, self.rotation_amount)?;
        let size = context.eval(module, self.size_amount)?;
        let scroll = context.eval(module, self.scroll_speed)?;
        let delta = simulation_delta(context);
        let frequency = self.frequency.max(1e-6);
        let scale = if self.damping { frequency.recip() } else { 1. };
        if !context.extra_code.contains("fn unity_curl(") {
            context
                .extra_code
                .push_str(include_str!("particle_noise.wesl"));
        }
        let motion = if self.local_particles {
            format!("curl * ({position}) * vec3<f32>(1.0, 1.0, -1.0)")
        } else {
            format!(
                "(unpack_compressed_transform((*spawner).inverse_transform) * vec4<f32>(curl * ({position}) * vec3<f32>(1.0, 1.0, -1.0), 0.0)).xyz"
            )
        };
        let use_motion = if self.velocity_addition {
            format!("particle.f32x3_1 = {motion};")
        } else {
            format!("particle.position += ({motion}) * {delta};")
        };
        let size_value = if self.separate_size_axes {
            "scale_factor"
        } else {
            "vec3<f32>(scale_factor.x)"
        };
        let pitch_yaw = if self.rotation_3d {
            "particle.f32x2_0 += angular.xy;"
        } else {
            ""
        };
        context.main_code.push_str(&format!(
            "\n{{\nlet p = particle.position * vec3<f32>(1.0, 1.0, -1.0) + ({seed_offset}) * 100.0;\n\
             let curl = unity_curl(p, sim_params.time * ({scroll}), {frequency:?}, {}u, {}u, {:?}, {:?}) * ({strength}) * {scale:?};\n\
             {use_motion}\nlet angular = curl * ({rotation}) * (0.008726646 * {delta});\nparticle.f32_0 += angular.z;\n{pitch_yaw}\n\
             let scale_factor = max(vec3<f32>(0.0), vec3<f32>(1.0) + curl * (0.5 * ({size})));\n\
             particle.size3 {} {size_value};\n}}\n",
            self.quality, self.octaves, self.octave_multiplier, self.octave_scale, if self.size_from_lifetime { "*=" } else { "= particle.f32x3_0 *" },
        ));
        Ok(())
    }
}
