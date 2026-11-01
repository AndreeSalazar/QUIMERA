//! GPU de QUIMERA con wgpu: una sola tubería que dibuja **cuadros** (rectángulos de color y
//! letras recortadas de un atlas) en una sola pasada.
//!
//! Es todo lo que necesita el pintado de una página por ahora. Los cuadros se cargan una vez
//! por maquetación ([`Gpu::cargar`]); desplazar la página solo cambia un uniforme
//! ([`Gpu::dibujar`]), sin volver a subir nada.
//!
//! No conoce la ventana: recibe cualquier cosa que wgpu acepte como superficie
//! (`Arc<winit::window::Window>`, por ejemplo).

use std::borrow::Cow;
use wgpu::util::DeviceExt;

/// Un rectángulo que dibujar, en píxeles de la superficie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cuadro {
    /// Izquierda.
    pub x: f32,
    /// Arriba.
    pub y: f32,
    /// Ancho.
    pub ancho: f32,
    /// Alto.
    pub alto: f32,
    /// Color RGBA de 0 a 1 (sRGB, sin premultiplicar).
    pub color: [f32; 4],
    /// Si es una letra: su rectángulo en el atlas (u0, v0, u1, v1, de 0 a 1).
    pub letra: Option<[f32; 4]>,
}

/// Un vértice tal como lo lee el sombreador.
#[repr(C)]
#[derive(Clone, Copy)]
struct Vertice {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    modo: f32,
}

const TAM_VERTICE: usize = 9 * 4;

impl Vertice {
    fn bytes(v: &[Vertice]) -> Vec<u8> {
        let mut b = Vec::with_capacity(v.len() * TAM_VERTICE);
        for x in v {
            for f in [x.pos[0], x.pos[1], x.uv[0], x.uv[1], x.color[0], x.color[1], x.color[2], x.color[3], x.modo] {
                b.extend_from_slice(&f.to_le_bytes());
            }
        }
        b
    }
}

/// Los seis vértices (dos triángulos) de un cuadro; los `fijos` no se desplazan.
fn vertices(c: &Cuadro, v: &mut Vec<Vertice>, fijo: bool) {
    let (x0, y0, x1, y1) = (c.x, c.y, c.x + c.ancho, c.y + c.alto);
    let ([u0, v0, u1, v1], modo) = match c.letra {
        Some(uv) => (uv, 1.0),
        None => ([0.0; 4], 0.0),
    };
    let modo = if fijo { modo + 2.0 } else { modo };
    let p = |x, y, u, w| Vertice { pos: [x, y], uv: [u, w], color: c.color, modo };
    v.extend_from_slice(&[
        p(x0, y0, u0, v0),
        p(x1, y0, u1, v0),
        p(x0, y1, u0, v1),
        p(x1, y0, u1, v0),
        p(x1, y1, u1, v1),
        p(x0, y1, u0, v1),
    ]);
}

/// La GPU lista para dibujar en una superficie.
pub struct Gpu {
    /// `None` sin ventana (para capturar imágenes).
    superficie: Option<wgpu::Surface<'static>>,
    dispositivo: wgpu::Device,
    cola: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    tuberia: wgpu::RenderPipeline,
    uniforme: wgpu::Buffer,
    grupo: wgpu::BindGroup,
    atlas: wgpu::Texture,
    tam_atlas: u32,
    vertices: Option<wgpu::Buffer>,
    cuantos: u32,
    /// Lo que no se desplaza (la barra del navegador), dibujado encima.
    fijos: Option<wgpu::Buffer>,
    cuantos_fijos: u32,
}

/// Un error al preparar o usar la GPU, como texto para quien lo muestre.
pub type Error = String;

