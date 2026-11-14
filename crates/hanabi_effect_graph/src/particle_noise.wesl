const unity_permutation = array<i32, 256>(
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225,
    140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148,
    247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32,
    57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122,
    60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54,
    65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169,
    200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64,
    52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212,
    207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213,
    119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9,
    129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104,
    218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241,
    81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157,
    184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93,
    222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
);

fn unity_perm(i: i32) -> i32 {
    return unity_permutation[u32(i & 255)];
}

fn unity_derivative1(x: f32, frequency: f32) -> f32 {
    let p = x * frequency;
    let f = floor(p);
    let t = p - f;
    let i = i32(f);
    let g0 = select(-1.0, 1.0, (unity_perm(i) & 1) == 0);
    let g1 = select(-1.0, 1.0, (unity_perm(i + 1) & 1) == 0);
    let fade = ((t * t) * t) * (t * (6.0 * t - 15.0) + 10.0);
    let slope = ((30.0 * t) * t) * ((t - 2.0) * t + 1.0);
    let delta = g1 * (t - 1.0) - g0 * t;
    return (2.0 * frequency) * ((delta * slope + g0) + (g1 - g0) * fade);
}

fn unity_gradient_mix(a: vec3<f32>, b: vec3<f32>, weight: f32, slope: f32, axis: u32) -> vec3<f32> {
    let delta = b - a;
    var value = delta * weight + a;
    if (axis != 0u) {
        value[axis] += delta.x * slope;
    }
    return value;
}

fn unity_derivative2(point: vec2<f32>, frequency: f32) -> vec2<f32> {
    let p = point * frequency;
    let f = floor(p);
    let t = p - f;
    let i = vec2<i32>(f);
    let q = 0.70710677;
    let gradients = array<vec2<f32>, 8>(
        vec2<f32>(1,0), vec2<f32>(-1,0), vec2<f32>(0,1), vec2<f32>(0,-1),
        vec2<f32>(q,q), vec2<f32>(-q,q), vec2<f32>(q,-q), vec2<f32>(-q,-q),
    );
    let fade = (t*t*t) * (t * (t*6.0 - vec2<f32>(15.0)) + vec2<f32>(10.0));
    let slope = (t*t*30.0) * ((t - vec2<f32>(2.0)) * t + vec2<f32>(1.0));
    var corners: array<vec3<f32>, 4>;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let hash = unity_perm(unity_perm(i.x + x) + i.y + y);
            let gradient = gradients[u32(hash & 7)];
            let offset = t - vec2<f32>(f32(x), f32(y));
            corners[u32(x + 2*y)] = vec3<f32>(gradient.x*offset.x + gradient.y*offset.y, gradient);
        }
    }
    let a = unity_gradient_mix(corners[0], corners[1], fade.x, slope.x, 1u);
    let b = unity_gradient_mix(corners[2], corners[3], fade.x, slope.x, 1u);
    return unity_gradient_mix(a, b, fade.y, slope.y, 2u).yz * (1.41421354 * frequency);
}

fn unity_derivative3(point: vec3<f32>, frequency: f32) -> vec2<f32> {
    let p = point * frequency;
    let f = floor(p);
    let t = p - f;
    let i = vec3<i32>(f);
    let gradients = array<vec3<f32>, 16>(
        vec3<f32>(1,1,0), vec3<f32>(-1,1,0), vec3<f32>(1,-1,0), vec3<f32>(-1,-1,0),
        vec3<f32>(1,0,1), vec3<f32>(-1,0,1), vec3<f32>(1,0,-1), vec3<f32>(-1,0,-1),
        vec3<f32>(0,1,1), vec3<f32>(0,-1,1), vec3<f32>(0,1,-1), vec3<f32>(0,-1,-1),
        vec3<f32>(1,1,0), vec3<f32>(-1,1,0), vec3<f32>(0,-1,1), vec3<f32>(0,-1,-1),
    );
    let fade = (t*t*t) * (t * (t*6.0 - vec3<f32>(15.0)) + vec3<f32>(10.0));
    let slope = (t*t*30.0) * ((t - vec3<f32>(2.0)) * t + vec3<f32>(1.0));
    var corners: array<vec3<f32>, 8>;
    for (var z = 0; z < 2; z++) {
        for (var y = 0; y < 2; y++) {
            for (var x = 0; x < 2; x++) {
                let hash = unity_perm(unity_perm(unity_perm(i.x + x) + i.y + y) + i.z + z);
                let gradient = gradients[u32(hash & 15)];
                let offset = t - vec3<f32>(f32(x), f32(y), f32(z));
                let contribution = (gradient.x*offset.x + gradient.y*offset.y) + gradient.z*offset.z;
                corners[u32(x + 2*y + 4*z)] = vec3<f32>(contribution, gradient.xy);
            }
        }
    }
    let x0 = unity_gradient_mix(corners[0], corners[1], fade.x, slope.x, 1u);
    let x1 = unity_gradient_mix(corners[2], corners[3], fade.x, slope.x, 1u);
    let x2 = unity_gradient_mix(corners[4], corners[5], fade.x, slope.x, 1u);
    let x3 = unity_gradient_mix(corners[6], corners[7], fade.x, slope.x, 1u);
    let y0 = unity_gradient_mix(x0, x1, fade.y, slope.y, 2u);
    let y1 = unity_gradient_mix(x2, x3, fade.y, slope.y, 2u);
    return unity_gradient_mix(y0, y1, fade.z, 0.0, 0u).yz * frequency;
}

fn unity_octaves(point: vec3<f32>, base_frequency: f32, octaves: u32, multiplier: f32, scale: f32) -> vec2<f32> {
    var frequency = base_frequency;
    var weight = 1.0;
    var total_weight = 1.0;
    var value = unity_derivative3(point, frequency);
    for (var i = 1u; i < octaves; i++) {
        frequency *= scale;
        weight *= multiplier;
        total_weight += weight;
        value += unity_derivative3(point, frequency) * weight;
    }
    return value / total_weight;
}

fn unity_curl(p: vec3<f32>, scroll: f32, frequency: f32, quality: u32, octaves: u32, multiplier: f32, scale: f32) -> vec3<f32> {
    if (quality == 1u) {
        let a = unity_derivative2(vec2<f32>(p.z, p.y + scroll), frequency);
        let b = unity_derivative2(vec2<f32>(p.x + 100.0, p.z + scroll), frequency);
        let c = unity_derivative2(vec2<f32>(p.y, p.x + 100.0 + scroll), frequency);
        return vec3<f32>(c.x - b.y, a.x - c.y, b.x - a.y);
    }
    if (quality == 2u) {
        let a = unity_octaves(vec3<f32>(p.z, p.y, p.x + scroll), frequency, octaves, multiplier, scale);
        let b = unity_octaves(vec3<f32>(p.x + 100.0, p.z, p.y + scroll), frequency, octaves, multiplier, scale);
        let c = unity_octaves(vec3<f32>(p.y, p.x + 100.0, p.z + scroll), frequency, octaves, multiplier, scale);
        return vec3<f32>(c.x - b.y, a.x - c.y, b.x - a.y);
    }
    let x = p.y + scroll;
    let y = p.z + scroll;
    let z = p.x + 100.0 + scroll;
    let value = vec3<f32>(unity_derivative1(x, frequency), unity_derivative1(y, frequency), unity_derivative1(z, frequency));
    if (octaves == 2u) {
        let f2 = frequency * scale;
        let second = vec3<f32>(unity_derivative1(x, f2), unity_derivative1(y, f2), unity_derivative1(z, f2));
        return (value + second * multiplier) / (1.0 + multiplier);
    }
    return value;
}
