fn source_state_component(data: texture_2d<f32>, slot: u32, field: u32) -> vec4<f32> {
    let width = textureDimensions(data).x / 6u;
    let pixel = vec2<i32>(i32((slot % width) * 6u + field), i32((slot / width) * 2u));
    return textureLoad(data, pixel, 0) + textureLoad(data, pixel + vec2<i32>(0, 1), 0) / 2048.0;
}
