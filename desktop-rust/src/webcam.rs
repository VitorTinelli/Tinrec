use anyhow::{Context, Result, bail};
use pinray::{CaptureEvent, Rect};
use std::time::{Duration, Instant};

pub struct Webcam {
    camera: Option<crate::camera::Camera>,
    enabled: bool,
    draw: bool,
    position: (f64, f64),
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}
impl Webcam {
    pub fn composites_frame(&self) -> bool {
        self.enabled && self.draw
    }
    pub fn start(
        options: &crate::RecordOptions,
        synthetic: bool,
        commands: &std::sync::mpsc::Receiver<crate::recording::Control>,
    ) -> Result<Option<Self>> {
        let Some(device) = &options.webcam else {
            return Ok(None);
        };
        if options.capture_mode == "camera" {
            bail!("Webcam sobreposta exige captura de tela, janela ou área");
        }
        let mut webcam = Self {
            camera: None,
            enabled: !options.webcam_disabled,
            draw: !options.webcam_visible_in_capture || synthetic,
            position: (options.webcam_x, options.webcam_y),
            pixels: vec![255, 0, 0, 255],
            width: 1,
            height: 1,
        };
        if !synthetic && webcam.enabled {
            webcam.camera = Some(crate::camera::Camera::start_webcam(options, device)?);
            webcam.pixels.clear();
            let deadline = Instant::now() + Duration::from_secs(10);
            while webcam.pixels.is_empty() && Instant::now() < deadline {
                if commands
                    .try_iter()
                    .any(|command| matches!(command, crate::recording::Control::Stop))
                {
                    bail!("Gravação cancelada");
                }
                webcam.update(Duration::from_millis(50))?;
            }
            if webcam.pixels.is_empty() {
                bail!("A webcam não enviou imagem. Confira a permissão e o dispositivo.");
            }
        }
        Ok(Some(webcam))
    }
    pub fn set_enabled(
        &mut self,
        enabled: bool,
        options: &crate::RecordOptions,
        synthetic: bool,
    ) -> Result<()> {
        if enabled && !self.enabled && !synthetic {
            self.camera = Some(crate::camera::Camera::start_webcam(
                options,
                options.webcam.as_deref().context("Escolha uma webcam")?,
            )?);
        }
        if !enabled {
            self.camera.take();
        }
        self.enabled = enabled;
        Ok(())
    }
    pub fn set_position(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.position = (x.clamp(0., 1.), y.clamp(0., 1.));
        }
    }
    pub fn preview(&self) -> Option<crate::recording::PreviewFrame> {
        if !self.enabled {
            return None;
        }
        let mut rgb = Vec::with_capacity((self.width * self.height * 3) as usize);
        for pixel in self.pixels.as_chunks::<4>().0 {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        Some(crate::recording::PreviewFrame {
            width: self.width,
            height: self.height,
            source_width: self.width,
            source_height: self.height,
            rgb,
        })
    }
    fn update(&mut self, timeout: Duration) -> Result<()> {
        if let Some(camera) = &mut self.camera {
            match camera.next_event(Some(timeout)) {
                Ok(CaptureEvent::Video(frame)) => {
                    self.pixels = frame
                        .to_tight_bytes()
                        .context("Quadro da webcam indisponível")?;
                    self.width = frame.width;
                    self.height = frame.height;
                }
                Err(pinray::PinrayError::Timeout(_)) => {}
                Err(error) => return Err(error.into()),
                Ok(CaptureEvent::End) => bail!("Webcam encerrada"),
                _ => {}
            }
        }
        Ok(())
    }
    pub fn paint(
        &mut self,
        data: &mut [u8],
        width: u32,
        height: u32,
        crop: Option<Rect>,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.update(Duration::ZERO)?;
        if !self.draw {
            return Ok(());
        }
        let region = crop.unwrap_or(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        let diameter = (region.height / 5).min(region.width / 2).max(1);
        let margin = (region.height / 40).min(region.width.saturating_sub(diameter));
        let span_x = region.width.saturating_sub(diameter + 2 * margin);
        let span_y = region.height.saturating_sub(diameter + 2 * margin);
        let left = region.x as u32 + margin + (f64::from(span_x) * self.position.0).round() as u32;
        let top = region.y as u32 + margin + (f64::from(span_y) * self.position.1).round() as u32;
        let side = self.width.min(self.height);
        let radius = diameter as f64 / 2.;
        for y in 0..diameter {
            for x in 0..diameter {
                let distance =
                    ((x as f64 + 0.5 - radius).powi(2) + (y as f64 + 0.5 - radius).powi(2)).sqrt();
                if distance > radius {
                    continue;
                }
                let sx = (self.width - side) / 2 + x * side / diameter;
                let sy = (self.height - side) / 2 + y * side / diameter;
                let src = ((sy * self.width + sx) * 4) as usize;
                let dst = (((top + y) * width + left + x) * 4) as usize;
                if dst + 4 <= data.len() && src + 4 <= self.pixels.len() {
                    if distance > radius - 2. {
                        data[dst..dst + 4].copy_from_slice(&[255; 4]);
                    } else {
                        data[dst..dst + 4].copy_from_slice(&self.pixels[src..src + 4]);
                    }
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circle_stays_inside_selected_area() {
        let mut webcam = Webcam {
            camera: None,
            enabled: true,
            draw: true,
            position: (0., 1.),
            pixels: vec![255, 0, 0, 255],
            width: 1,
            height: 1,
        };
        let mut image = vec![0; 640 * 360 * 4];
        webcam
            .paint(
                &mut image,
                640,
                360,
                Some(Rect {
                    x: 100,
                    y: 50,
                    width: 320,
                    height: 240,
                }),
            )
            .unwrap();
        assert_eq!(&image[((260 * 640 + 130) * 4)..][..4], &[255, 0, 0, 255]);
        assert_eq!(&image[((266 * 640 + 106) * 4)..][..4], &[0, 0, 0, 0]);
        assert!(image[..(50 * 640 * 4)].iter().all(|p| *p == 0));
    }
}
