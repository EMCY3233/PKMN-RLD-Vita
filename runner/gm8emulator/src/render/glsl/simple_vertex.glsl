// Shader de vértices simplificado (port PS Vita): mismo resultado que vertex.glsl para juegos 2D
// sin iluminación 3D. Escrito en GLSL 1.00 (attribute/varying) para que lo acepte el traductor
// GLSL→CG de vitaGL; en escritorio se antepone un preámbulo que lo convierte a GLSL 3.30.
uniform mat4 model;
uniform mat4 viewproj;
uniform float colour_blending;

attribute vec3 pos;
attribute vec4 blend;
attribute vec2 tex_coord;
attribute vec4 atlas_xywh;

varying vec2 frag_tex_coord;
varying vec4 frag_atlas_xywh;
varying vec4 frag_blend;
varying float fog_z;

void main() {
    vec4 world_pos = model * vec4(pos, 1.0);
    frag_atlas_xywh = atlas_xywh;
    // primitivas sin textura: coordenada centinela (antes NaN, que la GPU de la Vita no garantiza)
    if (tex_coord.x < -50000.0) {
        frag_tex_coord = vec2(0.0, 0.0);
        frag_blend = blend;
    } else {
        frag_tex_coord = tex_coord;
        frag_blend = colour_blending > 0.5 ? blend : vec4(1.0, 1.0, 1.0, 1.0);
    }
    gl_Position = viewproj * world_pos;
    fog_z = gl_Position.z;
}
