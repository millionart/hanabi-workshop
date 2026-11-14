import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@if(MULTISAMPLED)
@group(0) @binding(0) var scene_depth: texture_depth_multisampled_2d;
@else
@group(0) @binding(0) var scene_depth: texture_depth_2d;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let depth = textureLoad(scene_depth, vec2<i32>(in.position.xy), 0);
    // Preserve reversed-Z precision in a filterable material texture.
    let coarse = quantizeToF16(depth);
    return vec4<f32>(coarse, (depth - coarse) * 2048.0, 1.0, 1.0);
}
