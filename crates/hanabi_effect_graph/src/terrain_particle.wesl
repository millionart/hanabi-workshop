fn terrain_projected(tex: texture_2d<f32>, samp: sampler, p: vec3<f32>, scale: vec2<f32>, weights: vec3<f32>) -> vec4<f32> {
    return textureSample(tex, samp, p.zy * scale) * weights.x
        + textureSample(tex, samp, p.xz * scale) * weights.y
        + textureSample(tex, samp, p.xy * scale) * weights.z;
}

fn terrain_half_step(value: f32) -> f32 {
    let lower = floor(value);
    let fraction = value - lower;
    let even = u32(lower) % 2u == 0u;
    return select(lower + 1.0, lower, fraction < 0.5 || (fraction == 0.5 && even));
}

fn terrain_height_mix(alpha: vec3<f32>, paint: vec3<f32>, depth: f32) -> vec4<f32> {
    let raw = alpha * paint;
    let peak = max(max(raw.x, raw.y), raw.z) - depth;
    let clipped = max(raw - peak, vec3<f32>(0.0));
    let continuous = clipped / max(clipped.x + clipped.y + clipped.z, 1e-8);
    // The source snaps to half steps using ties-to-even.
    let scaled = continuous * 2.0;
    let snapped = vec3<f32>(terrain_half_step(scaled.x), terrain_half_step(scaled.y), terrain_half_step(scaled.z));
    return vec4<f32>(snapped / max(snapped.x + snapped.y + snapped.z, 1e-8), length(clipped));
}
