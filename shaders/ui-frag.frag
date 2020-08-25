#version 460

layout(set = 0, binding = 0) uniform texture2D text_texture;
layout(set = 0, binding = 1) uniform sampler image_sampler;
layout(set = 1, binding = 0) uniform texture2D image_texture;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) flat in uint v_mode;

layout(location = 0) out vec4 Target0;

void main() {
    // Text
    if (v_mode == uint(0)) {
        float a = texture(sampler2D(text_texture, image_sampler), v_uv).r;
        Target0 = v_color * vec4(1.0, 1.0, 1.0, a);

    // Image
    } else if (v_mode == uint(1)) {
        Target0 = texture(sampler2D(image_texture, image_sampler), v_uv);

    // 2D Geometry
    } else if (v_mode == uint(2)) {
        Target0 = v_color;
    }
}
