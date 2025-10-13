// HLSL Pixel Shader for GPU-accelerated HSV color correction
// Converts RGB -> HSV -> Apply hue mapping -> Convert back to RGB

Texture2D screenTexture : register(t0);
SamplerState samplerState : register(s0);

cbuffer ColorParams : register(b0)
{
    float strength;      // Color correction strength (0.0-1.0)
    float padding[3];    // Alignment
};

struct PS_INPUT
{
    float4 pos : SV_POSITION;
    float2 tex : TEXCOORD0;
};

// RGB to HSV conversion
float3 rgb2hsv(float3 rgb)
{
    float4 K = float4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    float4 p = lerp(float4(rgb.bg, K.wz), float4(rgb.gb, K.xy), step(rgb.b, rgb.g));
    float4 q = lerp(float4(p.xyw, rgb.r), float4(rgb.r, p.yzx), step(p.x, rgb.r));

    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    return float3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

// HSV to RGB conversion
float3 hsv2rgb(float3 hsv)
{
    float4 K = float4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    float3 p = abs(frac(hsv.xxx + K.xyz) * 6.0 - K.www);
    return hsv.z * lerp(K.xxx, clamp(p - K.xxx, 0.0, 1.0), hsv.y);
}

// Hue mapping function - this is where the color blind correction happens
// TODO: Replace with actual spectrum lookup from texture
float mapHue(float hue)
{
    // Placeholder: simple hue shift for testing
    // In production, this should sample from the spectrum pair texture
    float mappedHue = hue + 0.1 * strength;
    return frac(mappedHue); // Keep in 0-1 range
}

float4 main(PS_INPUT input) : SV_TARGET
{
    // Sample the captured screen
    float4 color = screenTexture.Sample(samplerState, input.tex);

    // Convert to HSV
    float3 hsv = rgb2hsv(color.rgb);

    // Apply hue mapping with strength
    float originalHue = hsv.x;
    float mappedHue = mapHue(originalHue);
    hsv.x = lerp(originalHue, mappedHue, strength);

    // Convert back to RGB
    float3 correctedRgb = hsv2rgb(hsv);

    return float4(correctedRgb, color.a);
}
