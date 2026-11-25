use gtk::gdk::prelude::GdkCairoContextExt;
use gtk::{cairo, gdk_pixbuf};

pub const MODES: [&str; 5] = ["fill", "fit", "stretch", "center", "tile"];

pub fn mode_index(value: Option<&str>) -> u32 {
    MODES
        .iter()
        .position(|mode| Some(*mode) == value)
        .unwrap_or_default() as u32
}

#[derive(Clone)]
pub struct Background {
    pattern: cairo::Pattern,
    width: f64,
    height: f64,
}

impl Background {
    pub fn new(pixbuf: &gdk_pixbuf::Pixbuf) -> Option<Self> {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).ok()?;
        let context = cairo::Context::new(&surface).ok()?;
        context.set_source_pixbuf(pixbuf, 0.0, 0.0);
        Some(Self {
            pattern: context.source(),
            width: pixbuf.width().into(),
            height: pixbuf.height().into(),
        })
    }

    pub fn draw(
        &self,
        context: &cairo::Context,
        width: i32,
        height: i32,
        mode: &str,
        native_scale: f64,
    ) {
        let (width, height) = (f64::from(width), f64::from(height));
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        let (x_ratio, y_ratio) = (width / self.width, height / self.height);
        let (x_scale, y_scale) = match mode {
            "stretch" => (x_ratio, y_ratio),
            "center" | "tile" => (native_scale, native_scale),
            "fit" => (x_ratio.min(y_ratio), x_ratio.min(y_ratio)),
            _ => (x_ratio.max(y_ratio), x_ratio.max(y_ratio)),
        };
        let tile = mode == "tile";
        let (x, y) = if tile {
            (0.0, 0.0)
        } else {
            (
                (width - self.width * x_scale) / 2.0,
                (height - self.height * y_scale) / 2.0,
            )
        };
        if context.save().is_err() {
            return;
        }
        context.translate(x, y);
        context.scale(x_scale, y_scale);
        self.pattern.set_extend(if tile {
            cairo::Extend::Repeat
        } else {
            cairo::Extend::Pad
        });
        if context.set_source(&self.pattern).is_ok() {
            if tile {
                let _ = context.paint();
            } else {
                context.rectangle(0.0, 0.0, self.width, self.height);
                let _ = context.fill();
            }
        }
        let _ = context.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_center_fit_and_tile() {
        let bytes = gtk::glib::Bytes::from_static(&[255, 0, 0, 255, 255, 0, 0, 255]);
        let image =
            gdk_pixbuf::Pixbuf::from_bytes(&bytes, gdk_pixbuf::Colorspace::Rgb, true, 8, 2, 1, 8);
        let image = Background::new(&image).unwrap();
        let render = |mode| {
            let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 8, 7).unwrap();
            {
                let context = cairo::Context::new(&surface).unwrap();
                image.draw(&context, 8, 7, mode, 1.0);
            }
            let data = surface.data().unwrap();
            let pixel = |x: usize, y: usize| -> u32 {
                let offset = (y * 8 + x) * 4;
                u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap())
            };
            (pixel(0, 0), pixel(0, 3), pixel(3, 3))
        };
        let red = 0xffff0000;
        assert_eq!(render("center"), (0, 0, red));
        assert_eq!(render("fit"), (0, red, red));
        assert_eq!(render("tile"), (red, red, red));
        assert_eq!(render("fill"), (red, red, red));
        assert_eq!(render("stretch"), (red, red, red));
    }
}
