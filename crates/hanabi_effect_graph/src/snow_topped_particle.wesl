fn snow_topped_basis(world: vec3<f32>, uv: vec2<f32>, normal: vec3<f32>) -> mat3x3<f32> {
    let px = dpdx(world);
    let py = dpdy(world);
    let ux = dpdx(uv);
    let uy = dpdy(uv);
    let sign_uv = sign(ux.x * uy.y - ux.y * uy.x);
    let t = (px * uy.y - py * ux.y) * sign_uv;
    let b = (py * ux.x - px * uy.x) * sign_uv;
    return mat3x3<f32>(
        t / max(length(t), 1e-8),
        b / max(length(b), 1e-8),
        normal,
    );
}

fn snow_topped_normal(packed: vec4<f32>, strength: f32) -> vec3<f32> {
    let xy = vec2<f32>(packed.r * packed.a, packed.g) * 2.0 - 1.0;
    let z = sqrt(1.0 - min(dot(xy, xy), 1.0));
    return mix(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(xy, z), strength);
}

fn snow_topped_weights(heights: vec3<f32>, depth: f32) -> vec3<f32> {
    let threshold = max(heights.x, max(heights.y, heights.z)) - depth;
    let delta = max(heights - threshold, vec3<f32>(0.0));
    let fraction = 2.0 * delta / max(dot(delta, vec3<f32>(1.0)), 1e-8);
    let weights = vec3<f32>(terrain_half_step(fraction.x), terrain_half_step(fraction.y), terrain_half_step(fraction.z));
    return weights / dot(weights, vec3<f32>(1.0));
}
