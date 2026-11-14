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
    #[serde(default)]
    pub scene_depth_slot: u32,
    #[serde(default)]
    pub inverse_fade: f32,
    #[serde(default)]
    pub depth_offset: f32,
    #[serde(default)]
    pub fade_close_distance: f32,
    #[serde(default)]
    pub soft_power: f32,
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
            scene_depth_slot: 0,
            inverse_fade: 0.,
            depth_offset: 0.,
            fade_close_distance: 0.,
            soft_power: 1.,
        }
    }
}

pub fn register(registry: &AppTypeRegistry) {
    registry.write().register::<CloudRemapModifier>();
    registry.write().register::<BillboardGeometryModifier>();
    registry.write().register::<ParticleDistortionModifier>();
    registry.write().register::<DamageNumberModifier>();
    registry.write().register::<SnapshotColorModifier>();
    register_reflect_modifier::<CloudRemapModifier>(registry, |_| {
        Box::new(CloudRemapModifier::default())
    });
    register_reflect_modifier::<BillboardGeometryModifier>(registry, |_| {
        Box::new(BillboardGeometryModifier::default())
    });
    register_reflect_modifier::<ParticleDistortionModifier>(registry, |_| {
        Box::new(ParticleDistortionModifier::default())
    });
    register_reflect_modifier::<DamageNumberModifier>(registry, |_| {
        Box::new(DamageNumberModifier::default())
    });
    register_reflect_modifier::<SnapshotColorModifier>(registry, |_| {
        Box::new(SnapshotColorModifier)
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
        let depth_fade = self.inverse_fade > 0. && (self.intersection || self.use_ramp);
        if depth_fade {
            code.push_str(&format!("var depth_fade = 1.0;\nlet scene_sample = textureLoad(material_texture_{}, vec2<i32>(in.position.xy), 0);\nif (scene_sample.b > 0.0) {{\nlet raw_depth = scene_sample.r + scene_sample.g / 2048.0;\nlet scene_h = view.view_from_clip * vec4<f32>(0.0, 0.0, raw_depth, 1.0);\nlet fragment_h = view.view_from_clip * vec4<f32>(0.0, 0.0, in.position.z, 1.0);\nlet fragment_depth = -fragment_h.z / fragment_h.w;\nif (raw_depth > 0.0) {{\nlet scene_depth = -scene_h.z / scene_h.w;\ndepth_fade = clamp((scene_depth - fragment_depth - {:?}) * {:?}, 0.0, 1.0);\n}}\n", self.scene_depth_slot, self.depth_offset, self.inverse_fade));
            if self.intersection {
                code.push_str(&format!(
                    "depth_fade = pow(depth_fade, {:?});\n",
                    self.soft_power
                ));
            } else if self.fade_close_distance > 0. {
                code.push_str(&format!(
                    "depth_fade *= clamp((fragment_depth + {:?} - 0.5) * {:?}, 0.0, 1.0);\n",
                    self.depth_offset, self.fade_close_distance
                ));
            }
            code.push_str("}\n");
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
            let rim = if depth_fade {
                "rim + 1.0 - depth_fade"
            } else {
                "rim"
            };
            code.push_str(&format!("let facing = abs(dot(normalize(view.world_position - world_h.xyz / world_h.w), normalize(normal)));\nlet rim = pow(1.0 - clamp(facing, 0.0, 1.0), {:?}) * {:?};\nlet ramp_x = clamp(({rim}) * alpha * {:?}, 0.0, 1.0);\nlet ramp = {sample_ramp};\ncolor = ramp * vec4<f32>({:?}, {:?}, {:?}, {:?}) * vec4<f32>({:?}, {:?}, {:?}, {vertex_alpha});\n}}\n", self.rim_power, self.rim_strength, self.intersection_strength * self.alpha_boost * self.opacity * 4., self.tint.x, self.tint.y, self.tint.z, self.tint.w, self.boost, self.boost, self.boost));
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
            let depth = if depth_fade { " * depth_fade" } else { "" };
            let coords = format!(
                "vec2<f32>((alpha * {:?} + {:?}) * {:?}{fade}{depth}, 0.5)",
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

/// Preserve the host's original color gradient when particle state comes from a snapshot.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct SnapshotColorModifier;

impl Modifier for SnapshotColorModifier {
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
        Cow::Borrowed(&[Attribute::F32_7, Attribute::HDR_COLOR])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        Err(ExprError::InvalidModifierContext(
            bevy_hanabi::EvalContext::modifier_context(context),
            ModifierContext::Render,
            "Snapshot color is a vertex modifier.",
        ))
    }
}

impl RenderModifier for SnapshotColorModifier {
    fn apply_render(&self, _: &mut Module, context: &mut RenderContext) -> Result<(), ExprError> {
        context
            .vertex_code
            .push_str("\nif (particle.f32_7 > 0.0) { color = particle.hdr_color; }\n");
        Ok(())
    }
    fn boxed_render_clone(&self) -> Box<dyn RenderModifier> {
        Box::new(*self)
    }
    fn as_modifier(&self) -> &dyn Modifier {
        self
    }
}

/// Draw the source four-cell damage-number atlas on a particle billboard.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct DamageNumberModifier {
    pub atlas_slot: u32,
    pub tint: Vec4,
    pub critical: Vec4,
    pub texture_transform: Vec4,
    pub character_limit: f32,
}

impl Default for DamageNumberModifier {
    fn default() -> Self {
        Self {
            atlas_slot: 0,
            tint: Vec4::ONE,
            critical: Vec4::ONE,
            texture_transform: Vec4::new(1., 1., 0., 0.),
            character_limit: 4.,
        }
    }
}

impl Modifier for DamageNumberModifier {
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
        Cow::Borrowed(&[Attribute::F32X2_1])
    }
    fn boxed_clone(&self) -> BoxedModifier {
        Box::new(*self)
    }
    fn apply(&self, _: &mut Module, context: &mut ShaderWriter) -> Result<(), ExprError> {
        Err(ExprError::InvalidModifierContext(
            bevy_hanabi::EvalContext::modifier_context(context),
            ModifierContext::Render,
            "Damage Number is a fragment modifier.",
        ))
    }
}

impl RenderModifier for DamageNumberModifier {
    fn apply_render(&self, _: &mut Module, context: &mut RenderContext) -> Result<(), ExprError> {
        context.set_needs_uv();
        context.set_needs_particle_fragment();
        if !context.render_extra.contains("fn number_atlas_uv(") {
            context
                .render_extra
                .push_str(include_str!("particle_number.wesl"));
        }
        let vec4 = |v: Vec4| format!("vec4<f32>({:?}, {:?}, {:?}, {:?})", v.x, v.y, v.z, v.w);
        let transform = vec4(self.texture_transform);
        let tint = vec4(self.tint);
        let critical = vec4(self.critical);
        context.fragment_code.push_str(&format!("\n{{\nlet st = {transform};\nlet coords = uv * st.xy + st.zw;\nif (coords.x > 1.0) {{ discard; }}\nlet glyph = select(vec2<f32>(1234.0, 0.0), particle.f32x2_1, particle.f32x2_1.x != 0.0);\nlet atlas = number_atlas_uv(coords, glyph.x, {:?});\nlet texel = textureSample(material_texture_{slot}, material_sampler_{slot}, atlas);\nlet edge = (1.0 - texel.r) * clamp(glyph.y, 0.0, 1.0) * texel.a;\ncolor *= mix(texel, {critical}, edge) * {tint};\n}}\n", self.character_limit, slot = self.atlas_slot));
        Ok(())
    }
    fn boxed_render_clone(&self) -> Box<dyn RenderModifier> {
        Box::new(*self)
    }
    fn as_modifier(&self) -> &dyn Modifier {
        self
    }
}

/// Refract the current camera's opaque color using mask and normal textures.
#[derive(Clone, Copy, Debug, Reflect, Serialize, Deserialize)]
pub struct ParticleDistortionModifier {
    pub scene_color_slot: u32,
    pub scene_depth_slot: u32,
    pub mask_slot: u32,
    pub normal_slot: u32,
    pub mask_uv: Vec4,
    pub normal_uv: Vec4,
    pub distance_fade: Vec4,
    pub inverse_fade: f32,
    pub magnitude: f32,
    pub use_mask: bool,
    pub use_normal: bool,
}

impl Default for ParticleDistortionModifier {
    fn default() -> Self {
        let uv = Vec4::new(1., 1., 0., 0.);
        Self {
            scene_color_slot: 0,
            scene_depth_slot: 0,
            mask_slot: 0,
            normal_slot: 0,
            mask_uv: uv,
            normal_uv: uv,
            distance_fade: Vec4::new(0., 5., 250000., 200000.),
            inverse_fade: 0.,
            magnitude: 1.,
            use_mask: false,
            use_normal: false,
        }
    }
}

impl Modifier for ParticleDistortionModifier {
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
            "Distortion is a fragment modifier.",
        ))
    }
}

