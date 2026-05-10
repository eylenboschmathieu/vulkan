#version 450

layout(set = 1, binding = 0) uniform sampler2DArray texArray;

layout(location = 0) in vec2 fragTexCoord;
layout(location = 1) flat in uint fragLayer;
layout(location = 2) in float fragBrightness;

layout(location = 0) out vec4 outColor;

void main() {
    vec4 color = texture(texArray, vec3(fragTexCoord, float(fragLayer)));
    outColor = vec4(color.rgb * fragBrightness, color.a);
}
