//! Las imágenes en la GPU: una textura RGBA por imagen, enlazada en el grupo 1 del sombreador.

/// Cómo ve el sombreador una imagen (grupo 1): la textura y su muestreo lineal.
pub(crate) fn disposicion_imagen(d: &wgpu::Device) -> wgpu::BindGroupLayout {
    d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("imagen"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Una textura RGBA con sus píxeles y el grupo que la enlaza.
pub(crate) fn textura_imagen(
    d: &wgpu::Device,
    cola: &wgpu::Queue,
    (disposicion, muestreo): (&wgpu::BindGroupLayout, &wgpu::Sampler),
    (ancho, alto): (u32, u32),
    rgba: &[u8],
) -> (wgpu::Texture, wgpu::BindGroup) {
    let tam = wgpu::Extent3d { width: ancho, height: alto, depth_or_array_layers: 1 };
    let textura = d.create_texture(&wgpu::TextureDescriptor {
        label: Some("imagen"),
        size: tam,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    cola.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &textura,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(ancho * 4), rows_per_image: Some(alto) },
        tam,
    );
    let vista = textura.create_view(&wgpu::TextureViewDescriptor::default());
    let grupo = d.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("imagen"),
        layout: disposicion,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&vista) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(muestreo) },
        ],
    });
    (textura, grupo)
}

/// Reduce una imagen para que ningún lado pase de `max` (cada píxel, el más cercano).
pub(crate) fn reducir(ancho: u32, alto: u32, rgba: &[u8], max: u32) -> ((u32, u32), Vec<u8>) {
    let f = f64::from(max) / f64::from(ancho.max(alto));
    let (w, h) = (((f64::from(ancho) * f) as u32).clamp(1, max), ((f64::from(alto) * f) as u32).clamp(1, max));
    let mut v = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        let sy = (u64::from(y) * u64::from(alto) / u64::from(h)) as usize;
        for x in 0..w {
            let sx = (u64::from(x) * u64::from(ancho) / u64::from(w)) as usize;
            let i = (sy * ancho as usize + sx) * 4;
            v.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    ((w, h), v)
}

/// Alfa premultiplicado: así el filtrado lineal no deja halos en los bordes transparentes.
pub(crate) fn premultiplicar(rgba: &mut [u8]) {
    for p in rgba.chunks_exact_mut(4) {
        let a = u16::from(p[3]);
        if a < 255 {
            for c in &mut p[..3] {
                *c = ((u16::from(*c) * a + 127) / 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn reducir_guarda_la_proporcion() {
        let rgba: Vec<u8> = (0..400 * 100 * 4).map(|i| (i / 4 % 256) as u8).collect();
        let ((w, h), v) = reducir(400, 100, &rgba, 100);
        assert_eq!((w, h, v.len()), (100, 25, 100 * 25 * 4));
    }

    #[test]
    fn premultiplica() {
        let mut p = [200, 100, 50, 128, 10, 20, 30, 255, 9, 9, 9, 0];
        premultiplicar(&mut p);
        assert_eq!(p, [100, 50, 25, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
    }
}
