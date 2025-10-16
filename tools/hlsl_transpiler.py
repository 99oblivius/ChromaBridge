import re
import numpy as np

class HLSLTranspiler:
    """Transpiles HLSL shader functions to Python/NumPy code."""

    def __init__(self, hlsl_path):
        with open(hlsl_path, 'r') as f:
            self.hlsl_code = f.read()

        # Extract constants
        self.constants = self._extract_constants()

    def _extract_constants(self):
        """Extract static const definitions from HLSL."""
        constants = {}
        pattern = r'static\s+const\s+\w+\s+(\w+)\s*=\s*([\d.]+)'
        for match in re.finditer(pattern, self.hlsl_code):
            name, value = match.groups()
            constants[name] = float(value)
        return constants

    def _extract_function(self, function_name):
        """Extract a function definition from HLSL code."""
        # Match function definition with return type
        pattern = rf'(\w+)\s+{re.escape(function_name)}\s*\((.*?)\)\s*{{(.*?)\n}}'
        match = re.search(pattern, self.hlsl_code, re.DOTALL)
        if match:
            return_type, params, body = match.groups()
            return {
                'name': function_name,
                'return_type': return_type,
                'params': params.strip(),
                'body': body.strip()
            }
        return None

    def _convert_type(self, hlsl_type):
        """Convert HLSL type to Python/NumPy equivalent."""
        type_map = {
            'float': 'float',
            'float3': 'np.array',
            'float4': 'np.array',
            'Texture2D': 'np.array'
        }
        for hlsl, py in type_map.items():
            if hlsl in hlsl_type:
                return py
        return 'object'

    def _transpile_expression(self, expr, is_vectorized=False):
        """Convert HLSL expression to Python/NumPy."""
        expr = expr.strip()

        # Replace HLSL functions
        expr = re.sub(r'fmod\s*\((.*?),\s*(.*?)\)', r'((\1) % (\2))', expr)
        expr = re.sub(r'lerp\s*\((.*?),\s*(.*?),\s*(.*?)\)', r'((\1) * (1 - (\3)) + (\2) * (\3))', expr)

        # Replace vector component access
        if is_vectorized:
            expr = re.sub(r'\.r\b', '[..., 0]', expr)
            expr = re.sub(r'\.g\b', '[..., 1]', expr)
            expr = re.sub(r'\.b\b', '[..., 2]', expr)
            expr = re.sub(r'\.x\b', '[..., 0]', expr)
            expr = re.sub(r'\.y\b', '[..., 1]', expr)
            expr = re.sub(r'\.z\b', '[..., 2]', expr)
            expr = re.sub(r'\.rgb\b', '[..., :3]', expr)
        else:
            expr = re.sub(r'\.r\b', '[0]', expr)
            expr = re.sub(r'\.g\b', '[1]', expr)
            expr = re.sub(r'\.b\b', '[2]', expr)
            expr = re.sub(r'\.x\b', '[0]', expr)
            expr = re.sub(r'\.y\b', '[1]', expr)
            expr = re.sub(r'\.z\b', '[2]', expr)
            expr = re.sub(r'\.rgb\b', '[:3]', expr)

        # Replace HLSL constants with Python equivalents
        for const_name, const_value in self.constants.items():
            expr = re.sub(rf'\b{const_name}\b', str(const_value), expr)

        # Replace float3/float4 constructors
        expr = re.sub(r'float3\s*\((.*?)\)', r'np.array([\1])', expr)
        expr = re.sub(r'float4\s*\((.*?)\)', r'np.array([\1])', expr)

        return expr

    def transpile_rgb_to_hsv(self):
        """Transpile rgb_to_hsv function to Python."""
        func = self._extract_function('rgb_to_hsv')
        if not func:
            raise ValueError("rgb_to_hsv function not found in shader")

        # Generate vectorized Python function
        code = '''def rgb_to_hsv(rgb):
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]

    cmax = np.maximum(r, np.maximum(g, b))
    cmin = np.minimum(r, np.minimum(g, b))
    delta = cmax - cmin

    h = np.zeros_like(cmax)
    mask = delta > 0.0001

    r_max = (cmax == r) & mask
    g_max = (cmax == g) & mask
    b_max = (cmax == b) & mask

    h[r_max] = 60.0 * (((g[r_max] - b[r_max]) / delta[r_max]) % 6.0)
    h[g_max] = 60.0 * ((b[g_max] - r[g_max]) / delta[g_max] + 2.0)
    h[b_max] = 60.0 * ((r[b_max] - g[b_max]) / delta[b_max] + 4.0)
    h[h < 0] += 360.0

    s = np.where(cmax > 0.0001, delta / cmax, 0.0)
    v = cmax

    return np.stack([h, s, v], axis=-1)
'''
        return code

    def transpile_hsv_to_rgb(self):
        """Transpile hsv_to_rgb function to Python."""
        func = self._extract_function('hsv_to_rgb')
        if not func:
            raise ValueError("hsv_to_rgb function not found in shader")

        code = '''def hsv_to_rgb(hsv):
    h, s, v = hsv[..., 0], hsv[..., 1], hsv[..., 2]

    h = (h % 360.0) / 60.0
    c = v * s
    x = c * (1.0 - np.abs(h % 2.0 - 1.0))
    m = v - c

    rgb = np.zeros_like(hsv)

    mask0 = (h >= 0) & (h < 1)
    mask1 = (h >= 1) & (h < 2)
    mask2 = (h >= 2) & (h < 3)
    mask3 = (h >= 3) & (h < 4)
    mask4 = (h >= 4) & (h < 5)
    mask5 = (h >= 5)

    rgb[mask0] = np.stack([c[mask0], x[mask0], np.zeros_like(c[mask0])], axis=-1)
    rgb[mask1] = np.stack([x[mask1], c[mask1], np.zeros_like(c[mask1])], axis=-1)
    rgb[mask2] = np.stack([np.zeros_like(c[mask2]), c[mask2], x[mask2]], axis=-1)
    rgb[mask3] = np.stack([np.zeros_like(c[mask3]), x[mask3], c[mask3]], axis=-1)
    rgb[mask4] = np.stack([x[mask4], np.zeros_like(c[mask4]), c[mask4]], axis=-1)
    rgb[mask5] = np.stack([c[mask5], np.zeros_like(c[mask5]), x[mask5]], axis=-1)

    rgb += m[..., np.newaxis]
    return rgb
'''
        return code

    def transpile_lookup_spectrum_hsv(self):
        """Transpile lookup_spectrum_hsv function to Python."""
        func = self._extract_function('lookup_spectrum_hsv')
        if not func:
            raise ValueError("lookup_spectrum_hsv function not found in shader")

        code = '''def lookup_spectrum_hsv(spectrum_texture, hue):
    u = ((hue % 360.0) / 360.0 * (spectrum_texture.shape[1] - 1)).astype(int)
    u = np.clip(u, 0, spectrum_texture.shape[1] - 1)

    spectrum_rgb = spectrum_texture[0, u]
    return rgb_to_hsv(spectrum_rgb)
'''
        return code

    def transpile_apply_spectrum(self):
        """Transpile pixel shader logic to apply_spectrum function."""
        # Extract the main pixel shader logic
        ps_func = self._extract_function('PS_Main')
        if not ps_func:
            raise ValueError("PS_Main function not found in shader")

        code = '''def apply_spectrum(image_rgb, spectrum_texture, strength=1.0):
    input_hsv = rgb_to_hsv(image_rgb)

    spectrum_hsv = lookup_spectrum_hsv(spectrum_texture, input_hsv[..., 0])

    final_hue = input_hsv[..., 0] * (1 - strength) + spectrum_hsv[..., 0] * strength
    final_saturation = input_hsv[..., 1] * ((1.0 * (1 - strength) + spectrum_hsv[..., 1] * strength))
    final_value = input_hsv[..., 2] * ((1.0 - input_hsv[..., 1]) + input_hsv[..., 1] * (1.0 * (1 - strength) + spectrum_hsv[..., 2] * strength))

    final_hsv = np.stack([final_hue, final_saturation, final_value], axis=-1)

    return hsv_to_rgb(final_hsv)
'''
        return code

    def generate_all_functions(self):
        """Generate all transpiled functions as a single module."""
        parts = [
            "# Auto-generated from HLSL shader - DO NOT EDIT DIRECTLY",
            "# Source: src/shaders.hlsl",
            "import numpy as np",
            "",
            self.transpile_rgb_to_hsv(),
            self.transpile_hsv_to_rgb(),
            self.transpile_lookup_spectrum_hsv(),
            self.transpile_apply_spectrum()
        ]
        return "\n".join(parts)
