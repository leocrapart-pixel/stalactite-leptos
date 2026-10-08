//! Small view helpers and the dark "cave instrument" theme, mirroring the Elm
//! app's `Ui` module.

pub const PAPER: &str = "#0a0c10";
pub const INK: &str = "#e8eef6";
pub const DIM: &str = "#8b98a8";
pub const FAINT: &str = "#5a6675";
pub const ACCENT: &str = "#e0b070";
pub const ACCENT_SOFT: &str = "rgba(224,176,112,0.14)";
pub const WATER: &str = "#6fb3d8";
pub const PANEL_BG: &str = "rgba(18,22,29,0.92)";
pub const OK: &str = "#79c98a";
pub const WARN: &str = "#e0805a";

/// Inline styles, kept as strings so the markup stays declarative.
pub const SHEET: &str = "display:flex;flex-direction:column;gap:14px;padding:14px;background:rgba(18,22,29,0.92);border:1px solid #5a6675;border-radius:10px;backdrop-filter:blur(6px)";

pub const SECTION: &str = "display:flex;flex-direction:column;gap:8px";

pub const SECTION_TITLE: &str = "font:600 10px ui-monospace, monospace;letter-spacing:0.16em;text-transform:uppercase;color:#e0b070;opacity:0.85";

pub const HINT: &str = "font:400 10.5px ui-sans-serif, system-ui, sans-serif;color:#5a6675;line-height:1.45";

pub const ROW: &str = "display:flex;justify-content:space-between;align-items:baseline;gap:10px;font:11.5px ui-monospace, monospace";

pub const ROW_VALUE: &str = "font-weight:600;text-align:right";

pub const SLIDER_NAME: &str = "font:500 11.5px ui-sans-serif, system-ui, sans-serif;color:#e8eef6";

pub const SLIDER_VALUE: &str =
    "font:600 11.5px ui-monospace, monospace;color:#e0b070;white-space:nowrap";

pub const SLIDER_INPUT: &str =
    "width:100%;height:18px;accent-color:#e0b070;cursor:pointer;background:transparent";

pub const BUTTON_ON: &str = "font:600 11px ui-sans-serif, system-ui, sans-serif;padding:7px 11px;border-radius:7px;cursor:pointer;border:1px solid #e0b070;background:rgba(224,176,112,0.14);color:#e0b070;transition:all 120ms;white-space:nowrap";

pub const BUTTON_OFF: &str = "font:600 11px ui-sans-serif, system-ui, sans-serif;padding:7px 11px;border-radius:7px;cursor:pointer;border:1px solid #5a6675;background:rgba(255,255,255,0.03);color:#e8eef6;transition:all 120ms;white-space:nowrap";

// --- colour helpers ---------------------------------------------------------

fn rgba(r: u8, g: u8, b: u8, a: f64) -> String {
    let alpha = (a.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
    format!("rgba({r},{g},{b},{alpha})")
}

pub const BADGE_WATER: &str = "font:600 10px ui-monospace, monospace;padding:2px 7px;border-radius:999px;border:1px solid #6fb3d8;color:#6fb3d8;background:rgba(255,255,255,0.03)";
pub const BADGE_ACCENT: &str = "font:600 10px ui-monospace, monospace;padding:2px 7px;border-radius:999px;border:1px solid #e0b070;color:#e0b070;background:rgba(255,255,255,0.03)";
pub const BADGE_FAINT: &str = "font:600 10px ui-monospace, monospace;padding:2px 7px;border-radius:999px;border:1px solid #5a6675;color:#5a6675;background:rgba(255,255,255,0.03)";

// --- formatting -------------------------------------------------------------

fn hundredths(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn thousandths(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

pub fn format_years(y: f64) -> String {
    if y < 1000.0 {
        format!("{} years", y.round() as i64)
    } else if y < 1.0e6 {
        format!("{} kyr", hundredths(y / 1000.0))
    } else {
        format!("{} Myr", hundredths(y / 1.0e6))
    }
}

pub fn format_speed(v: f64) -> String {
    if v >= 1000.0 {
        format!("{} kyr / s", hundredths(v / 1000.0))
    } else {
        format!("{} yr / s", v.round() as i64)
    }
}

pub fn format_rate(mm_per_year: f64) -> String {
    if mm_per_year >= 10.0 {
        format!("{} mm / yr", hundredths(mm_per_year))
    } else if mm_per_year >= 0.01 {
        format!("{} mm / yr", thousandths(mm_per_year))
    } else {
        format!("{} µm / yr", thousandths(mm_per_year * 1000.0))
    }
}

pub fn format_seconds(s: f64) -> String {
    if s < 0.001 {
        format!("{} µs", thousandths(s * 1.0e6))
    } else if s < 1.0 {
        format!("{} ms", thousandths(s * 1000.0))
    } else if s < 120.0 {
        format!("{} s", hundredths(s))
    } else {
        format!("{} min", hundredths(s / 60.0))
    }
}

/// `sim.mass` is in kilograms (`deposited * RHO_CALCITE`).
pub fn format_mass(kilograms: f64) -> String {
    if kilograms < 1.0e-6 {
        format!("{} µg", thousandths(kilograms * 1.0e9))
    } else if kilograms < 1.0e-3 {
        format!("{} mg", hundredths(kilograms * 1.0e6))
    } else if kilograms < 1.0 {
        format!("{} g", hundredths(kilograms * 1000.0))
    } else {
        format!("{} kg", hundredths(kilograms))
    }
}

pub fn format_small(x: f64) -> String {
    if x == 0.0 {
        "0".to_string()
    } else if x.abs() < 0.001 || x.abs() >= 100000.0 {
        format!("{}e-3", thousandths(x * 1.0e3))
    } else {
        format!("{}", thousandths(x))
    }
}

pub fn format_rate_text(r: f64) -> String {
    if r < 0.001 {
        format!("{} nL / min", thousandths(r * 1.0e6))
    } else {
        format!("{} mL / min", thousandths(r))
    }
}

pub fn format_interval(s: f64) -> String {
    if s < 90.0 {
        format!("{} s", s.round() as i64)
    } else if s < 5400.0 {
        format!("{} min", hundredths(s / 60.0))
    } else {
        format!("{} h", hundredths(s / 3600.0))
    }
}

/// Unused alias kept so `rgba` is not flagged as dead code by the linter when the
/// drip colour is inlined elsewhere.
pub fn drip_colour(alpha: f64) -> String {
    rgba(158, 210, 242, alpha)
}
