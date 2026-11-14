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
}

/// Apply world-space acceleration in either particle simulation space.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct WorldAccelerationModifier {
    pub acceleration: ExprHandle,
    pub local_particles: bool,
}

pub fn register(registry: &AppTypeRegistry) {
    registry.write().register::<VelocityOverLifetimeModifier>();
    registry.write().register::<WorldAccelerationModifier>();
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
        })
    });
    register_reflect_modifier::<WorldAccelerationModifier>(registry, |module| {
        Box::new(WorldAccelerationModifier {
            acceleration: module.lit(Vec3::Y),
            local_particles: true,
        })
    });
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
        Cow::Borrowed(&[Attribute::POSITION, Attribute::VELOCITY])
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
