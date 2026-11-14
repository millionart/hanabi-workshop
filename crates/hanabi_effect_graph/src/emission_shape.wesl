fn source_shape_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

fn source_shape_normalize(v: vec3<f32>) -> vec3<f32> {
    let magnitude = length(v);
    return select(vec3<f32>(0.0, 0.0, 1.0), v / max(magnitude, 1e-20), magnitude > 1e-20);
}

fn source_shape_sphere(angle: f32, y: f32, hemisphere: bool) -> vec3<f32> {
    let z = select(2.0 * y - 1.0, y, hemisphere);
    return vec3<f32>(vec2<f32>(cos(angle), sin(angle)) * sqrt(max(1.0 - z * z, 0.0)), z);
}

fn source_shape_power(value: f32) -> f32 {
    let bits = bitcast<u32>(value);
    let m = bitcast<f32>((bits & 0x807fffffu) | 0x3f800000u) - 1.0;
    let log = (f32(bits >> 23u) - 127.0 + m * bitcast<f32>(0x3fb80d57u))
        + m * m * (m * bitcast<f32>(0x3e470bd9u) + bitcast<f32>(0xbf21dda4u));
    let power = max(log * (1.0 / 3.0), -127.0);
    let exponent = floor(power);
    let fraction = power - exponent;
    return bitcast<f32>(u32(i32(exponent) + 127) << 23u)
        * ((fraction * bitcast<f32>(0x3f2ea941u) + 1.0) + fraction * fraction * bitcast<f32>(0x3ea2ad7fu));
}

fn source_shape_donut_cos(turn: f32) -> f32 {
    let x = 0.25 - abs(turn - round(turn));
    let x2 = x * x;
    let x4 = x2 * x2;
    let x8 = x4 * x4;
    let lo = bitcast<f32>(0xc2255ddcu) * x2 + bitcast<f32>(0x40c90fdau);
    let hi = (bitcast<f32>(0xc2992322u) * x2 + bitcast<f32>(0x42a33422u)) * x4;
    return ((hi + lo) + bitcast<f32>(0x421ea0cdu) * x8) * x;
}

struct EmissionTriangle {
    origin: vec2<i32>,
    weights: vec3<f32>,
}

fn source_mesh_texel(data: texture_2d<f32>, pixel: vec2<i32>) -> vec4<f32> {
    return textureLoad(data, pixel, 0) + textureLoad(data, pixel + vec2<i32>(0, 1), 0) / 2048.0;
}

fn source_mesh_lookup(data: texture_2d<f32>, count: u32, mode: u32) -> EmissionTriangle {
    let width = textureDimensions(data).x / 10u;
    let ticket = frand();
    var low = 0u;
    var high = count - 1u;
    while (low < high) {
        let i = low + (high - low) / 2u;
        let coord = vec2<i32>(i32((i % width) * 10u), i32((i / width) * 2u));
        if (ticket < source_mesh_texel(data, coord).x) { high = i; }
        else { low = i + 1u; }
    }
    let coord = vec2<i32>(i32((low % width) * 10u), i32((low / width) * 2u));
    if (mode == 1u) {
        let range = source_mesh_texel(data, coord).xy;
        let x = clamp((ticket - range.y) / max(range.x - range.y, 1e-10), 0.0, 1.0) * 3.0;
        let a = clamp(1.0 - x, 0.0, 1.0) + clamp(x - 2.0, 0.0, 1.0);
        let b = clamp(x, 0.0, 1.0) - clamp(x - 1.0, 0.0, 1.0);
        let c = clamp(x - 1.0, 0.0, 1.0) - clamp(x - 2.0, 0.0, 1.0);
        return EmissionTriangle(coord, vec3<f32>(a, b, c));
    }
    let r = frand2();
    let uv = select(r, 1.0 - r, r.x + r.y > 1.0);
    return EmissionTriangle(coord, vec3<f32>(uv, 1.0 - uv.x - uv.y));
}

fn source_mesh_mix(data: texture_2d<f32>, pixel: vec2<i32>, weights: vec3<f32>, at: i32) -> vec4<f32> {
    return (source_mesh_texel(data, pixel + vec2<i32>(at + 1, 0)) * weights.y
        + source_mesh_texel(data, pixel + vec2<i32>(at, 0)) * weights.x)
        + source_mesh_texel(data, pixel + vec2<i32>(at + 2, 0)) * weights.z;
}

fn source_mesh_color(data: texture_2d<f32>, triangle: EmissionTriangle) -> vec4<u32> {
    let weights = vec3<u32>(triangle.weights * 255.0);
    var total = vec4<u32>(0u);
    for (var i = 0; i < 3; i += 1) {
        let rgba = vec4<u32>(floor(source_mesh_texel(data, triangle.origin + vec2<i32>(i + 7, 0)) * 255.0 + 0.5));
        let product = rgba * weights[i] + 128u;
        total += (product + (product >> vec4<u32>(8u))) >> vec4<u32>(8u);
    }
    return min(total, vec4<u32>(255u));
}

