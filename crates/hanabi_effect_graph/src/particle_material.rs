use std::borrow::Cow;

use bevy::{
    math::{Vec2, Vec4},
    prelude::{AppTypeRegistry, Reflect},
};
use bevy_hanabi::{
    Attribute, BoxedModifier, ExprError, Modifier, ModifierContext, Module, RenderContext,
    RenderModifier, ShaderWriter, register_reflect_modifier,
};
use serde::{Deserialize, Serialize};

/// Map particle opacity through a color ramp, optionally using a scrolling cloud mask.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct CloudRemapModifier {
    pub mask_slot: u32,
    pub ramp_slot: u32,
    pub cloud1_slot: u32,
    pub cloud2_slot: u32,
    pub mask_uv: Vec4,
    pub ramp_uv: Vec4,
    pub cloud1_uv: Vec4,
    pub cloud2_uv: Vec4,
    pub cloud_scroll: Vec4,
    pub tint: Vec4,
    pub opacity: f32,
    pub alpha_boost: f32,
    pub alpha_bias: f32,
    pub boost: f32,
    pub distortion_strength: f32,
    pub modulate_vertex_rgb: bool,
    pub use_mask: bool,
    pub use_ramp: bool,
    pub use_cloud1: bool,
    pub use_cloud2: bool,
    pub displace_mask: bool,
    #[serde(default)]
    pub opaque_cutout: bool,
    #[serde(default)]
    pub fresnel_power: f32,
    #[serde(default)]
    pub inject_vertex_offset: f32,
    #[serde(default)]
    pub intersection: bool,
    #[serde(default)]
    pub intersection_strength: f32,
    #[serde(default)]
    pub rim_strength: f32,
    #[serde(default)]
    pub rim_power: f32,
    #[serde(default)]
    pub use_vertex_red_as_alpha: bool,
}

impl Default for CloudRemapModifier {
    fn default() -> Self {
        let uv = Vec4::new(1., 1., 0., 0.);
        Self {
            mask_slot: 0,
            ramp_slot: 0,
            cloud1_slot: 0,
            cloud2_slot: 0,
            mask_uv: uv,
            ramp_uv: uv,
            cloud1_uv: uv,
            cloud2_uv: uv,
            cloud_scroll: Vec4::ZERO,
            tint: Vec4::ONE,
            opacity: 1.,
            alpha_boost: 1.,
            alpha_bias: 0.,
            boost: 1.,
            distortion_strength: 0.,
            modulate_vertex_rgb: true,
            use_mask: false,
            use_ramp: false,
            use_cloud1: false,
            use_cloud2: false,
            displace_mask: false,
            opaque_cutout: false,
            fresnel_power: 0.,
            inject_vertex_offset: 0.,
            intersection: false,
            intersection_strength: 0.,
            rim_strength: 0.,
            rim_power: 1.,
            use_vertex_red_as_alpha: false,
        }
    }
}

pub fn register(registry: &AppTypeRegistry) {
    registry.write().register::<CloudRemapModifier>();
    registry.write().register::<BillboardGeometryModifier>();
    register_reflect_modifier::<CloudRemapModifier>(registry, |_| {
        Box::new(CloudRemapModifier::default())
    });
    register_reflect_modifier::<BillboardGeometryModifier>(registry, |_| {
        Box::new(BillboardGeometryModifier::default())
    });
}

impl Modifier for CloudRemapModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Render
    }
    fn as_render(&self) -> Option<&dyn RenderModifier> {
        Some(self)
    }
    fn as_render_mut(&mut self) -> Option<&mut dyn RenderModifier> {
        Some(self)
    }
    fn into_boxed_render(self: Box<Self>) -> Option<Box<dyn RenderModifier>> {
        Some(self)
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        Err(ExprError::InvalidModifierContext(
            bevy_hanabi::EvalContext::modifier_context(context),
            ModifierContext::Render,
            "Cloud remap is a fragment modifier.",
        ))
    }
}

