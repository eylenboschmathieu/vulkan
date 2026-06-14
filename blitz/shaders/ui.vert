#version 450

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec2 inUv;
layout(location = 2) in vec4 inColor;

layout(push_constant) uniform PushConstants {
    mat4 proj;
    vec4 clip_rect; // left, top, right, bottom
} pc;

layout(location = 0) out vec2 fragUv;
layout(location = 1) out vec4 fragColor;

void main() {
    gl_Position = pc.proj * vec4(inPosition, 0.0, 1.0);
    fragUv = inUv;
    fragColor = inColor;
}
