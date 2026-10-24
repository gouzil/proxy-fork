@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> step: vec4<f32>;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> Vertex {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Vertex(vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0), uv);
}

fn blur(uv: vec2<f32>, direction: vec2<f32>) -> vec4<f32> {
    // Contiguous logical-pixel samples avoid the text ghosts produced by widely spaced taps.
    let weights = array<f32, 9>(
        0.133571220, 0.126352961, 0.106955472, 0.081015040, 0.054912770,
        0.033306279, 0.018076899, 0.008779440, 0.003815529,
    );
    var color = textureSample(source, source_sampler, uv) * weights[0];
    for (var i = 1u; i < 9u; i++) {
        let offset = direction * f32(i);
        color += textureSample(source, source_sampler, uv + offset) * weights[i];
        color += textureSample(source, source_sampler, uv - offset) * weights[i];
    }
    return vec4<f32>(color.rgb, 1.0);
}

@fragment
fn horizontal(input: Vertex) -> @location(0) vec4<f32> {
    return blur(input.uv, vec2<f32>(step.x, 0.0));
}

@fragment
fn vertical(input: Vertex) -> @location(0) vec4<f32> {
    return blur(input.uv, vec2<f32>(0.0, step.y));
}
