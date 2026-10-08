//! Drawing layer over the canvas 2D context.
//!
//! A scene is built as a plain value — a vector of `Shape` — and handed to a
//! single `draw` call. This mirrors the Elm app's `Canvas` module: the scene is
//! data, so it can be inspected or tested without a live canvas, and the only code
//! that touches the DOM is `draw` itself.

use std::f64::consts::PI;

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::CanvasRenderingContext2d;

/// A gradient stop: position 0..1 and a CSS colour.
pub type Stop = (f64, &'static str);

/// Fill styles.
#[derive(Clone, Debug)]
pub enum Fill {
    Flat(&'static str),
    /// A colour built at runtime; kept separate so the common case stays `&'static`.
    FlatOwned(String),
    Linear {
        stops: Vec<Stop>,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
    Radial {
        stops: Vec<Stop>,
        x0: f64,
        y0: f64,
        r0: f64,
        x1: f64,
        y1: f64,
        r1: f64,
    },
}

/// Stroke styles.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub colour: &'static str,
    pub width: f64,
    pub dash: Vec<f64>,
}

impl Stroke {
    pub fn new(colour: &'static str, width: f64) -> Self {
        Self {
            colour,
            width,
            dash: Vec::new(),
        }
    }

    pub fn dashed(colour: &'static str, width: f64, dash: Vec<f64>) -> Self {
        Self {
            colour,
            width,
            dash,
        }
    }
}

/// Every primitive the renderer understands.
#[derive(Clone, Debug)]
pub enum Shape {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: Fill,
    },
    Circle {
        x: f64,
        y: f64,
        r: f64,
        fill: Fill,
        stroke: Option<Stroke>,
    },
    Ellipse {
        x: f64,
        y: f64,
        rx: f64,
        ry: f64,
        fill: Fill,
        stroke: Option<Stroke>,
    },
    /// `closed == true` fills, `closed == false` is an open polyline.
    Poly {
        closed: bool,
        points: Vec<(f64, f64)>,
        fill: Fill,
        stroke: Option<Stroke>,
    },
    Seg {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        stroke: Stroke,
    },
    Label {
        x: f64,
        y: f64,
        text: String,
        font: &'static str,
        colour: &'static str,
        align: &'static str,
    },
    Group(Vec<Shape>),
}

/// A whole scene.
#[derive(Clone, Debug)]
pub struct Draw {
    pub w: f64,
    pub h: f64,
    pub bg: &'static str,
    pub shapes: Vec<Shape>,
}

// --- constructors -----------------------------------------------------------

pub fn solid(colour: &'static str) -> Fill {
    Fill::Flat(colour)
}

/// A fill from an owned colour string, for values computed at runtime such as a
/// drop's fading alpha.
pub fn solid_color(colour: &str) -> Fill {
    Fill::FlatOwned(colour.to_string())
}

pub fn linear(stops: Vec<Stop>, x0: f64, y0: f64, x1: f64, y1: f64) -> Fill {
    Fill::Linear {
        stops,
        x0,
        y0,
        x1,
        y1,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn radial(stops: Vec<Stop>, x0: f64, y0: f64, r0: f64, x1: f64, y1: f64, r1: f64) -> Fill {
    Fill::Radial {
        stops,
        x0,
        y0,
        r0,
        x1,
        y1,
        r1,
    }
}

pub fn rect(x: f64, y: f64, w: f64, h: f64, fill: Fill) -> Shape {
    Shape::Rect { x, y, w, h, fill }
}

pub fn circle(x: f64, y: f64, r: f64, fill: Fill) -> Shape {
    Shape::Circle {
        x,
        y,
        r,
        fill,
        stroke: None,
    }
}

pub fn ellipse(x: f64, y: f64, rx: f64, ry: f64, fill: Fill) -> Shape {
    Shape::Ellipse {
        x,
        y,
        rx,
        ry,
        fill,
        stroke: None,
    }
}

pub fn polygon(points: Vec<(f64, f64)>, fill: Fill) -> Shape {
    Shape::Poly {
        closed: true,
        points,
        fill,
        stroke: None,
    }
}

pub fn polyline(points: Vec<(f64, f64)>, stroke: Stroke) -> Shape {
    Shape::Poly {
        closed: false,
        points,
        fill: solid("rgba(0,0,0,0)"),
        stroke: Some(stroke),
    }
}

pub fn segment(x0: f64, y0: f64, x1: f64, y1: f64, stroke: Stroke) -> Shape {
    Shape::Seg {
        x0,
        y0,
        x1,
        y1,
        stroke,
    }
}

pub fn label(
    x: f64,
    y: f64,
    text: impl Into<String>,
    colour: &'static str,
    align: &'static str,
) -> Shape {
    Shape::Label {
        x,
        y,
        text: text.into(),
        font: "400 12px ui-sans-serif, system-ui, sans-serif",
        colour,
        align,
    }
}

pub fn group(shapes: Vec<Shape>) -> Shape {
    Shape::Group(shapes)
}

/// `rgba()` from components, clamping alpha to a sane range.
pub fn rgba(r: u8, g: u8, b: u8, a: f64) -> String {
    let alpha = (a.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
    format!("rgba({r},{g},{b},{alpha})")
}

// --- rendering --------------------------------------------------------------

/// Apply a fill to the context.
///
/// Each fill type has its own setter: the generic `JsValue` one is deprecated,
/// and casting a gradient through `JsValue` just to reach it is what the typed
/// setters exist to avoid.
fn apply_fill(ctx: &CanvasRenderingContext2d, fill: &Fill) {
    match fill {
        Fill::Flat(c) => ctx.set_fill_style_str(c),
        Fill::FlatOwned(c) => ctx.set_fill_style_str(c),
        Fill::Linear {
            stops,
            x0,
            y0,
            x1,
            y1,
        } => {
            let g = ctx.create_linear_gradient(*x0, *y0, *x1, *y1);
            for (p, c) in stops {
                let _ = g.add_color_stop(p.clamp(0.0, 1.0) as f32, c);
            }
            ctx.set_fill_style_canvas_gradient(&g);
        }
        Fill::Radial {
            stops,
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
        } => {
            // A radial gradient with a zero radius is a hard error in the spec,
            // so both radii are floored.
            if let Ok(g) = ctx.create_radial_gradient(
                *x0,
                *y0,
                r0.max(0.0),
                *x1,
                *y1,
                r1.max(0.01),
            ) {
                for (p, c) in stops {
                    let _ = g.add_color_stop(p.clamp(0.0, 1.0) as f32, c);
                }
                ctx.set_fill_style_canvas_gradient(&g);
            }
        }
    }
}

fn apply_stroke(ctx: &CanvasRenderingContext2d, s: &Stroke) {
    ctx.set_stroke_style_str(s.colour);
    ctx.set_line_width(s.width);
    ctx.set_line_cap("round");
    ctx.set_line_join("round");
    let dash = js_sys::Array::new();
    for d in &s.dash {
        dash.push(&JsValue::from_f64(*d));
    }
    ctx.set_line_dash(&dash).ok();
}

fn draw_shape(ctx: &CanvasRenderingContext2d, shape: &Shape) {
    match shape {
        Shape::Rect { x, y, w, h, fill } => {
            apply_fill(ctx, fill);
            ctx.fill_rect(*x, *y, *w, *h);
        }
        Shape::Circle {
            x,
            y,
            r,
            fill,
            stroke,
        } => {
            ctx.begin_path();
            let _ = ctx.arc(*x, *y, r.max(0.01), 0.0, PI * 2.0);
            apply_fill(ctx, fill);
            ctx.fill();
            if let Some(s) = stroke {
                apply_stroke(ctx, s);
                ctx.stroke();
            }
        }
        Shape::Ellipse {
            x,
            y,
            rx,
            ry,
            fill,
            stroke,
        } => {
            ctx.begin_path();
            let _ = ctx.ellipse(*x, *y, rx.max(0.01), ry.max(0.01), 0.0, 0.0, PI * 2.0);
            apply_fill(ctx, fill);
            ctx.fill();
            if let Some(s) = stroke {
                apply_stroke(ctx, s);
                ctx.stroke();
            }
        }
        Shape::Poly {
            closed,
            points,
            fill,
            stroke,
        } => {
            if points.len() < 2 {
                return;
            }
            ctx.begin_path();
            for (i, (x, y)) in points.iter().enumerate() {
                if i == 0 {
                    ctx.move_to(*x, *y);
                } else {
                    ctx.line_to(*x, *y);
                }
            }
            if *closed {
                ctx.close_path();
            }
            apply_fill(ctx, fill);
            ctx.fill();
            if let Some(s) = stroke {
                apply_stroke(ctx, s);
                ctx.stroke();
            }
            // reset so a dash never leaks into the next shape
            ctx.set_line_dash(&js_sys::Array::new()).ok();
        }
        Shape::Seg {
            x0,
            y0,
            x1,
            y1,
            stroke,
        } => {
            apply_stroke(ctx, stroke);
            ctx.begin_path();
            ctx.move_to(*x0, *y0);
            ctx.line_to(*x1, *y1);
            ctx.stroke();
            ctx.set_line_dash(&js_sys::Array::new()).ok();
        }
        Shape::Label {
            x,
            y,
            text,
            font,
            colour,
            align,
        } => {
            ctx.set_font(font);
            ctx.set_fill_style_str(colour);
            ctx.set_text_align(align);
            ctx.set_text_baseline("alphabetic");
            let _ = ctx.fill_text(text, *x, *y);
        }
        Shape::Group(inner) => {
            for s in inner {
                draw_shape(ctx, s);
            }
        }
    }
}

/// Draw a scene onto a canvas.
///
/// The backing store is matched to the CSS box times the device pixel ratio so the
/// drawing is crisp on high-density displays. The scene is authored in CSS pixels,
/// so the context is scaled instead of the geometry.
pub fn draw(canvas: &web_sys::HtmlCanvasElement, scene: &Draw) {
    let win = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let dpr = win.device_pixel_ratio().clamp(1.0, 2.0);

    let box_w = canvas.get_bounding_client_rect().width();
    let css_w = if box_w > 0.0 { box_w } else { scene.w }.max(240.0);
    let css_h = css_w * (scene.h / scene.w);

    let bw = (css_w * dpr).round() as u32;
    let bh = (css_h * dpr).round() as u32;
    if canvas.width() != bw || canvas.height() != bh {
        canvas.set_width(bw);
        canvas.set_height(bh);
    }
    let _ = canvas.style().set_property("height", &format!("{css_h}px"));

    let ctx = match canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|o| o.dyn_into::<CanvasRenderingContext2d>().ok())
    {
        Some(c) => c,
        None => return,
    };

    ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0).ok();
    ctx.clear_rect(0.0, 0.0, css_w, css_h);

    // the scene was authored at scene.w; scale to the actual CSS width
    let k = css_w / scene.w;
    ctx.save();
    ctx.scale(k, k).ok();
    ctx.set_fill_style_str(scene.bg);
    ctx.fill_rect(0.0, 0.0, scene.w, scene.h);
    for s in &scene.shapes {
        draw_shape(&ctx, s);
    }
    ctx.restore();
    ctx.set_line_dash(&js_sys::Array::new()).ok();
}

/// Count primitives in a scene. Used by the tests.
pub fn count_shapes(shapes: &[Shape]) -> std::collections::BTreeMap<&'static str, usize> {
    let mut out = std::collections::BTreeMap::new();
    fn walk(shapes: &[Shape], out: &mut std::collections::BTreeMap<&'static str, usize>) {
        for s in shapes {
            let key = match s {
                Shape::Rect { .. } => "rect",
                Shape::Circle { .. } => "circle",
                Shape::Ellipse { .. } => "ellipse",
                Shape::Poly { .. } => "poly",
                Shape::Seg { .. } => "seg",
                Shape::Label { .. } => "label",
                Shape::Group(_) => "group",
            };
            *out.entry(key).or_insert(0) += 1;
            if let Shape::Group(inner) = s {
                walk(inner, out);
            }
        }
    }
    walk(shapes, &mut out);
    out
}
