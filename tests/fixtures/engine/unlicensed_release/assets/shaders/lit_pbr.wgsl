// Bhippi standard lit surface shader.
//
// Edit this file to change how lit materials are drawn. The material document
// (lit_pbr.shader.json) names this file; materials reference that document.

struct SurfaceInput {
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    uv: vec2<f32>,
};

struct SurfaceOutput {
    base_color: vec3<f32>,
    roughness: f32,
    metallic: f32,
    emissive: vec3<f32>,
};

fn surface(input: SurfaceInput) -> SurfaceOutput {
    var output: SurfaceOutput;
    output.base_color = material_base_color;
    output.roughness = material_roughness;
    output.metallic = material_metallic;
    output.emissive = material_emissive;
    return output;
}
