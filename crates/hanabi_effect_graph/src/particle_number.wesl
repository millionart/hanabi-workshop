fn number_atlas_uv(uv: vec2<f32>, value: f32, limit: f32) -> vec2<f32> {
    let lower = floor(value);
    let fraction = value - lower;
    var number = clamp(select(lower, lower + 1.0, fraction > 0.5 || (fraction == 0.5 && u32(lower) % 2u == 1u)), 1.0, 999000000.0);
    let order = floor(log2(number) * 0.30103001);
    let group = floor(order / 3.0);
    var digits = order + 1.0;
    let shortened = limit < digits;
    if (shortened) {
        let scaled = number / trunc(exp2(group * 9.96578407));
        let delta = fma(scaled, 10.0, -number);
        digits = floor(log2(scaled * 10.0) * 0.30103001) + 1.0;
        number = fma(1.0, delta, number);
    }
    let cell = floor(limit * uv.x);
    let place = limit - 1.0 - cell;
    var digit = floor(fract(number / exp2(place * 3.32192802) * 0.1) * 10.0 + 0.0001);
    if (place >= digits) { digit = -1.0; }
    var column = floor(fract(abs(digit) * 0.25) * select(-4.0, 4.0, digit >= 0.0));
    var row = floor(digit * 0.25);
    if (shortened && place == 0.0 && group >= 1.0) { column = group - 1.0; row = 3.0; }
    return vec2<f32>((column + fract(limit * uv.x)) * 0.25, 1.0 - (0.75 + uv.y * 0.25 - row * 0.25));
}
