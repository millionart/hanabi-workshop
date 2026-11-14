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
        }
    }
}

pub fn register(registry: &AppTypeRegistry) {
    registry.write().register::<CloudRemapModifier>();
    register_reflect_modifier::<CloudRemapModifier>(registry, |_| {
        Box::new(CloudRemapModifier::default())
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
            } else {
                "vec4<f32>(1.0)".into()
            }
        };
        let mut code = format!(
            "{{\nlet mask_uv = {mask_uv};\nlet mask = {};\nvar alpha = mask.a * color.a * {:?};\n",
            mask("mask_uv"),
            self.tint.w
        );
        if self.use_cloud1 || self.use_cloud2 {
            let phase = if self.modulate_vertex_rgb {
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
            } else {
                code.push_str("alpha *= (c1 * c2) * 4.0;\n");
            }
        }
        let ramp = if self.use_ramp {
            let coords = format!(
                "vec2<f32>((alpha * {:?} + {:?}) * {:?}, 0.5)",
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
