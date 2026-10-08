//! Draws the cave chamber.
//!
//! Physical space is metres; this module maps it to canvas pixels. The chamber has
//! a fixed vertical extent, so the stalactite is seen to elongate against an
//! unchanging room rather than being rescaled on every frame.
//!
//! A direct port of the Elm `Scene` module.

use std::f64::consts::PI;

use crate::canvas as c;
use crate::physics as p;

pub const DIM: &str = "#8b98a8";
pub const FAINT: &str = "#5a6675";
pub const ACCENT: &str = "#e0b070";

/// Everything the renderer needs.
pub struct SceneOptions<'a> {
    pub w: f64,
    pub h: f64,
    pub env: &'a p::Env,
    pub sim: &'a p::Sim,
    /// seconds of wall-clock time, drives the drip animation
    pub time: f64,
    /// height of the modelled chamber, metres
    pub chamber_m: f64,
    pub show_rings: bool,
    /// height of the floor boss below the drip, metres (0 = none)
    pub stalagmite_m: f64,
}

pub struct Layout {
    pub cx: f64,
    pub ceil_y: f64,
    pub floor_y: f64,
    pub scale: f64,
}

pub fn layout(opts: &SceneOptions) -> Layout {
    let pad_top = 52.0;
    let pad_bottom = 62.0;
    let usable = (opts.h - pad_top - pad_bottom).max(120.0);
    Layout {
        cx: opts.w / 2.0,
        ceil_y: pad_top,
        floor_y: pad_top + usable,
        scale: usable / opts.chamber_m.max(0.05),
    }
}

fn to_y(l: &Layout, depth: f64) -> f64 {
    l.ceil_y + depth * l.scale
}

fn grid_step(chamber_m: f64) -> f64 {
    if chamber_m <= 0.35 {
        0.05
    } else if chamber_m <= 0.9 {
        0.1
    } else if chamber_m <= 2.2 {
        0.25
    } else {
        0.5
    }
}

fn chamber_glow(opts: &SceneOptions, l: &Layout) -> c::Shape {
    let cy = l.ceil_y + (l.floor_y - l.ceil_y) * 0.45;
    c::rect(
        0.0,
        0.0,
        opts.w,
        opts.h,
        c::radial(
            vec![
                (0.0, "rgba(126,158,196,0.11)"),
                (0.6, "rgba(90,120,150,0.03)"),
                (1.0, "rgba(0,0,0,0)"),
            ],
            l.cx,
            cy,
            8.0,
            l.cx,
            cy,
            opts.w * 0.78,
        ),
    )
}

fn rock_ceiling(opts: &SceneOptions, l: &Layout) -> c::Shape {
    c::rect(
        0.0,
        l.ceil_y - 52.0,
        opts.w,
        52.0,
        c::linear(
            vec![(0.0, "#40382b"), (0.5, "#2c261e"), (1.0, "#181510")],
            0.0,
            l.ceil_y - 52.0,
            0.0,
            l.ceil_y,
        ),
    )
}

fn depth_grid(opts: &SceneOptions, l: &Layout) -> c::Shape {
    let step = grid_step(opts.chamber_m);
    let count = (opts.chamber_m / step).floor() as i32;
    let mut out = Vec::new();
    for k in 1..=count {
        let y = to_y(l, k as f64 * step);
        out.push(c::segment(
            0.0,
            y,
            opts.w,
            y,
            c::Stroke::dashed("rgba(255,255,255,0.06)", 1.0, vec![2.0, 8.0]),
        ));
    }
    c::group(out)
}

/// The side of the formation catching the cave light.
fn lit_face(pts: &[(f64, f64)], l: &Layout) -> c::Shape {
    let mut poly: Vec<(f64, f64)> = pts
        .iter()
        .map(|(d, r)| (l.cx - r * l.scale * 0.46, to_y(l, *d)))
        .collect();
    poly.extend(
        pts.iter()
            .rev()
            .map(|(d, r)| (l.cx - r * l.scale * 0.6, to_y(l, *d))),
    );
    c::polygon(poly, c::solid("rgba(255,247,228,0.18)"))
}

/// Annual layering etched across the surface.
fn growth_rings(pts: &[(f64, f64)], l: &Layout) -> c::Shape {
    let every = (pts.len() / 22).max(1);
    let mut out = Vec::new();
    for (i, (d, r)) in pts.iter().enumerate() {
        if i % every != 0 {
            continue;
        }
        let y = to_y(l, *d);
        out.push(c::segment(
            l.cx - r * l.scale * 0.94,
            y,
            l.cx + r * l.scale * 0.94,
            y,
            c::Stroke::new("rgba(84,66,42,0.13)", 1.0),
        ));
    }
    c::group(out)
}

/// The water film: a bright lip on each side.
fn water_film(pts: &[(f64, f64)], l: &Layout) -> c::Shape {
    let right: Vec<(f64, f64)> = pts
        .iter()
        .map(|(d, r)| (l.cx + r * l.scale + 1.4, to_y(l, *d)))
        .collect();
    let left: Vec<(f64, f64)> = pts
        .iter()
        .map(|(d, r)| (l.cx - r * l.scale - 1.4, to_y(l, *d)))
        .collect();
    c::group(vec![
        c::polyline(right, c::Stroke::new("rgba(152,208,240,0.60)", 1.7)),
        c::polyline(left, c::Stroke::new("rgba(152,208,240,0.34)", 1.3)),
    ])
}

