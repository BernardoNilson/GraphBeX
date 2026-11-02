// Draws every vertex as an instance of one sphere, lit by a fixed light.
// Adapted from Bevy's `custom_shader_instancing` example.

#import bevy_pbr::mesh_view_bindings::view

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(3) offset: vec3<f32>,
    @location(4) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

// Toward the light, the same direction for the whole scene.
const LIGHT: vec3<f32> = vec3<f32>(0.37, 0.56, 0.74);
const AMBIENT: f32 = 0.35;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let world = vertex.position + vertex.offset;

    var out: VertexOutput;
    out.clip_position = view.clip_from_world * vec4<f32>(world, 1.0);
    out.normal = vertex.normal;
    out.color = vertex.color;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let diffuse = max(dot(normalize(in.normal), LIGHT), 0.0);
    return vec4<f32>(in.color.rgb * (AMBIENT + (1.0 - AMBIENT) * diffuse), 1.0);
}
