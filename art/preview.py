"""A preview of the game's surfaces in Blender (docs/art-style.md §6, ADR-0026).

The renderer's mesh shader is the authority on how a surface looks; this node tree follows its
layers closely enough to judge a model while it is made, live in Blender:

1. the token's colour, shifted by the part's tone (blue of "Color");
2. the token's pattern (wood grain along the part, brushed metal, paint, stone, cloth, leather);
3. mottling and fine grain over everything;
4. age (the object's "age" property: the years it has weathered, as the game's instance carries
   them): patches turning into the token it ages to at its own rate (timber greys in a year,
   steel rusts), dirt gathering over two years, edges chipping;
5. grime: the ground's splash (alpha) and dirt in creases (from the occlusion, red);
6. edge wear: chips on worn edges (green) revealing the token it wears to;
7. the baked occlusion (only a preview of what the renderer does with ambient light).
"""

import bake
import bpy

# Grime's colour: dark earthy dirt (linear); it gathers over this many years outdoors.
GRIME = (0.022, 0.018, 0.013)
GRIME_YEARS = 2.0
# Wood: grain lines per metre across the grain, and how far (in lines) each wanders.
GRAIN_LINES = 45.0
GRAIN_WANDER = 6.0
# Hair: strands per metre across the way it lies, and how far (in strands) each wanders.
HAIR_LINES = 600.0
HAIR_WANDER = 2.5


class _Tree:
    """Small helpers over a material's node tree."""

    def __init__(self, mat):
        self.nodes, self.links = mat.node_tree.nodes, mat.node_tree.links

    def node(self, kind, **settings):
        n = self.nodes.new(kind)
        for key, value in settings.items():
            setattr(n, key, value)
        return n

    def link(self, out, inp):
        self.links.new(out, inp)

    def value(self, x):
        n = self.node("ShaderNodeValue")
        n.outputs[0].default_value = x
        return n.outputs[0]

    def math(self, op, a, b=None, clamp=False):
        n = self.node("ShaderNodeMath", operation=op, use_clamp=clamp)
        for k, x in enumerate((a, b)):
            if x is None:
                continue
            if isinstance(x, (int, float)):
                n.inputs[k].default_value = x
            else:
                self.link(x, n.inputs[k])
        return n.outputs[0]

    def mix(self, a, b, factor):
        n = self.node("ShaderNodeMix", data_type="RGBA")
        for socket, x in (("A", a), ("B", b), ("Factor", factor)):
            if isinstance(x, tuple):
                n.inputs[socket].default_value = (*x, 1.0)
            elif isinstance(x, (int, float)):
                n.inputs[socket].default_value = x
            else:
                self.link(x, n.inputs[socket])
        return n.outputs["Result"]

    def scale(self, colour, factor):
        n = self.node("ShaderNodeMix", data_type="RGBA", blend_type="MULTIPLY")
        n.inputs["Factor"].default_value = 1.0
        self.link(colour, n.inputs["A"])
        c = self.node("ShaderNodeCombineColor")
        for k in range(3):
            self.link(factor, c.inputs[k])
        self.link(c.outputs[0], n.inputs["B"])
        return n.outputs["Result"]

    def noise(self, vector, scale, detail=4.0, roughness=0.55):
        n = self.node("ShaderNodeTexNoise")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = roughness
        self.link(vector, n.inputs["Vector"])
        return n.outputs["Fac"]

    def stretch(self, vector, sx, sy, sz=1.0):
        n = self.node("ShaderNodeMapping")
        n.inputs["Scale"].default_value = (sx, sy, sz)
        self.link(vector, n.inputs["Vector"])
        return n.outputs["Vector"]

    def smoothstep(self, x, lo, hi):
        n = self.node("ShaderNodeMapRange", interpolation_type="SMOOTHSTEP")
        n.inputs["From Min"].default_value = lo
        n.inputs["From Max"].default_value = hi
        self.link(x, n.inputs["Value"])
        return n.outputs["Result"]