impl RenderModifier for CloudRemapModifier {
    fn apply_render(&self, _: &mut Module, context: &mut RenderContext) -> Result<(), ExprError> {
        context.set_needs_uv();
        if self.inject_vertex_offset != 0. {
            context.set_needs_normal();
            context.vertex_code.push_str(&format!(
                "\nposition += axis_z * {:?};\n",
                self.inject_vertex_offset
            ));
        }
        let uv = |expr: &str, transform: Vec4| {
            format!(
                "({expr} * {} + {})",
                vec2(transform.truncate().truncate()),
                vec2(Vec2::new(transform.z, transform.w))
            )
        };
        let sample = |slot: u32, coords: &str| {
            format!("textureSample(material_texture_{slot}, material_sampler_{slot}, {coords})")
        };
        let mask_uv = uv("uv", self.mask_uv);
        let mask = |coord: &str| {
            if self.use_mask {
                sample(self.mask_slot, coord)
            } else if self.intersection {
                "vec4<f32>(0.5)".into()
            } else {
                "vec4<f32>(1.0)".into()
            }
        };
        let mut code = format!(
            "{{\nlet mask_uv = {mask_uv};\nlet mask = {};\nvar alpha = mask.a * color.a * {:?};\n",
            mask("mask_uv"),
            if self.opaque_cutout {
                self.tint.w * self.opacity * self.alpha_boost * 4.
            } else if self.intersection {
                self.tint.w * 0.25
            } else {
                self.tint.w
            }
        );
        if self.use_cloud1 || self.use_cloud2 {
            let phase = if self.modulate_vertex_rgb || self.opaque_cutout || self.intersection {
                "0.0"
            } else {
                "color.r + color.g + color.b"
            };
            code.push_str(&format!("let phase = sim_params.time / 20.0 + {phase};\n"));
            for (name, used, slot, transform, scroll) in [
                (
                    "c1",
                    self.use_cloud1,
                    self.cloud1_slot,
                    self.cloud1_uv,
                    Vec2::new(self.cloud_scroll.x, self.cloud_scroll.y),
                ),
                (
                    "c2",
                    self.use_cloud2,
                    self.cloud2_slot,
                    self.cloud2_uv,
                    Vec2::new(self.cloud_scroll.z, self.cloud_scroll.w),
                ),
            ] {
                let coords = uv("uv", transform);
                let coords = format!("{coords} + phase * {}", vec2(scroll));
                let alpha = if used {
                    format!("{}.a", sample(slot, &coords))
                } else if self.intersection {
                    "0.5".into()
                } else {
                    "1.0".into()
                };
                code.push_str(&format!("let {name} = {alpha};\n"));
            }
            if self.displace_mask {
                code.push_str(&format!(
                    "alpha *= {}.a * color.a * {:?};\n",
                    mask(&format!(
                        "mask_uv + vec2<f32>(0.0, (2.0 * (c1 + c2) - 1.0) * {:?})",
                        self.distortion_strength
                    )),
                    self.tint.w
                ));
            } else if self.opaque_cutout {
                code.push_str("alpha *= c1 * c2;\n");
            } else {
                code.push_str("alpha *= (c1 * c2) * 4.0;\n");
            }
        }
        if self.opaque_cutout {
            let ramp = if self.use_ramp {
                sample(self.ramp_slot, &uv("vec2<f32>(alpha, 0.5)", self.ramp_uv))
            } else {
                "vec4<f32>(0.5)".into()
            };
            code.push_str(&format!("let ramp = {ramp};\ncolor = vec4<f32>(ramp.rgb * color.rgb * vec3<f32>({:?}, {:?}, {:?}), alpha);\n}}\n", self.tint.x, self.tint.y, self.tint.z));
            context.fragment_code.push_str(&code);
            return Ok(());
        }
        if self.intersection {
            context.set_needs_normal();
            fragment_world_position(&mut code);
            let sample_ramp = if self.use_ramp {
                sample(self.ramp_slot, &uv("vec2<f32>(ramp_x, 0.5)", self.ramp_uv))
            } else {
                "vec4<f32>(0.5)".into()
            };
            let vertex_alpha = if self.use_vertex_red_as_alpha {
                "color.r"
            } else {
                "1.0"
            };
            code.push_str(&format!("let facing = abs(dot(normalize(view.world_position - world_h.xyz / world_h.w), normalize(normal)));\nlet rim = pow(1.0 - clamp(facing, 0.0, 1.0), {:?}) * {:?};\nlet ramp_x = clamp(rim * alpha * {:?}, 0.0, 1.0);\nlet ramp = {sample_ramp};\ncolor = ramp * vec4<f32>({:?}, {:?}, {:?}, {:?}) * vec4<f32>({:?}, {:?}, {:?}, {vertex_alpha});\n}}\n", self.rim_power, self.rim_strength, self.intersection_strength * self.alpha_boost * self.opacity * 4., self.tint.x, self.tint.y, self.tint.z, self.tint.w, self.boost, self.boost, self.boost));
            context.fragment_code.push_str(&code);
            return Ok(());
        }
        let fade = if self.fresnel_power != 0. && self.use_ramp {
            context.set_needs_normal();
            let facing = if self.fresnel_power > 0. {
                "1.0 - facing"
            } else {
                "facing"
            };
            fragment_world_position(&mut code);
            code.push_str(&format!("let facing = dot(normalize(view.world_position - world_h.xyz / world_h.w), normalize(normal));\nlet fresnel = pow(clamp({facing}, 0.0, 1.0), {:?});\n", self.fresnel_power.abs()));
            " * fresnel"
        } else {
            ""
        };
        let ramp = if self.use_ramp {
            let coords = format!(
                "vec2<f32>((alpha * {:?} + {:?}) * {:?}{fade}, 0.5)",
                self.alpha_boost, self.alpha_bias, self.opacity
            );
            sample(self.ramp_slot, &uv(&coords, self.ramp_uv))
        } else {
            format!(
                "vec4<f32>(mask.rgb, (alpha * {:?} + {:?}) * {:?})",
                self.alpha_boost, self.alpha_bias, self.opacity
            )
        };
        let rgb = if self.modulate_vertex_rgb {
            "color.rgb"
        } else {
            "vec3<f32>(1.0)"
        };
        code.push_str(&format!(
            "let ramp = {ramp};\ncolor = vec4<f32>(ramp.rgb * {rgb} * vec3<f32>({:?}, {:?}, {:?}) * {:?}, ramp.a * {:?});\n}}\n",
            self.tint.x, self.tint.y, self.tint.z, self.boost, if self.use_ramp { self.tint.w } else { 1. },
        ));
        context.fragment_code.push_str(&code);
        Ok(())
    }

