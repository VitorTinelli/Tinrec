//! One-shot desktop capture for Windows prints. Recording keeps its WGC pipeline.
use crate::Reply;
#[cfg(target_os = "windows")]
pub fn capture(x: i32, y: i32, width: u32, height: u32) -> Reply<image::DynamicImage> {
    use windows_sys::Win32::Graphics::Gdi::*;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 40_000_000 {
        return Err("Dimensões de captura inválidas".into());
    }
    struct Resources {
        screen: HDC,
        memory: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
    }
    impl Drop for Resources {
        fn drop(&mut self) {
            unsafe {
                if !self.memory.is_null() && !self.previous.is_null() {
                    SelectObject(self.memory, self.previous);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
                if !self.memory.is_null() {
                    DeleteDC(self.memory);
                }
                if !self.screen.is_null() {
                    ReleaseDC(std::ptr::null_mut(), self.screen);
                }
            }
        }
    }
    unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmFlush();
    }
    let mut r = Resources {
        screen: unsafe { GetDC(std::ptr::null_mut()) },
        memory: std::ptr::null_mut(),
        bitmap: std::ptr::null_mut(),
        previous: std::ptr::null_mut(),
    };
    if r.screen.is_null() {
        return Err("Não foi possível acessar a tela".into());
    }
    r.memory = unsafe { CreateCompatibleDC(r.screen) };
    r.bitmap = unsafe { CreateCompatibleBitmap(r.screen, width as i32, height as i32) };
    if r.memory.is_null() || r.bitmap.is_null() {
        return Err("Não foi possível criar a imagem da tela".into());
    }
    r.previous = unsafe { SelectObject(r.memory, r.bitmap) };
    if r.previous.is_null() || r.previous as isize == -1 {
        return Err("Não foi possível preparar a captura".into());
    }
    if unsafe {
        BitBlt(
            r.memory,
            0,
            0,
            width as i32,
            height as i32,
            r.screen,
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        )
    } == 0
    {
        return Err(format!(
            "Falha ao capturar tela: {}",
            std::io::Error::last_os_error()
        ));
    }
    unsafe {
        SelectObject(r.memory, r.previous);
    }
    r.previous = std::ptr::null_mut();
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    if unsafe {
        GetDIBits(
            r.screen,
            r.bitmap,
            0,
            height,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        )
    } != height as i32
    {
        return Err("Não foi possível ler os pixels da tela".into());
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    image::RgbaImage::from_raw(width, height, pixels)
        .map(image::DynamicImage::ImageRgba8)
        .ok_or("Imagem da tela inválida".into())
}
/// Uncompressed, top-down BMP V4: no compression or base64 on the selection path.
pub fn bitmap(image: &image::DynamicImage) -> Reply<Vec<u8>> {
    let rgba = image.as_rgba8().ok_or("Formato da prévia inválido")?;
    if u64::from(rgba.width()) * u64::from(rgba.height()) > 40_000_000 {
        return Err("Imagem muito grande".into());
    }
    let mut bytes = vec![0u8; 122];
    bytes[0..2].copy_from_slice(b"BM");
    let length = 122 + rgba.as_raw().len() as u32;
    for (offset, value) in [
        (2, length),
        (10, 122),
        (14, 108),
        (18, rgba.width()),
        (30, 3),
        (34, rgba.as_raw().len() as u32),
        (54, 0x00ff0000),
        (58, 0x0000ff00),
        (62, 0x000000ff),
        (66, 0xff000000),
        (70, 0x73524742),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[22..26].copy_from_slice(&(-(rgba.height() as i32)).to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(rgba.as_raw());
    for pixel in bytes[122..].as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn bitmap_preserves_rgba_channels_and_top_down_rows() {
        let source = image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([(x * 60) as u8, (y * 90) as u8, 123, 200])
        }));
        let data = super::bitmap(&source).unwrap();
        assert_eq!(
            image::load_from_memory(&data).unwrap().to_rgba8(),
            source.to_rgba8()
        );
    }
}