/// A dome drawn over the apex, so the tip reads as a rounded cap.
fn cap_arc(pts: &[(f64, f64)], l: &Layout) -> Vec<(f64, f64)> {
    let (d0, r0) = pts.first().copied().unwrap_or((0.0, p::SEED_RADIUS));
    let second = pts.get(4).copied().unwrap_or((d0, r0));
    let drop = (second.0 - d0).max(0.00015);
    let steps = 18;
    (0..=steps)
        .map(|k| {
            let a = PI * k as f64 / steps as f64;
            (
                l.cx + r0 * l.scale * a.cos(),
                to_y(l, d0) - drop * l.scale * a.sin() * 0.85,
            )
        })
        .collect()
}

/// Falling drops. The rhythm follows the drip rate; the acceleration follows
/// gravity, so a slow drip visibly spaces the stream out.
fn falling_drops(opts: &SceneOptions, l: &Layout, apex: (f64, f64)) -> c::Shape {
    let (apex_depth, apex_r) = apex;
    let period = (p::drip_interval_from_rate(opts.env.drip_rate) / 24.0).clamp(0.12, 12.0);
    let fall = (opts.chamber_m - apex_depth).max(0.05);
    let count = 8;
    let mut out = Vec::new();
    for k in 0..count {
        let raw = ((opts.time / period + k as f64 * 0.41) * 1000.0) % 1000.0;
        let phase = raw.round() / 1000.0;
        // constant acceleration: distance ~ phase^2
        let frac = phase * phase;
        let y = to_y(l, apex_depth + fall * frac);
        let stretch = 1.0 + phase * 1.4;
        let alpha = (0.9 - phase * 0.6).clamp(0.05, 0.9);
        out.push(c::ellipse(
            l.cx + apex_r * l.scale * 0.05,
            y,
            1.7 / stretch.sqrt(),
            2.4 * stretch,
            c::solid_color(&c::rgba(158, 210, 242, alpha)),
        ));
    }
    c::group(out)
}

/// The stalagmite growing on the floor below.
fn stalagmite(opts: &SceneOptions, l: &Layout) -> c::Shape {
    if opts.stalagmite_m <= 0.002 {
        return c::group(Vec::new());
    }
    let h = opts.stalagmite_m.min(opts.chamber_m * 0.5);
    let w = h * 0.9;
    let steps = 18;
    let mut outline: Vec<(f64, f64)> = (0..=steps)
        .map(|k| {
            let a = PI * k as f64 / steps as f64;
            (
                l.cx + w * l.scale * a.cos(),
                l.floor_y - h * l.scale * a.sin(),
            )
        })
        .collect();
    outline.push((l.cx - w * l.scale, l.floor_y + 10.0));
    outline.push((l.cx + w * l.scale, l.floor_y + 10.0));
    c::polygon(
        outline,
        c::linear(
            vec![
                (0.0, "rgba(214,192,152,0.92)"),
                (1.0, "rgba(104,88,66,0.92)"),
            ],
            l.cx,
            l.floor_y - h * l.scale,
            l.cx,
            l.floor_y,
        ),
    )
}

fn axis_legend(opts: &SceneOptions, l: &Layout) -> c::Shape {
    let bar_m = grid_step(opts.chamber_m);
    let px = bar_m * l.scale;
    let x = 20.0;
    let y = l.floor_y + 30.0;
    let length_cm = (p::length_of(opts.sim) * 100.0 * 100.0).round() / 100.0;

    c::group(vec![
        c::segment(x, y, x + px, y, c::Stroke::new(DIM, 1.5)),
        c::segment(x, y - 4.0, x, y + 4.0, c::Stroke::new(DIM, 1.5)),
        c::segment(x + px, y - 4.0, x + px, y + 4.0, c::Stroke::new(DIM, 1.5)),
        c::label(x + px + 8.0, y + 4.0, format!("{bar_m} m"), DIM, "left"),
        c::label(
            opts.w - 18.0,
            y + 4.0,
            format!("length {length_cm:.2} cm"),
            ACCENT,
            "right",
        ),
    ])
}

/// Build the whole canvas scene as a value.
pub fn draw(opts: &SceneOptions) -> c::Draw {
    let l = layout(opts);
    let pts = p::profile_points(opts.sim);

    let mut body: Vec<(f64, f64)> = pts
        .iter()
        .map(|(d, r)| (l.cx + r * l.scale, to_y(&l, *d)))
        .collect();
    body.extend(
        pts.iter()
            .rev()
            .map(|(d, r)| (l.cx - r * l.scale, to_y(&l, *d))),
    );

    let apex = pts.first().copied().unwrap_or((0.0, p::SEED_RADIUS));

    c::Draw {
        w: opts.w,
        h: opts.h,
        bg: "#070a0e",
        shapes: vec![
            c::rect(
                0.0,
                0.0,
                opts.w,
                opts.h,
                c::linear(
                    vec![(0.0, "#0e141c"), (0.5, "#0a0e14"), (1.0, "#06080b")],
                    0.0,
                    0.0,
                    0.0,
                    opts.h,
                ),
            ),
            chamber_glow(opts, &l),
            rock_ceiling(opts, &l),
            depth_grid(opts, &l),
            c::polygon(
                body,
                c::linear(
                    vec![
                        (0.0, "#dcc7a0"),
                        (0.2, "#c4ad86"),
                        (0.65, "#9e8867"),
                        (1.0, "#79664c"),
                    ],
                    l.cx - 60.0,
                    to_y(&l, 0.0),
                    l.cx + 60.0,
                    to_y(&l, opts.chamber_m),
                ),
            ),
            lit_face(&pts, &l),
            if opts.show_rings {
                growth_rings(&pts, &l)
            } else {
                c::group(Vec::new())
            },
            water_film(&pts, &l),
            c::polyline(
                cap_arc(&pts, &l),
                c::Stroke::new("rgba(255,248,232,0.30)", 1.3),
            ),
            falling_drops(opts, &l, apex),
            stalagmite(opts, &l),
            axis_legend(opts, &l),
        ],
    }
}
