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
    vec2 pan;
    float zoom;
};
layout(set = 0, binding = 3) uniform texture2D t_Displ;
layout(set = 0, binding = 4) uniform texture2D t_Albedo;
layout(set = 0, binding = 5) uniform texture2D t_Normal;
layout(set = 0, binding = 6) uniform texture2D t_Roughness;


void main() {
    vec3 uv = vec3(v_TexCoord, 0.);
    vec4 tex = texture(sampler2D(t_Displ, s_Texture), v_TexCoord);

    outColor = vec4(tex.r, tex.r, tex.r, 1.0);
}