    fn boxed_render_clone(&self) -> Box<dyn RenderModifier> {
        Box::new(*self)
    }
    fn as_modifier(&self) -> &dyn Modifier {
        self
    }
}

fn vec2(v: Vec2) -> String {
    format!("vec2<f32>({:?}, {:?})", v.x, v.y)
}

fn fragment_world_position(code: &mut String) {
    code.push_str("let ndc_xy = ((in.position.xy - view.viewport.xy) / view.viewport.zw) * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);\nlet world_h = view.world_from_clip * vec4<f32>(ndc_xy, in.position.z, 1.0);\n");
}

/// Preserve source billboard sizing, pivot and facing before expanding the mesh.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct BillboardGeometryModifier {
    pub orientation: u32,
    pub min_viewport_size: f32,
    pub max_viewport_size: f32,
    pub length_scale: f32,
    pub velocity_scale: f32,
    pub pivot: bevy::math::Vec3,
    pub local_particles: bool,
}

impl Default for BillboardGeometryModifier {
    fn default() -> Self {
        Self {
            orientation: 0,
            min_viewport_size: 0.,
            max_viewport_size: 1.,
            length_scale: 1.,
            velocity_scale: 0.,
            pivot: bevy::math::Vec3::ZERO,
            local_particles: true,
        }
    }
}