def _pattern(t, kind, uv, obj):
    """The pattern's value around 1 (a brightness factor), or None for plain surfaces."""
    if kind == "Wood":
        # Grain: fine streaks along the part, and wavy growth lines, darkest in their troughs.
        # Soft streaks (about 1:20 along the part), and thin dark grain lines a few centimetres
        # apart, wandering.
        streaks = t.noise(t.stretch(uv, 8.0, 60.0), 1.0, 5.0, 0.6)
        # Lines: the crests of sin(2π·(GRAIN_LINES·v + GRAIN_WANDER·noise)), the noise slow along
        # the part, so each line wanders a little as it runs.
        across = t.node("ShaderNodeSeparateXYZ")
        t.link(uv, across.inputs["Vector"])
        wander = t.math("MULTIPLY", t.noise(t.stretch(uv, 0.5, 12.0), 1.0, 3.0), GRAIN_WANDER)
        phase = t.math("ADD", t.math("MULTIPLY", across.outputs["Y"], GRAIN_LINES), wander)
        crest = t.math("SINE", t.math("MULTIPLY", phase, 6.283185))
        lines = t.math("MULTIPLY", t.smoothstep(crest, 0.75, 1.0), t.math("MULTIPLY", t.noise(t.stretch(uv, 1.0, 20.0), 1.0, 2.0), 1.6))
        soft = t.math("ADD", t.math("MULTIPLY", streaks, 0.16), 0.92)
        return t.math("SUBTRACT", soft, t.math("MULTIPLY", lines, 0.28))
    if kind == "Metal":
        brushed = t.noise(t.stretch(uv, 4.0, 300.0), 1.0, 3.0)
        return t.math("ADD", t.math("MULTIPLY", brushed, 0.14), 0.93)
    if kind == "Paint":
        peel = t.noise(obj, 260.0, 2.0)
        return t.math("ADD", t.math("MULTIPLY", peel, 0.08), 0.96)
    if kind == "Stone":
        coarse = t.noise(obj, 40.0, 4.0)
        fine = t.noise(obj, 220.0, 2.0)
        return t.math("ADD", t.math("MULTIPLY", t.math("ADD", coarse, fine), 0.12), 0.88)
    if kind == "Fabric":
        weave = t.noise(t.stretch(uv, 400.0, 400.0), 1.0, 1.0)
        return t.math("ADD", t.math("MULTIPLY", weave, 0.12), 0.94)
    if kind == "Hair":
        # Strands along the grain, each wandering; darker and lighter hairs in long streaks.
        across = t.node("ShaderNodeSeparateXYZ")
        t.link(uv, across.inputs["Vector"])
        wander = t.math("MULTIPLY", t.noise(t.stretch(uv, 3.0, 60.0), 1.0, 3.0), HAIR_WANDER)
        phase = t.math("ADD", t.math("MULTIPLY", across.outputs["Y"], HAIR_LINES), wander)
        strand = t.math("ADD", t.math("MULTIPLY", t.math("SINE", t.math("MULTIPLY", phase, 6.283185)), 0.5), 0.5)
        shade = t.math("ADD", t.noise(t.stretch(uv, 6.0, 90.0), 1.0, 3.0), 0.5)
        return t.math("ADD", t.math("MULTIPLY", t.math("MULTIPLY", strand, shade), 0.4), 0.72)
    if kind == "Leather":
        cells = t.node("ShaderNodeTexVoronoi")
        cells.inputs["Scale"].default_value = 180.0
        t.link(obj, cells.inputs["Vector"])
        return t.math("ADD", t.math("MULTIPLY", cells.outputs["Distance"], 0.2), 0.9)
    return None


