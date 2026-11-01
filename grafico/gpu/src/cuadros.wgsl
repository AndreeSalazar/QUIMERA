// Cuadros de QUIMERA: rectángulos de color y letras del atlas, en una sola pasada.
// Las posiciones llegan en píxeles del documento; el desplazamiento (scroll) va en el uniforme.

struct Uniforme {
    tam: vec2<f32>,   // tamaño de la superficie en píxeles
    desp: vec2<f32>,  // desplazamiento del documento
};

@group(0) @binding(0) var<uniform> u: Uniforme;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var muestreo: sampler;

struct Entrada {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) modo: f32,       // 0: color liso; 1: letra (cobertura del atlas); +2: fijo (no se desplaza)
};

struct Salida {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) modo: f32,
};

@vertex
fn vs(e: Entrada) -> Salida {
    // Lo fijo (la barra del navegador) no se mueve al desplazar la página.
    let fijo = e.modo > 1.5;
    let p = e.pos - select(u.desp, vec2<f32>(0.0, 0.0), fijo);
    var s: Salida;
    s.pos = vec4<f32>(p.x / u.tam.x * 2.0 - 1.0, 1.0 - p.y / u.tam.y * 2.0, 0.0, 1.0);
    s.uv = e.uv;
    s.color = e.color;
    s.modo = select(e.modo, e.modo - 2.0, fijo);
    return s;
}

@fragment
fn fs(s: Salida) -> @location(0) vec4<f32> {
    // Se muestrea siempre (WGSL lo exige fuera de ramas que dependan de cada píxel).
    let cobertura = textureSample(atlas, muestreo, s.uv).r;
    let a = select(s.color.a, s.color.a * cobertura, s.modo > 0.5);
    return vec4<f32>(s.color.rgb * a, a); // alfa premultiplicado
}