impl RenderModifier for ParticleDistortionModifier {
    fn apply_render(&self, _: &mut Module, context: &mut RenderContext) -> Result<(), ExprError> {
        context.set_needs_uv();
        let [near_zero, near_one, far_zero, far_one] = self.distance_fade.to_array();
        let mut code = format!(
            "{{\nlet fragment_h = view.view_from_clip * vec4<f32>(0.0, 0.0, in.position.z, 1.0);\nlet fragment_depth = -fragment_h.z / fragment_h.w;\nvar fade = clamp(min((fragment_depth - {near_zero:?}) / max({near_one:?} - {near_zero:?}, 1e-6), ({far_zero:?} - fragment_depth) / max({far_zero:?} - {far_one:?}, 1e-6)), 0.0, 1.0);\n"
        );
        if self.inverse_fade > 0. {
            code.push_str(&format!("let scene_sample = textureLoad(material_texture_{}, vec2<i32>(in.position.xy), 0);\nlet raw_depth = scene_sample.r + scene_sample.g / 2048.0;\nif (raw_depth > 0.0) {{\nlet scene_h = view.view_from_clip * vec4<f32>(0.0, 0.0, raw_depth, 1.0);\nlet scene_depth = -scene_h.z / scene_h.w;\nfade *= clamp((scene_depth - fragment_depth) * {:?}, 0.0, 1.0);\n}}\n", self.scene_depth_slot, self.inverse_fade));
        }
        let sample = |slot: u32, transform: Vec4| {
            let coords = format!(
                "(uv * {} + {})",
                vec2(transform.truncate().truncate()),
                vec2(Vec2::new(transform.z, transform.w))
            );
            format!("textureSample(material_texture_{slot}, material_sampler_{slot}, {coords})")
        };
        let mask = if self.use_mask {
            format!("{}.a", sample(self.mask_slot, self.mask_uv))
        } else {
            "1.0".into()
        };
        let bump = if self.use_normal {
            sample(self.normal_slot, self.normal_uv)
        } else {
            "vec4<f32>(0.5, 0.5, 1.0, 1.0)".into()
        };
        code.push_str(&format!("let alpha = {mask} * color.a * fade;\nif (alpha < 0.01) {{ discard; }}\nlet bump = {bump};\nlet normal = vec2<f32>(bump.r * bump.a, bump.g) * 2.0 - 1.0;\nif (sqrt(1.0 - min(dot(normal, normal), 1.0)) < 0.99) {{ discard; }}\nlet dims = vec2<f32>(textureDimensions(material_texture_{0}));\nlet screen = (in.position.xy + view.viewport.zw * (normal * {1:?} * alpha / fragment_depth)) / dims;\ncolor = vec4<f32>(textureSample(material_texture_{0}, material_sampler_{0}, screen).rgb, 1.0);\n}}\n", self.scene_color_slot, self.magnitude));
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
    #[serde(default = "default_render_alignment")]
    pub render_alignment: u32,
}

fn default_render_alignment() -> u32 {
    u32::MAX
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
            render_alignment: default_render_alignment(),
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
            Attribute::F32X2_0,
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
        } else if self.orientation <= 3 {
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
            if (self.orientation == 2 || self.orientation == 3) && self.render_alignment == u32::MAX
            {
                code.push_str("let roll = -particle.f32_0;\nlet axes = mat2x2<f32>(cos(roll), sin(roll), -sin(roll), cos(roll));\nlet first = axis_x * axes[0].x + axis_y * axes[0].y;\naxis_y = axis_x * axes[1].x + axis_y * axes[1].y;\naxis_x = first;\n");
            }
        }
        if self.render_alignment != u32::MAX && self.orientation != 1 {
            let alignment = self.render_alignment;
            if matches!(self.orientation, 0 | 4) {
                let basis = match alignment {
                    0 => "get_camera_rotation_effect_space()",
                    1 if self.local_particles => {
                        "unpack_compressed_transform_3x3_transpose(spawners[spawner_index].inverse_transform)"
                    }
                    2 if !self.local_particles => {
                        "transpose(unpack_compressed_transform_3x3_transpose(spawners[spawner_index].transform))"
                    }
                    3 | 4 => {
                        let facing = if alignment == 3 {
                            "get_camera_position_effect_space() - position"
                        } else {
                            "-particle.velocity"
                        };
                        code.push_str(&format!("let direction = {facing};\nlet camera_axes = get_camera_rotation_effect_space();\nlet forward = select(camera_axes[2], direction / max(length(direction), 1e-6), length(direction) > 1e-6);\nlet side = cross(camera_axes[1], forward);\nlet right = select(camera_axes[0], side / max(length(side), 1e-6), length(side) > 1e-6);\n"));
                        "mat3x3<f32>(right, cross(forward, right), forward)"
                    }
                    _ => "mat3x3<f32>(axis_x, axis_y, axis_z)",
                };
                code.push_str(&format!("let basis = {basis};\n"));
            } else {
                code.push_str("let basis = mat3x3<f32>(axis_x, axis_y, axis_z);\n");
            }
            let rotation = match (self.orientation, alignment) {
                (2 | 3, _) => "vec3<f32>(0.0, 0.0, -particle.f32_0)",
                (0, 0) => "-vec3<f32>(particle.f32x2_0, particle.f32_0)",
                (0, _) => "vec3<f32>(particle.f32x2_0, -particle.f32_0)",
                _ => "vec3<f32>(-particle.f32x2_0, particle.f32_0)",
            };
            code.push_str(&format!("let angle = {rotation};\nlet s = sin(angle);\nlet c = cos(angle);\nlet rx = mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, c.x, s.x), vec3<f32>(0.0, -s.x, c.x));\nlet ry = mat3x3<f32>(vec3<f32>(c.y, 0.0, -s.y), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(s.y, 0.0, c.y));\nlet rz = mat3x3<f32>(vec3<f32>(c.z, s.z, 0.0), vec3<f32>(-s.z, c.z, 0.0), vec3<f32>(0.0, 0.0, 1.0));\nlet rotated = basis * ry * rx * rz;\naxis_x = rotated[0];\naxis_y = rotated[1];\naxis_z = rotated[2];\n"));
        }
        if self.orientation < 4 {
            code.push_str(&format!(
                "let pivot = vec3<f32>({:?}, {:?}, {:?}) * size;\nposition += axis_x * pivot.x + axis_y * pivot.y + axis_z * pivot.z;\nsize *= particle.f32x3_2;\n",
                self.pivot.x, self.pivot.y, self.pivot.z,
            ));
        } else if context.particle_layout.contains(Attribute::F32_7) {
            code.push_str("if (particle.f32_7 > 0.0) { size *= particle.f32x3_2; }\n");
        }
        if context.particle_layout.contains(Attribute::F32_7) {
            code.push_str(
                "if (particle.f32_7 < 0.0) { position = vec3<f32>(0.0); size = vec3<f32>(0.0); }\n",
            );
        }
        code.push_str("}\n");
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