impl Gpu {
    /// Prepara la GPU para dibujar en `superficie` de `ancho × alto` píxeles, con un atlas de
    /// letras de `tam_atlas × tam_atlas`.
    pub async fn nueva(
        superficie: impl Into<wgpu::SurfaceTarget<'static>>,
        ancho: u32,
        alto: u32,
        tam_atlas: u32,
    ) -> Result<Gpu, Error> {
        let instancia = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let superficie = instancia.create_surface(superficie).map_err(|e| format!("superficie: {e}"))?;
        let opciones = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&superficie),
            force_fallback_adapter: false,
        };
        let adaptador = instancia.request_adapter(&opciones).await.ok_or("no hay adaptador gráfico compatible")?;
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("quimera"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adaptador.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
        };
        let (dispositivo, cola) =
            adaptador.request_device(&descriptor, None).await.map_err(|e| format!("dispositivo: {e}"))?;
        let capacidades = superficie.get_capabilities(&adaptador);
        // Un formato sin sRGB: los colores de CSS ya son sRGB y se mezclan así, como en los navegadores.
        let formato = capacidades
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or(capacidades.formats.first().copied())
            .ok_or("la superficie no tiene formatos")?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: formato,
            width: ancho.max(1),
            height: alto.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: capacidades.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        superficie.configure(&dispositivo, &config);
        let (tuberia, uniforme, grupo, atlas) = Self::tuberia(&dispositivo, formato, tam_atlas);
        let superficie = Some(superficie);
        Ok(Gpu {
            superficie,
            dispositivo,
            cola,
            config,
            tuberia,
            uniforme,
            grupo,
            atlas,
            tam_atlas,
            vertices: None,
            cuantos: 0,
            fijos: None,
            cuantos_fijos: 0,
        })
    }

    /// Una GPU sin ventana que dibuja en una imagen de `ancho × alto` ([`Gpu::capturar`]).
    pub async fn sin_ventana(ancho: u32, alto: u32, tam_atlas: u32) -> Result<Gpu, Error> {
        let instancia = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let opciones = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        };
        let adaptador = instancia.request_adapter(&opciones).await.ok_or("no hay adaptador gráfico")?;
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("quimera sin ventana"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adaptador.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
        };
        let (dispositivo, cola) =
            adaptador.request_device(&descriptor, None).await.map_err(|e| format!("dispositivo: {e}"))?;
        let formato = wgpu::TextureFormat::Rgba8Unorm;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: formato,
            width: ancho.max(1),
            height: alto.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        let (tuberia, uniforme, grupo, atlas) = Self::tuberia(&dispositivo, formato, tam_atlas);
        Ok(Gpu {
            superficie: None,
            dispositivo,
            cola,
            config,
            tuberia,
            uniforme,
            grupo,
            atlas,
            tam_atlas,
            vertices: None,
            cuantos: 0,
            fijos: None,
            cuantos_fijos: 0,
        })
    }

    fn tuberia(
        d: &wgpu::Device,
        formato: wgpu::TextureFormat,
        tam_atlas: u32,
    ) -> (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup, wgpu::Texture) {
        let (uniforme, atlas, disposicion, grupo) = Self::recursos(d, tam_atlas);
        let sombreador = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cuadros"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("cuadros.wgsl"))),
        });
        let diseno = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cuadros"),
            bind_group_layouts: &[&disposicion],
            push_constant_ranges: &[],
        });
        let tuberia = Self::crear_tuberia(d, &sombreador, &diseno, formato);
        (tuberia, uniforme, grupo, atlas)
    }

    /// El uniforme, el atlas y cómo los ve el sombreador.
    fn recursos(
        d: &wgpu::Device,
        tam_atlas: u32,
    ) -> (wgpu::Buffer, wgpu::Texture, wgpu::BindGroupLayout, wgpu::BindGroup) {
        let uniforme = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforme"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas de letras"),
            size: wgpu::Extent3d { width: tam_atlas, height: tam_atlas, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let muestreo = d.create_sampler(&wgpu::SamplerDescriptor::default()); // vecino más cercano: letras nítidas
        let entrada = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty,
            count: None,
        };
        let disposicion = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cuadros"),
            entries: &[
                entrada(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entrada(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entrada(2, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let vista = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let grupo = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cuadros"),
            layout: &disposicion,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforme.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&vista) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&muestreo) },
            ],
        });
        (uniforme, atlas, disposicion, grupo)
    }

    /// La tubería: vértices de cuadros, el sombreador y mezcla con alfa premultiplicado.
    fn crear_tuberia(
        d: &wgpu::Device,
        sombreador: &wgpu::ShaderModule,
        diseno: &wgpu::PipelineLayout,
        formato: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let atributos = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32];
        d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cuadros"),
            layout: Some(diseno),
            vertex: wgpu::VertexState {
                module: sombreador,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: TAM_VERTICE as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &atributos,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: sombreador,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: formato,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// El tamaño de la superficie cambió (la ventana se redimensionó).
    pub fn redimensionar(&mut self, ancho: u32, alto: u32) {
        self.config.width = ancho.max(1);
        self.config.height = alto.max(1);
        if let Some(s) = &self.superficie {
            s.configure(&self.dispositivo, &self.config);
        }
    }

    /// Sube al atlas las filas `desde..hasta` (cada fila tiene `tam_atlas` bytes).
    pub fn subir_atlas(&mut self, pixeles: &[u8], desde: u32, hasta: u32) {
        let (desde, hasta) = (desde.min(self.tam_atlas), hasta.min(self.tam_atlas));
        if hasta <= desde {
            return;
        }
        let fila = self.tam_atlas as usize;
        let datos = &pixeles[desde as usize * fila..hasta as usize * fila];
        self.cola.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: desde, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            datos,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.tam_atlas),
                rows_per_image: Some(hasta - desde),
            },
            wgpu::Extent3d { width: self.tam_atlas, height: hasta - desde, depth_or_array_layers: 1 },
        );
    }

    /// Carga los cuadros de la página (una vez por maquetación).
    pub fn cargar(&mut self, cuadros: &[Cuadro]) {
        let (b, n) = self.bufer(cuadros, false);
        (self.vertices, self.cuantos) = (b, n);
    }

    /// Carga los cuadros fijos (la barra), que no se desplazan y van encima.
    pub fn cargar_fijos(&mut self, cuadros: &[Cuadro]) {
        let (b, n) = self.bufer(cuadros, true);
        (self.fijos, self.cuantos_fijos) = (b, n);
    }

    fn bufer(&self, cuadros: &[Cuadro], fijo: bool) -> (Option<wgpu::Buffer>, u32) {
        let mut v = Vec::with_capacity(cuadros.len() * 6);
        for c in cuadros {
            vertices(c, &mut v, fijo);
        }
        let b = (!v.is_empty()).then(|| {
            self.dispositivo.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cuadros"),
                contents: &Vertice::bytes(&v),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        (b, v.len() as u32)
    }

    /// Dibuja los cuadros cargados sobre `fondo`, con el documento desplazado `desp_y` píxeles.
    pub fn dibujar(&mut self, fondo: [f32; 4], desp_y: f32) -> Result<(), Error> {
        let Some(superficie) = &self.superficie else { return Err(String::from("sin ventana: usa capturar")) };
        let marco = match superficie.get_current_texture() {
            Ok(m) => m,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                superficie.configure(&self.dispositivo, &self.config);
                return Ok(()); // se vuelve a pedir en el siguiente cuadro
            }
            Err(e) => return Err(format!("marco: {e}")),
        };
        let vista = marco.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let orden = self.pasada(&vista, fondo, desp_y);
        self.cola.submit([orden]);
        marco.present();
        Ok(())
    }

    /// La pasada que pinta los cuadros en `vista`.
    fn pasada(&self, vista: &wgpu::TextureView, fondo: [f32; 4], desp_y: f32) -> wgpu::CommandBuffer {
        let uniforme = [self.config.width as f32, self.config.height as f32, 0.0, desp_y];
        let bytes: Vec<u8> = uniforme.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.cola.write_buffer(&self.uniforme, 0, &bytes);
        let mut orden =
            self.dispositivo.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pintar") });
        {
            let limpiar = wgpu::Color {
                r: f64::from(fondo[0]),
                g: f64::from(fondo[1]),
                b: f64::from(fondo[2]),
                a: f64::from(fondo[3]),
            };
            let mut pasada = orden.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("página"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: vista,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(limpiar), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pasada.set_pipeline(&self.tuberia);
            pasada.set_bind_group(0, &self.grupo, &[]);
            for (b, n) in [(&self.vertices, self.cuantos), (&self.fijos, self.cuantos_fijos)] {
                if let Some(b) = b {
                    pasada.set_vertex_buffer(0, b.slice(..));
                    pasada.draw(0..n, 0..1);
                }
            }
        }
        orden.finish()
    }

    /// Dibuja en una imagen y la devuelve en RGBA (`ancho × alto × 4` bytes, fila a fila).
    pub fn capturar(&mut self, fondo: [f32; 4], desp_y: f32) -> Result<Vec<u8>, Error> {
        let (ancho, alto) = (self.config.width, self.config.height);
        let imagen = self.dispositivo.create_texture(&wgpu::TextureDescriptor {
            label: Some("captura"),
            size: wgpu::Extent3d { width: ancho, height: alto, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let vista = imagen.create_view(&wgpu::TextureViewDescriptor::default());
        let pintar = self.pasada(&vista, fondo, desp_y);
        // Las filas de una copia a búfer van alineadas a 256 bytes.
        let fila = ancho * 4;
        let fila_alineada = fila.div_ceil(256) * 256;
        let bufer = self.dispositivo.create_buffer(&wgpu::BufferDescriptor {
            label: Some("captura"),
            size: u64::from(fila_alineada) * u64::from(alto),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut orden =
            self.dispositivo.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("copiar") });
        orden.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &imagen,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &bufer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(fila_alineada),
                    rows_per_image: Some(alto),
                },
            },
            wgpu::Extent3d { width: ancho, height: alto, depth_or_array_layers: 1 },
        );
        self.cola.submit([pintar, orden.finish()]);
        let trozo = bufer.slice(..);
        trozo.map_async(wgpu::MapMode::Read, |_| {});
        self.dispositivo.poll(wgpu::Maintain::Wait);
        let datos = trozo.get_mapped_range();
        let mut rgba = Vec::with_capacity((fila * alto) as usize);
        for y in 0..alto as usize {
            let inicio = y * fila_alineada as usize;
            rgba.extend_from_slice(&datos[inicio..inicio + fila as usize]);
        }
        drop(datos);
        bufer.unmap();
        Ok(rgba)
    }

    /// Tamaño del atlas (lado, en píxeles).
    pub fn tam_atlas(&self) -> u32 {
        self.tam_atlas
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn un_cuadro_son_dos_triangulos() {
        let mut v = Vec::new();
        vertices(
            &Cuadro { x: 10.0, y: 20.0, ancho: 30.0, alto: 40.0, color: [1.0, 0.0, 0.0, 1.0], letra: None },
            &mut v,
            false,
        );
        assert_eq!(v.len(), 6);
        assert_eq!((v[0].pos, v[4].pos, v[0].modo), ([10.0, 20.0], [40.0, 60.0], 0.0));
        assert_eq!(Vertice::bytes(&v).len(), 6 * TAM_VERTICE);
        let mut l = Vec::new();
        vertices(
            &Cuadro { x: 0.0, y: 0.0, ancho: 1.0, alto: 1.0, color: [0.0; 4], letra: Some([0.1, 0.2, 0.3, 0.4]) },
            &mut l,
            true,
        );
        assert_eq!((l[0].uv, l[4].uv, l[0].modo), ([0.1, 0.2], [0.3, 0.4], 3.0), "letra fija: 1 + 2");
    }
}
