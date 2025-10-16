import numpy as np
from PIL import Image
import json
import os
from hlsl_transpiler import HLSLTranspiler

shader_path = os.path.join(os.path.dirname(__file__), '..', 'src', 'shaders.hlsl')
shader_path = os.path.abspath(shader_path)

print(f"Loading shader from: {shader_path}")
transpiler = HLSLTranspiler(shader_path)

transpiled_code = transpiler.generate_all_functions()
exec(transpiled_code, globals())
print("Successfully transpiled shader functions to Python")

def hex_to_rgb(hex_color):
    hex_color = hex_color.lstrip('#')
    return tuple(int(hex_color[i:i+2], 16) / 255.0 for i in (0, 2, 4))

def create_spectrum_texture(nodes, width=360):
    spectrum = np.zeros((1, width, 3))

    for i in range(width):
        t = i / (width - 1)

        node1, node2 = None, None
        for j in range(len(nodes) - 1):
            if nodes[j]['position'] <= t <= nodes[j + 1]['position']:
                node1, node2 = nodes[j], nodes[j + 1]
                break

        if node1 and node2:
            segment_t = (t - node1['position']) / (node2['position'] - node1['position'])
            color1 = np.array(hex_to_rgb(node1['color']))
            color2 = np.array(hex_to_rgb(node2['color']))
            spectrum[0, i] = color1 * (1 - segment_t) + color2 * segment_t

    return spectrum

# Note: rgb_to_hsv, hsv_to_rgb, lookup_spectrum_hsv, and apply_spectrum are dynamically generated from src/shaders.hlsl via the transpiler

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    spectrums_dir = os.path.join(script_dir, '..', 'assets', 'spectrums')
    output_dir = os.path.join(script_dir, 'output')

    # Ensure output directory exists
    os.makedirs(output_dir, exist_ok=True)

    # Find all JSON files in the spectrums directory
    json_files = [f for f in os.listdir(spectrums_dir) if f.endswith('.json')]

    if not json_files:
        print(f"No JSON files found in {spectrums_dir}")
        return

    input_path = os.path.join(script_dir, 'input', 'normal_spectrum.png')
    img = Image.open(input_path)
    img_rgb = np.array(img).astype(np.float32) / 255.0

    # Process each JSON file
    for json_file in json_files:
        spectrum_json = os.path.join(spectrums_dir, json_file)
        json_basename = os.path.splitext(json_file)[0]

        print(f"\nProcessing {json_file}...")

        with open(spectrum_json, 'r') as f:
            data = json.load(f)

        spectra = data['spectra']

        for i, spectrum_data in enumerate(spectra, start=1):
            spectrum_texture = create_spectrum_texture(spectrum_data['nodes'])

            output = apply_spectrum(img_rgb, spectrum_texture, strength=1.0)
            output = (np.clip(output, 0, 1) * 255).astype(np.uint8)

            output_filename = os.path.join(output_dir, f'{json_basename}_{i}.png')
            Image.fromarray(output).save(output_filename)
            print(f"  Saved {output_filename}")

if __name__ == '__main__':
    main()