fn source_shape_sample(kind: u32, mode: u32, radius: f32, thickness: f32,
    minor_radius: f32, cone: f32, cone_length: f32, extent: f32, spread: f32,
    random_dir: f32, random_position: f32, spherical: f32, phase: f32,
    counter: u32, burst_size: u32, scale: vec3<f32>) -> mat2x3<f32> {
    let tau = bitcast<f32>(0x40c90fdau);
    let fallback = vec3<f32>(0.0, 0.0, 1.0);
    var position = vec3<f32>(0.0);
    var direction = fallback;
    var angle = 0.0;
    if (kind == 0u || kind == 2u || kind == 4u || kind == 8u || kind == 10u || kind == 17u) {
        switch mode {
            case 1u: { angle = fract(phase) * tau; }
            case 2u: { angle = (1.0 - abs(2.0 * fract(phase * 0.5) - 1.0)) * extent; }
            case 3u: { angle = f32(counter % max(burst_size, 1u)) / f32(max(burst_size, 1u)) * extent; }
            default: { angle = frand() * select(extent, tau, kind == 0u || kind == 2u); }
        }
        let step = extent * spread;
        if (step > 0.0) { angle = floor(angle / step) * step; }
    }
    switch kind {
        case 0u, 2u: {
            direction = source_shape_sphere(angle, frand(), kind == 2u);
            let t = 1.0 - thickness;
            let sample = frand();
            position = direction * (source_shape_power((1.0 - sample) + sample * t * t * t) * radius);
        }
        case 4u, 8u, 10u: {
            let unit = vec2<f32>(cos(angle), sin(angle));
            let sample = frand();
            if (kind == 10u) {
                let t = 1.0 - thickness;
                position = vec3<f32>(unit * sqrt((1.0 - t * t) * sample + t * t) * radius, 0.0);
                direction = vec3<f32>(unit, 0.0);
            } else {
                let disk = unit * sqrt((1.0 - sample) + sample * max(1.0 - thickness, 0.001));
                var aim = disk;
                if (kind == 4u && random_dir > 0.0) {
                    let r = frand() * tau;
                    let blend_sample = frand();
                    let other = vec2<f32>(cos(r), sin(r)) * sqrt((1.0 - blend_sample) + blend_sample * 0.001);
                    aim = disk + (other - disk) * random_dir;
                }
                direction = vec3<f32>(aim * sin(cone), cos(cone));
                position = vec3<f32>(disk * radius, 0.0);
                if (kind == 8u) { position += source_shape_normalize(direction) * (frand() * cone_length); }
            }
        }
        case 5u: { position = frand3() - 0.5; }
        case 12u: {
            var value = 0.0;
            let ratio = phase / max(radius, 1e-20);
            switch mode {
                case 1u: { value = fract(ratio); }
                case 2u: {
                    let t = abs(ratio % 2.0);
                    value = fract(select(t, 2.0 - t - 1e-6, t >= 1.0));
                }
                case 3u: { value = f32(counter % max(burst_size, 1u)) / f32(max(burst_size, 2u) - 1u); }
                default: { value = frand(); }
            }
            position.x = value * radius * 2.0 - radius;
            direction = vec3<f32>(0.0, 1.0, 0.0);
        }
        case 17u: {
            let ring_turn = angle * bitcast<f32>(0x3e22f983u);
            let ring = vec2<f32>(source_shape_donut_cos(ring_turn), source_shape_donut_cos(ring_turn - 0.25));
            let minor_turn = frand() * bitcast<f32>(0x40c90fdbu) * bitcast<f32>(0x3e22f983u);
            let c = source_shape_donut_cos(minor_turn);
            let s = source_shape_donut_cos(minor_turn - 0.25);
            let inner = max(1.0 - thickness, 0.001);
            let distance = minor_radius * (inner + (1.0 - inner) * frand());
            position = vec3<f32>(ring * (radius + distance * c), distance * s);
            direction = vec3<f32>(ring * c, s);
        }
        case 18u: { position = vec3<f32>(frand2() - 0.5, 0.0); }
        default: {}
    }
    direction = source_shape_normalize(direction);
    if (kind != 4u && (random_dir > 0.0 || spherical > 0.0)) {
        if (random_dir > 0.0) {
            let random = source_shape_sphere(frand() * tau, frand(), false);
            direction += (random - direction) * random_dir;
        }
        direction += (source_shape_normalize(position) - direction) * spherical;
        direction = source_shape_normalize(direction * scale);
    }
    if (random_position > 0.0) {
        position += source_shape_sphere(frand() * tau, frand(), false) * random_position;
    }
    return mat2x3<f32>(position, direction);
}
