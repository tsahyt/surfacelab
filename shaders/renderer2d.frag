#version 450

layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler s_Texture;
layout(set = 0, binding = 1) uniform Occupancy {
    uint has_albedo;
    uint has_roughness;
    uint has_normal;
    uint has_displacement;
    uint has_metallic;
};
layout(set = 0, binding = 2) uniform Camera {
    vec2 resolution;
    vec2 pan;
    float zoom;
    uint channel;
};
layout(set = 0, binding = 3) uniform texture2D t_Displ;
layout(set = 0, binding = 4) uniform texture2D t_Albedo;
layout(set = 0, binding = 5) uniform texture2D t_Normal;
layout(set = 0, binding = 6) uniform texture2D t_Roughness;

#define CHANNEL_DISPLACEMENT 0
#define CHANNEL_ALBEDO 1
#define CHANNEL_NORMAL 2
#define CHANNEL_ROUGHNESS 3

#define TEX_SCALE 1.0
#define TEX_GRID 0.01

void main() {
    vec2 uv = v_TexCoord * resolution / resolution.y;
    uv = zoom * uv - pan;

    vec3 col;
    if (channel == CHANNEL_DISPLACEMENT && has_displacement != 0) {
        col = vec3(pow(texture(sampler2D(t_Displ, s_Texture), uv).r, 2.2));
    } else if (channel == CHANNEL_ALBEDO && has_albedo != 0) {
        col = texture(sampler2D(t_Albedo, s_Texture), uv).rgb;
    } else if (channel == CHANNEL_NORMAL && has_normal != 0) {
        col = texture(sampler2D(t_Normal, s_Texture), uv).rgb;
    } else if (channel == CHANNEL_ROUGHNESS && has_roughness != 0) {
        col = vec3(pow(texture(sampler2D(t_Roughness, s_Texture), uv).r, 2.2));
    } else {
        col = vec3(0.,0.,0.);
    }

    if (fract(uv.x) < TEX_GRID || fract(uv.y) < TEX_GRID) {
        col += vec3(0.3, 0.8, 0.);
    }

    outColor = vec4(col, 1.0);
}
