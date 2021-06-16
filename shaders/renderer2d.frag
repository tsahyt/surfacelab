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
    uint has_ao;
    uint has_alpha;
    uint has_view;
    uint view_type;
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
layout(set = 0, binding = 7) uniform texture2D t_Metallic;
layout(set = 0, binding = 8) uniform texture2D t_AO;
layout(set = 0, binding = 9) uniform texture2D t_Alpha;
layout(set = 0, binding = 10) uniform texture2D t_View;
layout(set = 0, binding = 11) uniform textureCube irradiance_map;
layout(set = 0, binding = 12) uniform textureCube environment_map;
layout(set = 0, binding = 13) uniform texture2D brdf_lut;
layout(set = 0, binding = 14) uniform texture2D matcap;

const uint CHANNEL_DISPLACEMENT = 0;
const uint CHANNEL_ALBEDO = 1;
const uint CHANNEL_NORMAL = 2;
const uint CHANNEL_ROUGHNESS = 3;
const uint CHANNEL_METALLIC = 4;
const uint CHANNEL_ALPHA = 5;
const uint CHANNEL_AMBIENTOCCLUSION = 6;

const uint VIEWTYPE_GRAYSCALE = 0;
const uint VIEWTYPE_RGB = 1;

const float TEX_SCALE = 1.0;
const float TEX_GRID = 0.01;

void main() {
    vec2 uv = v_TexCoord * resolution / resolution.y;
    uv = zoom * uv - pan;
    uv.y *= - 1.0;

    vec3 col;

    // Default to showing the view socket if available
    if (has_view != 0) {
        switch (view_type) {
            case 0:
                col = vec3(pow(texture(sampler2D(t_View, s_Texture), uv).r, 2.2));
                break;
            case 1:
                col = vec3(texture(sampler2D(t_View, s_Texture), uv).rgb);
                break;
        }
    } // Otherwise show the selected channel
    else if (channel == CHANNEL_DISPLACEMENT && has_displacement != 0) {
        col = vec3(pow(texture(sampler2D(t_Displ, s_Texture), uv).r, 2.2));
    } else if (channel == CHANNEL_ALBEDO && has_albedo != 0) {
        col = texture(sampler2D(t_Albedo, s_Texture), uv).rgb;
    } else if (channel == CHANNEL_NORMAL && has_normal != 0) {
        col = texture(sampler2D(t_Normal, s_Texture), uv).rgb;
    } else if (channel == CHANNEL_ROUGHNESS && has_roughness != 0) {
        col = vec3(pow(texture(sampler2D(t_Roughness, s_Texture), uv).r, 2.2));
    } else if (channel == CHANNEL_METALLIC && has_metallic != 0) {
        col = vec3(pow(texture(sampler2D(t_Metallic, s_Texture), uv).r, 2.2));
    } else if (channel == CHANNEL_ALPHA && has_alpha != 0) {
        col = vec3(pow(texture(sampler2D(t_Alpha, s_Texture), uv).r, 2.2));
    } else if (channel == CHANNEL_AMBIENTOCCLUSION && has_ao != 0) {
        col = vec3(pow(texture(sampler2D(t_AO, s_Texture), uv).r, 2.2));
    } else {
        col = vec3(0.,0.,0.);
    }

    if (fract(uv.x) < TEX_GRID || fract(uv.y) < TEX_GRID) {
        col += vec3(0.3, 0.8, 0.);
    }

    outColor = vec4(col, 1.0);
}
