#version 450

layout(set = 0, binding = 0) uniform UniformBufferObject {
    mat4 model;
    mat4 view;
    mat4 proj;
    vec4 sun_dir;
} ubo;

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inTexCoord;
layout(location = 2) in uint inLayer;
layout(location = 3) in vec3 inNormal;

layout(location = 0) out vec2 fragTexCoord;
layout(location = 1) flat out uint fragLayer;
layout(location = 2) out float fragBrightness;

void main() {
    gl_Position = ubo.proj * ubo.view * vec4(inPosition, 1.0);
    fragTexCoord = inTexCoord;
    fragLayer = inLayer;
    float t = max(ubo.sun_dir.z, 0.0);
    float ambient = mix(0.02, 0.3, t);
    float diffuse = max(dot(inNormal, ubo.sun_dir.xyz), 0.0);
    fragBrightness = ambient + (1.0 - ambient) * diffuse * t;
}
