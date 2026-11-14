use std::borrow::Cow;

use bevy::{
    math::Vec3,
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

/// Emit a GPU event when the particle is born or crosses the trail segment spacing.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct ParticleSpawnEventModifier {
    pub count: ExprHandle,
    pub child_index: u32,
    pub mode: u32,
    pub distance: f32,
    pub probability: f32,
}

pub fn register(registry: &AppTypeRegistry) {
    registry.write().register::<VelocityOverLifetimeModifier>();
    registry.write().register::<WorldAccelerationModifier>();
    registry.write().register::<CurlNoiseModifier>();
    registry.write().register::<ParticleSpawnEventModifier>();
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
        })
    });
}

impl Modifier for ParticleSpawnEventModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Update
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        if self.mode == 0 {
            Cow::Borrowed(&[Attribute::AGE])
        } else {
            Cow::Borrowed(&[Attribute::AGE, Attribute::POSITION, Attribute::F32X3_3])
        }
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, module: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        context.set_emits_gpu_spawn_events(true)?;
        let count = context.eval(module, self.count)?;
        let birth = "particle.age <= sim_params.delta_time";
        let condition = if self.mode == 0 {
            birth.to_owned()
        } else {
            format!(
                "({birth} || distance(particle.position, particle.f32x3_3) >= {:?})",
                self.distance.max(0.)
            )
        };
        let record = if self.mode == 0 {
            ""
        } else {
            "particle.f32x3_3 = particle.position;"
        };
        let chance = if self.probability >= 1. {
            String::new()
        } else {
            format!(" && frand() < {:?}", self.probability)
        };
        context.main_code.push_str(&format!("\nif (is_alive && {condition}{chance}) {{ {record} append_spawn_events_{}((*effect_metadata).base_child_index, particle_index, {count}); }}\n", self.child_index));
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
        context.main_code.push_str(&format!(
            "\nparticle.velocity += ({acceleration}) * sim_params.delta_time;\n"
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
        let mut code = format!("{{\nlet dt = sim_params.delta_time * ({factor});\n");
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
            format!("particle.position += ({motion}) * sim_params.delta_time;")
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
             {use_motion}\nlet angular = curl * ({rotation}) * (0.008726646 * sim_params.delta_time);\nparticle.f32_0 += angular.z;\n{pitch_yaw}\n\
             let scale_factor = max(vec3<f32>(0.0), vec3<f32>(1.0) + curl * (0.5 * ({size})));\n\
             particle.size3 {} {size_value};\n}}\n",
            self.quality, self.octaves, self.octave_multiplier, self.octave_scale, if self.size_from_lifetime { "*=" } else { "= particle.f32x3_0 *" },
        ));
        Ok(())
    }
}
