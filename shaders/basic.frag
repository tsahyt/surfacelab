#version 450

layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform texture2D t_Color;
layout(set = 0, binding = 1) uniform sampler s_Color;

void main() {
    vec3 uv = vec3(v_TexCoord, 0.);
    vec4 tex = texture(sampler2D(t_Color, s_Color), v_TexCoord);

    outColor = vec4(tex.r, tex.r, tex.r, 1.0);
}
