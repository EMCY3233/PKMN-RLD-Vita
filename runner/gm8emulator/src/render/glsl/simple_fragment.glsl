// Shader de fragmentos simplificado (port PS Vita): ver simple_vertex.glsl.
uniform sampler2D tex;
uniform vec2 tex_size;
uniform float repeat;
uniform float interp; // no se llama lerp: en vitaGL mix() se convierte en esa función de CG
uniform float alpha_test;
uniform float fog_enabled;
uniform float fog_begin;
uniform float fog_end;
uniform vec4 fog_colour;

varying vec2 frag_tex_coord;
varying vec4 frag_atlas_xywh;
varying vec4 frag_blend;
varying float fog_z;

void main() {
    vec2 sprite_coord;
    vec4 tex_col;
    vec2 wh = frag_atlas_xywh.zw;
    if (repeat > 0.5) {
        sprite_coord = fract(frag_tex_coord) * wh;
        if (interp > 0.5) {
            vec2 floor_coord = floor(sprite_coord - 0.5);
            // el traductor de vitaGL no tiene módulo vec2 entre vec2: fórmula explícita
            vec2 a = floor_coord + 0.5;
            vec2 b = floor_coord + 1.5;
            vec2 topleft = (frag_atlas_xywh.xy + (a - wh * floor(a / wh))) / tex_size;
            vec2 botright = (frag_atlas_xywh.xy + (b - wh * floor(b / wh))) / tex_size;
            vec4 sampleTL = texture2D(tex, topleft);
            vec4 sampleTR = texture2D(tex, vec2(botright.x, topleft.y));
            vec4 sampleBL = texture2D(tex, vec2(topleft.x, botright.y));
            vec4 sampleBR = texture2D(tex, botright);
            vec2 factor = fract(sprite_coord + 0.5);
            tex_col = mix(mix(sampleTL, sampleTR, factor.x), mix(sampleBL, sampleBR, factor.x), factor.y);
        } else {
            sprite_coord = clamp(sprite_coord, vec2(0.5, 0.5), wh - 0.5);
            tex_col = texture2D(tex, (frag_atlas_xywh.xy + sprite_coord) / tex_size);
        }
    } else {
        sprite_coord = clamp(frag_tex_coord * wh, vec2(0.5, 0.5), wh - 0.5);
        tex_col = texture2D(tex, (frag_atlas_xywh.xy + sprite_coord) / tex_size);
    }
    vec4 colour = tex_col * frag_blend;
    if (fog_enabled > 0.5) {
        // GM8: niebla con inicio = fin (truco habitual para teñir un sprite de un color) = niebla total
        float range = fog_end - fog_begin;
        float f = range > 0.0 ? clamp((fog_end - fog_z) / range, 0.0, 1.0) : (fog_z < fog_begin ? 1.0 : 0.0);
        colour.rgb = mix(fog_colour.rgb, colour.rgb, f);
    }
    if (alpha_test > 0.5 && colour.a <= 0.0) {
        discard;
    }
    gl_FragColor = colour;
}