impl Modifier for BillboardGeometryModifier {
    fn context(&self) -> ModifierContext {
        ModifierContext::Render
    }
    fn as_render(&self) -> Option<&dyn RenderModifier> {
        Some(self)
    }
    fn as_render_mut(&mut self) -> Option<&mut dyn RenderModifier> {
        Some(self)
    }
    fn into_boxed_render(self: Box<Self>) -> Option<Box<dyn RenderModifier>> {
        Some(self)
    }
    fn attributes(&self) -> Cow<'_, [Attribute]> {
        Cow::Borrowed(&[
            Attribute::POSITION,
            Attribute::SIZE3,
            Attribute::VELOCITY,
            Attribute::F32_0,
            Attribute::F32X3_2,
        ])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        Err(ExprError::InvalidModifierContext(
            bevy_hanabi::EvalContext::modifier_context(context),
            ModifierContext::Render,
            "Billboard geometry is a vertex modifier.",
        ))
    }
}

impl RenderModifier for BillboardGeometryModifier {
    fn apply_render(&self, _: &mut Module, context: &mut RenderContext) -> Result<(), ExprError> {
        let mut code = String::from("\n{\n");
        if self.orientation == 1 {
            code.push_str(&format!(
                "let vel = length(particle.velocity);\nlet aim = select(vec3<f32>(0.0, 1.0, 0.0), particle.velocity / max(vel, 1e-6), vel > 1e-6);\n\
                 let side = cross(aim, get_camera_position_effect_space() - position);\n\
                 axis_x = select(get_camera_rotation_effect_space()[0], side / max(length(side), 1e-6), length(side) > 1e-6);\n\
                 axis_y = aim;\naxis_z = cross(axis_x, axis_y);\nsize.y = size.y * {:?} + vel * {:?};\n",
                self.length_scale, self.velocity_scale,
            ));
        } else {
            let scaling = if self.local_particles {
                "max(length(unpack_compressed_transform(spawners[spawner_index].transform)[0].xyz), 1e-5)"
            } else {
                "1.0"
            };
            code.push_str(&format!(
                "let clip_w = transform_position_simulation_to_clip(position).w;\n\
                 let viewport_width = 2.0 * clip_w / abs(view.clip_from_view[0].x) / {scaling};\n\
                 let largest = max(max(size.x, size.y), 1e-6);\nvar clamped = max(largest, viewport_width * {:?});\n",
                self.min_viewport_size,
            ));
            if self.max_viewport_size >= 0. {
                code.push_str(&format!(
                    "clamped = min(clamped, viewport_width * {:?});\n",
                    self.max_viewport_size
                ));
            }
            code.push_str("size *= clamped / largest;\n");
            match self.orientation {
                2 => code.push_str("axis_x = vec3<f32>(1.0,0.0,0.0);\naxis_y = vec3<f32>(0.0,0.0,-1.0);\naxis_z = vec3<f32>(0.0,1.0,0.0);\n"),
                3 => code.push_str("let camera = get_camera_position_effect_space();\nlet normal = (camera - position) * vec3<f32>(1.0,0.0,1.0);\naxis_z = select(vec3<f32>(0.0,0.0,1.0), normal / max(length(normal), 1e-6), length(normal) > 1e-6);\naxis_y = vec3<f32>(0.0,1.0,0.0);\naxis_x = cross(axis_y, axis_z);\n"),
                _ => {}
            }
            if self.orientation == 2 || self.orientation == 3 {
                code.push_str("let roll = -particle.f32_0;\nlet axes = mat2x2<f32>(cos(roll), sin(roll), -sin(roll), cos(roll));\nlet first = axis_x * axes[0].x + axis_y * axes[0].y;\naxis_y = axis_x * axes[1].x + axis_y * axes[1].y;\naxis_x = first;\n");
            }
        }
        code.push_str(&format!(
            "let pivot = vec3<f32>({:?}, {:?}, {:?}) * size;\nposition += axis_x * pivot.x + axis_y * pivot.y + axis_z * pivot.z;\nsize *= particle.f32x3_2;\n}}\n",
            self.pivot.x, self.pivot.y, self.pivot.z,
        ));
        context.vertex_code.push_str(&code);
        Ok(())
    }

    fn boxed_render_clone(&self) -> Box<dyn RenderModifier> {
        Box::new(*self)
    }
    fn as_modifier(&self) -> &dyn Modifier {
        self
    }
}
