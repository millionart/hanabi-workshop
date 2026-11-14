fn source_state_component(data: texture_2d<f32>, slot: u32, field: u32) -> vec4<f32> {
    let width = textureDimensions(data).x / SNAPSHOT_FIELDS;
    let pixel = vec2<i32>(i32((slot % width) * SNAPSHOT_FIELDS + field), i32((slot / width) * 4u));
    let a = vec4<u32>(round(textureLoad(data, pixel, 0) * 255.0));
    let b = vec4<u32>(round(textureLoad(data, pixel + vec2<i32>(0, 1), 0) * 255.0));
    let c = vec4<u32>(round(textureLoad(data, pixel + vec2<i32>(0, 2), 0) * 255.0));
    let d = vec4<u32>(round(textureLoad(data, pixel + vec2<i32>(0, 3), 0) * 255.0));
    return bitcast<vec4<f32>>(a | (b << vec4<u32>(8u)) | (c << vec4<u32>(16u)) | (d << vec4<u32>(24u)));
}