def surface(mat, info, wears, ages):
    """Fills `mat`'s node tree with the preview of a token: `info` its (rgb, roughness, metalness,
    pattern, years to weather or None); `wears` the (rgb, roughness, metalness) of the token it
    wears to and `ages` the linear colour it ages to (each None without one)."""
    rgb, rough, metal, kind, weathers_in = info
    t = _Tree(mat)
    bsdf = next(n for n in t.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Metallic"].default_value = metal
    baked = t.node("ShaderNodeVertexColor", layer_name="Color")
    split = t.node("ShaderNodeSeparateColor")
    t.link(baked.outputs["Color"], split.inputs["Color"])
    ao, wear, tone = split.outputs["Red"], split.outputs["Green"], split.outputs["Blue"]
    uv = t.node("ShaderNodeUVMap", uv_map="UVMap").outputs["UV"]
    coords = t.node("ShaderNodeTexCoord")
    obj = coords.outputs["Object"]
    # The ground's splash: up the sides to bake.SPLASH above the base, none on what faces up,
    # all over what faces down.
    facing = t.node("ShaderNodeSeparateXYZ")
    t.link(coords.outputs["Normal"], facing.inputs["Vector"])
    height = t.math("MULTIPLY", baked.outputs["Alpha"], bake.HEIGHT_RANGE)
    splash = t.math("POWER", t.math("SUBTRACT", 1.0, t.smoothstep(height, 0.0, bake.SPLASH)), 1.5)
    splash = t.math("MULTIPLY", splash, t.math("SUBTRACT", 1.0, t.math("MAXIMUM", facing.outputs["Z"], 0.0)))
    down = t.smoothstep(t.math("MULTIPLY", facing.outputs["Z"], -1.0), 0.4, 0.6)
    grime_splash = t.math("MAXIMUM", splash, down)

    # 1-3: colour, the part's tone (±18 %), pattern, mottling (±15 %) and fine grain (±10 %).
    colour = t.scale(t.mix(rgb, rgb, 0.0), t.math("ADD", t.math("MULTIPLY", tone, 0.36), 0.82))
    pattern = _pattern(t, kind, uv, obj)
    if pattern is not None:
        colour = t.scale(colour, pattern)
    mottle = t.noise(obj, 3.5, 3.0)
    colour = t.scale(colour, t.math("ADD", t.math("MULTIPLY", mottle, 0.30), 0.85))
    speck = t.noise(obj, 420.0, 1.0)
    colour = t.scale(colour, t.math("ADD", t.math("MULTIPLY", speck, 0.20), 0.90))

    # The years the object has weathered: its "age" property here, its instance's in the game.
    # Dirt gathers over GRIME_YEARS; each part is a little older or younger (its tone, ±0.3).
    years = t.node("ShaderNodeAttribute", attribute_type="OBJECT", attribute_name="age").outputs["Fac"]
    object_age = t.math("DIVIDE", years, GRIME_YEARS, clamp=True)
    shift = t.math("MULTIPLY", t.math("SUBTRACT", tone, 0.5), 0.6)
    film_age = t.math("ADD", object_age, shift, clamp=True)

    # 5 (first, ageing follows the dirt): grime from the splash and the creases, gathering as
    # the object ages (a new one is clean).
    crease = t.math("SUBTRACT", 1.0, ao)
    grime = t.math("MAXIMUM", grime_splash, crease)
    grime = t.math("MULTIPLY", grime, t.math("ADD", t.noise(obj, 9.0, 4.0), 0.5))
    grime = t.math("MULTIPLY", grime, t.math("ADD", t.math("MULTIPLY", object_age, 0.9), 0.1), clamp=True)

    # 4: age, at the token's own rate (its years to weather). Wood greys all over and more in
    # patches; metal rusts and copper greens in patches only, where dirt holds water.
    if ages is not None and weathers_in:
        age = t.math("ADD", t.math("DIVIDE", years, weathers_in), shift, clamp=True)
        patches = t.math("ADD", t.math("MULTIPLY", t.noise(obj, 2.5, 5.0), 0.7),
                         t.math("MULTIPLY", grime, 0.5))
        threshold = t.math("SUBTRACT", 1.25, age)
        aged = t.smoothstep(t.math("SUBTRACT", patches, threshold), -0.08, 0.08)
        aged_colour = t.scale(t.mix(ages, ages, 0.0), pattern) if pattern is not None else t.mix(ages, ages, 0.0)
        if kind == "Wood":
            amount = t.math("MAXIMUM", t.math("MULTIPLY", age, 0.95), aged)
        else:
            amount = t.math("MULTIPLY", aged, 0.9)
        colour = t.mix(colour, aged_colour, amount)
    colour = t.mix(colour, GRIME, t.math("MULTIPLY", grime, 0.9))
    # A film of dirt over everything old, and streaks down the sides where rain ran.
    colour = t.scale(colour, t.math("SUBTRACT", 1.0, t.math("MULTIPLY", film_age, 0.3)))
    streak = t.smoothstep(t.noise(t.stretch(obj, 50.0, 50.0, 1.5), 1.0, 3.0), 0.5, 0.72)
    sides = t.math("SUBTRACT", 1.0, t.math("ABSOLUTE", facing.outputs["Z"]))
    streaks = t.math("MULTIPLY", t.math("MULTIPLY", streak, sides), t.math("MULTIPLY", film_age, 0.3))
    colour = t.scale(colour, t.math("SUBTRACT", 1.0, streaks))

    # 6: wear on edges, in patches a few centimetres long (never a continuous line, no
    # outlines, art-style.md §5), more as the object ages. It reveals the token beneath with
    # its own finish; what is revealed weathers and dirties too, lagging the surface: fresh on
    # a new object, mostly its surface on an old one.
    chips = t.smoothstep(t.math("MULTIPLY", wear, t.noise(obj, 9.0, 3.0)), 0.58, 0.72)
    chips = t.math("MULTIPLY", chips, t.math("ADD", t.math("MULTIPLY", object_age, 1.2), 0.1), clamp=True)
    fresh = t.math("MULTIPLY", chips, t.math("SUBTRACT", 0.7, t.math("MULTIPLY", film_age, 0.55)))
    roughness = t.math("ADD", rough, t.math("MULTIPLY", grime, 0.15))
    metallic = metal
    if wears is not None:
        wears_rgb, wears_rough, wears_metal = wears
        revealed = t.mix(wears_rgb, wears_rgb, 0.0)
        if pattern is not None:
            revealed = t.scale(revealed, pattern)
        colour = t.mix(colour, revealed, fresh)
        lerp = lambda a, b: t.math("ADD", a, t.math("MULTIPLY", t.math("SUBTRACT", b, a), fresh))  # noqa: E731
        roughness = lerp(roughness, wears_rough)
        metallic = lerp(metal, wears_metal)
        t.link(metallic, bsdf.inputs["Metallic"])

    # 7: the baked occlusion, previewed on the colour.
    colour = t.scale(colour, t.math("POWER", ao, 0.8))
    t.link(colour, bsdf.inputs["Base Color"])
    t.link(t.math("ADD", roughness, 0.0, clamp=True), bsdf.inputs["Roughness"])
    mat.diffuse_color = (*rgb, 1.0)
