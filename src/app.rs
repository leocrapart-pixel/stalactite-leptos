//! The application: state, controls and readouts.
//!
//! A Leptos port of the Elm app, and the same app as the Dioxus port next door.
//! The model in [`crate::physics`], the renderer in [`crate::canvas`] and the
//! scene in [`crate::scene`] are shared verbatim with the other ports and are
//! pinned to the Elm build by `tests/golden.rs`, so this file only has to get
//! the presentation right.
//!
//! Two things are worth knowing before reading it:
//!
//! * Leptos is fine-grained. A signal write re-runs the closures that read it,
//!   not the component, so every readout is a derived signal and the tree is
//!   built once. That is also why the wall clock is *not* a signal: at 60 Hz it
//!   would mark the whole UI dirty. It lives in an `Rc<Cell<f64>>` and reaches
//!   the canvas through the model, which changes every frame anyway.
//! * The animation loop is started once, from an `Effect` with no reactive
//!   reads, and stops itself when the component is cleaned up.

use leptos::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::canvas;
use crate::physics as p;
use crate::scene::SceneOptions;
use crate::theme as t;

// ---------------------------------------------------------------------------
// PRESETS
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Preset {
    name: &'static str,
    blurb: &'static str,
    env: p::Env,
    stalagmite_m: f64,
    chamber_m: f64,
}

fn preset(index: usize) -> Preset {
    match index {
        1 => Preset {
            name: "Starving drip",
            blurb: "An old cave with almost no supply: dense, very hard calcite, and a very long wait.",
            env: p::Env {
                temp_c: 11.0,
                cave_pco2: 5.0e-4,
                ca_in: 0.72,
                drip_rate: 3.0e-6,
                wetness: 0.15,
                kinetic_k: 5.0e-8,
                evaporation: 0.0,
                max_depth: 1.6,
            },
            stalagmite_m: 0.03,
            chamber_m: 1.6,
        },
        2 => Preset {
            name: "Well fed",
            blurb: "Plenty of water and lime. This one builds a metre in a few hundred thousand years.",
            env: p::Env {
                temp_c: 12.0,
                cave_pco2: 6.0e-4,
                ca_in: 1.6,
                drip_rate: 5.0e-4,
                wetness: 0.35,
                kinetic_k: 8.0e-8,
                evaporation: 0.0,
                max_depth: 1.6,
            },
            stalagmite_m: 0.1,
            chamber_m: 1.6,
        },
        3 => Preset {
            name: "Tropical cave",
            blurb: "Warm water holds less CO\u{2082} and calcite kinetics run faster, but the extra CO\u{2082} in the air works against it.",
            env: p::Env {
                temp_c: 22.0,
                cave_pco2: 9.0e-4,
                ca_in: 1.7,
                drip_rate: 1.5e-4,
                wetness: 0.3,
                kinetic_k: 6.0e-8,
                evaporation: 0.0,
                max_depth: 1.6,
            },
            stalagmite_m: 0.06,
            chamber_m: 1.6,
        },
        4 => Preset {
            name: "Drafty passage",
            blurb: "Dry air sweeps through, so evaporation deposits calcite alongside degassing.",
            env: p::Env {
                temp_c: 9.0,
                cave_pco2: 3.0e-4,
                ca_in: 1.1,
                drip_rate: 4.0e-5,
                wetness: 0.2,
                kinetic_k: 5.0e-8,
                evaporation: 0.6,
                max_depth: 1.6,
            },
            stalagmite_m: 0.04,
            chamber_m: 1.6,
        },
        5 => Preset {
            name: "Undersaturated",
            blurb: "Drip water poorer in lime than cave air allows. Nothing can be deposited at all.",
            env: p::Env {
                temp_c: 10.0,
                cave_pco2: 3.0e-3,
                ca_in: 0.7,
                drip_rate: 2.0e-5,
                wetness: 0.25,
                kinetic_k: 5.0e-8,
                evaporation: 0.0,
                max_depth: 1.6,
            },
            stalagmite_m: 0.0,
            chamber_m: 1.6,
        },
        _ => Preset {
            name: "Cave chamber",
            blurb: "A steady, well-fed drip: half a millimole of excess lime, one drop every 42 minutes.",
            env: p::Env {
                temp_c: 10.0,
                cave_pco2: 6.0e-4,
                ca_in: 1.0,
                drip_rate: 2.0e-5,
                wetness: 0.25,
                kinetic_k: 5.0e-8,
                evaporation: 0.0,
                max_depth: 1.6,
            },
            stalagmite_m: 0.05,
            chamber_m: 1.6,
        },
    }
}

const PRESET_COUNT: usize = 6;
const PRESET_NAMES: [&str; PRESET_COUNT] = [
    "Cave chamber",
    "Starving drip",
    "Well fed",
    "Tropical cave",
    "Drafty passage",
    "Undersaturated",
];

// ---------------------------------------------------------------------------
// FORMATTING
// ---------------------------------------------------------------------------

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn pct(x: f64) -> String {
    format!("{}%", x.clamp(0.0, 1.0) * 100.0)
}

/// The shared row-value style with an explicit colour. Merging these into one
/// attribute matters: a duplicate `style` is silently dropped by the parser.
fn row_value(colour: &str) -> String {
    format!("{};color:{}", t::ROW_VALUE, colour)
}

fn metres(x: f64) -> String {
    format!("{} m", round2(x))
}

fn wetness_text(x: f64) -> String {
    format!("{} % of circumference", (x * 100.0).round())
}

fn calcium_text(x: f64) -> String {
    format!("{} mmol / L", round2(x))
}

fn ppm_text(x: f64) -> String {
    format!("{} ppm", (x * 1.0e6).round())
}

fn temp_text(x: f64) -> String {
    format!("{} °C", round2(x))
}

fn k_text(x: f64) -> String {
    format!("{} nm / s", round3(x * 1.0e9))
}

fn percent_text(x: f64) -> String {
    format!("{} %", (x * 100.0).round())
}

/// A handle several closures can hold at once.
///
/// Cloning one is cheap — a pointer bump — and that is the only rule to
/// remember: every closure that captures a handle captures its own clone.
#[derive(Clone)]
struct Slot(Rc<Cell<f64>>);

impl Slot {
    fn new(value: f64) -> Self {
        Self(Rc::new(Cell::new(value)))
    }

    fn get(&self) -> f64 {
        self.0.get()
    }

    fn set(&self, value: f64) {
        self.0.set(value)
    }
}

/// The outstanding animation-frame id.
#[derive(Clone)]
struct FrameId(Rc<Cell<i32>>);

impl FrameId {
    fn new() -> Self {
        Self(Rc::new(Cell::new(-1)))
    }

    fn set(&self, value: i32) {
        self.0.set(value)
    }
}

/// Give a closure its own clones of the shared handles.
///
/// Deliberately shadows: the captured name is `last_sample`, and the local copy
/// is too, so the body reads normally. Cloning must happen *outside* the closure
/// that will take ownership, which is why every capture site is spelled out.
macro_rules! local {
    ($($name:ident => $copy:ident),+ $(,)?) => {
        $(let $copy = $name.clone();)+
    };
}

// ---------------------------------------------------------------------------
// THE MODEL
// ---------------------------------------------------------------------------

/// Slider limits. The cube and the exponent cannot be inverted by a range
/// input, so the two supply controls are mapped by hand.
const RATE_MIN: f64 = 5.0e-7;
const RATE_MAX: f64 = 2.0e-3;
const INTERVAL_MIN: f64 = 100.0;
const INTERVAL_MAX: f64 = 8.0e5;

/// Everything the cave is made of.
///
/// Not `Copy`: `Sim` owns the radius profile. Readouts therefore go through
/// `RwSignal::with` rather than `get`, which clones that array once per read —
/// 161 f64s, sixty times a second, for no reason.
#[derive(Clone)]
struct Model {
    env: p::Env,
    sim: p::Sim,
    speed: f64,
    chamber_m: f64,
    stalagmite_m: f64,
    scene_w: f64,
    scene_h: f64,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            env: preset(0).env,
            sim: p::Sim::initial(),
            speed: 80_000.0,
            chamber_m: preset(0).chamber_m,
            stalagmite_m: preset(0).stalagmite_m,
            // The scene is authored at a fixed aspect ratio; the height follows
            // from the measured width rather than from the measured height,
            // because deriving it from both would compound on every resize.
            scene_w: 620.0,
            scene_h: 720.0,
        }
    }
}

// ---------------------------------------------------------------------------
// SCENARIO <-> URL
// ---------------------------------------------------------------------------
//
// The whole scenario fits in a few numbers, so the hash carries it: a link
// reproduces the cave exactly, including how far the formation had grown.

const HASH_KEY: &str = "c";

fn encode(model: &Model, years: f64, running: bool, show_rings: bool) -> String {
    let e = &model.env;
    [
        e.temp_c,
        e.cave_pco2,
        e.ca_in,
        e.drip_rate,
        e.wetness,
        e.kinetic_k,
        e.evaporation,
        model.chamber_m,
        model.speed,
        years.round(),
        if running { 1.0 } else { 0.0 },
        if show_rings { 1.0 } else { 0.0 },
    ]
    .iter()
    .map(|v| v.to_string())
    .collect::<Vec<_>>()
    .join(",")
}

struct Scenario {
    model: Model,
    years: f64,
    running: bool,
    show_rings: bool,
}

fn decode(raw: &str) -> Option<Scenario> {
    let parts: Vec<f64> = raw
        .split(',')
        .map(|s| s.trim().parse::<f64>().unwrap_or(f64::NAN))
        .collect();
    if parts.len() != 12 || parts.iter().any(|n| !n.is_finite()) {
        return None;
    }
    Some(Scenario {
        model: Model {
            env: p::Env {
                temp_c: parts[0],
                cave_pco2: parts[1],
                ca_in: parts[2],
                drip_rate: parts[3],
                wetness: parts[4],
                kinetic_k: parts[5],
                evaporation: parts[6],
                max_depth: 1.6,
            },
            chamber_m: parts[7],
            speed: parts[8],
            ..Model::default()
        },
        years: parts[9],
        running: parts[10] != 0.0,
        show_rings: parts[11] != 0.0,
    })
}

fn current_hash() -> Option<String> {
    let win = web_sys::window()?;
    let raw = win.location().hash().ok()?;
    let hash = raw.strip_prefix('#').unwrap_or(&raw).to_string();
    if hash.is_empty() {
        return None;
    }
    web_sys::UrlSearchParams::new_with_str(&hash)
        .ok()?
        .get(HASH_KEY)
}

fn write_hash(payload: &str) {
    if let Some(win) = web_sys::window() {
        // Replacing rather than pushing keeps the back button useful instead of
        // filling history with every slider tick.
        let url = format!("#{HASH_KEY}={payload}");
        let _ = win.history().and_then(|h| {
            h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(url.as_str()))
        });
    }
}

// ---------------------------------------------------------------------------
// SMALL COMPONENTS
// ---------------------------------------------------------------------------

/// One labelled range control.
///
/// `text` is a callback rather than a string so the readout stays reactive: it
/// is re-evaluated whenever the props change, which is whenever the model does.
#[component]
fn Slider<F, T>(
    label: &'static str,
    value: Signal<f64>,
    min: f64,
    max: f64,
    step: f64,
    text: T,
    note: String,
    on_input: F,
) -> impl IntoView
where
    F: Fn(f64) + Copy + 'static,
    T: Fn(f64) -> String + Copy + Send + Sync + 'static,
{
    let shown = Signal::derive(move || text(value.get()));
    view! {
        <div style="display:flex;flex-direction:column;gap:5px">
            <div style="display:flex;justify-content:space-between;align-items:baseline;gap:8px">
                <span style=t::SLIDER_NAME>{label}</span>
                <span style=t::SLIDER_VALUE>{move || shown.get()}</span>
            </div>
            <input
                type="range"
                min=min.to_string()
                max=max.to_string()
                step=step.to_string()
                // `value` as a property rather than an attribute: an attribute
                // sets the default, and the control would then stop tracking the
                // model.
                prop:value=move || value.get().to_string()
                style=t::SLIDER_INPUT
                on:input=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                        on_input(v);
                    }
                }
            />
            <div style=t::HINT>{note}</div>
        </div>
    }
}

/// One readout row: a dim label and a bold right-aligned value.
#[component]
fn Row(label: &'static str, value: Signal<String>, colour: &'static str) -> impl IntoView {
    let style = row_value(colour);
    view! {
        <div style=t::ROW>
            <span style="color:#8b98a8">{label}</span>
            <span style=style>{move || value.get()}</span>
        </div>
    }
}

#[component]
fn Hint(text: Signal<String>) -> impl IntoView {
    view! { <div style=t::HINT>{move || text.get()}</div> }
}

#[component]
fn Heading(text: &'static str) -> impl IntoView {
    view! { <div style=t::SECTION_TITLE>{text}</div> }
}

// ---------------------------------------------------------------------------
// APP
// ---------------------------------------------------------------------------

#[component]
pub fn App() -> impl IntoView {
    let model = RwSignal::new(Model::default());
    let running = RwSignal::new(true);
    let show_rings = RwSignal::new(true);
    let preset_name = RwSignal::new("Cave chamber".to_string());
    let history = RwSignal::new(vec![(0.0_f64, p::SEED_RADIUS * 1000.0)]);

    // Wall-clock seconds, for the drip animation. Deliberately not a signal: at
    // 60 Hz it would mark the whole UI dirty.
    let clock = Slot::new(0.0);
    // Simulated-year watermark of the last growth-curve sample.
    let last_sample = Slot::new(0.0);

    // --- derived readouts -------------------------------------------------

    let view = Signal::derive(move || model.with(|m| p::view(&m.env, &m.sim)));
    let years = Signal::derive(move || model.with(|m| m.sim.years));

    let warning = Signal::derive(move || view.get().saturation_index <= 0.05);
    let _tip_colour = Signal::derive(move || if warning.get() { t::WARN } else { t::ACCENT }.to_string());
    let _si_colour = Signal::derive(move || if warning.get() { t::WARN } else { t::OK }.to_string());

    let tip_rate = Signal::derive(move || t::format_rate(view.get().tip_growth_mm_per_year));
    let wall_rate = Signal::derive(move || t::format_rate(view.get().wall_growth_mm_per_year));
    let length_cm = Signal::derive(move || format!("{} cm", round2(view.get().length * 100.0)));
    let slender = Signal::derive(move || {
        let v = view.get();
        format!(
            "{} mm per {} cm",
            round2(v.apex_radius * 2000.0),
            round2(v.length * 100.0)
        )
    });
    let tip_diameter = Signal::derive(move || format!("{}", round3(view.get().apex_radius * 2000.0)));
    let mass = Signal::derive(move || t::format_mass(view.get().mass_grams / 1000.0));
    let area = Signal::derive(move || format!("{}", round3(view.get().surface_area_m2 * 1.0e4)));
    let elapsed = Signal::derive(move || t::format_years(years.get()));

    let film = Signal::derive(move || format!("{}", round2(view.get().film_microns)));
    let film_speed = Signal::derive(move || format!("{}", round3(view.get().film_velocity_mm_s)));
    let perimeter = Signal::derive(move || format!("{}", round3(view.get().wetted_perimeter_mm)));
    let degas = Signal::derive(move || t::format_seconds(view.get().degas_seconds));
    let residence = Signal::derive(move || t::format_seconds(view.get().residence_seconds));
    let reynolds = Signal::derive(move || t::format_small(view.get().reynolds));

    let ca_in = Signal::derive(move || format!("{}", round2(view.get().ca_in)));
    let ca_eq = Signal::derive(move || format!("{}", round3(view.get().ca_eq)));
    let apex_excess = Signal::derive(move || format!("{}", round3(view.get().apex_excess)));
    let si = Signal::derive(move || format!("{}", round2(view.get().saturation_index)));
    let depletion = Signal::derive(move || format!("{}", round2(view.get().depletion_length_m)));
    let excess_bar = Signal::derive(move || pct(view.get().apex_excess / 5.0));

    let grown_hint = Signal::derive(move || {
        if warning.get() {
            "⚠ The drip water is at or below calcite equilibrium for this cave air, so nothing can precipitate. Raise the calcium or lower the CO₂.".to_string()
        } else {
            "The tip advances at γ times the wall thickening, because a fixed taper means the whole profile grows together.".to_string()
        }
    });
    let film_hint = Signal::derive(move || {
        let v = view.get();
        if v.degas_seconds < v.residence_seconds {
            "The film sheds its CO₂ long before it leaves the surface, so the water finishes the climb close to equilibrium.".to_string()
        } else {
            "The film is too thick to finish degassing: the water still carries excess CO₂ at the tip.".to_string()
        }
    });
    let chemistry_hint = Signal::derive(move || {
        let v = view.get();
        if v.depletion_length_m > v.length * 1.5 {
            "L is longer than the formation, so the film still carries excess calcium when it reaches the tip: the whole surface grows, and the tip runs ahead.".to_string()
        } else {
            "L is shorter than the formation, so most of the calcium is spent near the ceiling and the lower shaft starves.".to_string()
        }
    });

    // --- growth curve -----------------------------------------------------

    // A memo, so the bars only re-diff when the sample list actually changes
    // rather than on every animation frame.
    let bars = Memo::new(move |_| {
        let h = history.get();
        let y_max = h.iter().map(|point| point.1).fold(3.0_f64, f64::max);
        let len = h.len() as f64;
        h.iter()
            .enumerate()
            .map(|(i, point)| {
                let height = ((point.1 / y_max).clamp(0.0, 1.0) * 100.0).max(1.0);
                (height, 0.25 + 0.6 * ((i + 1) as f64 / len))
            })
            .collect::<Vec<(f64, f64)>>()
    });
    let chart_max = Signal::derive(move || {
        let y = history
            .get()
            .iter()
            .map(|point| point.1)
            .fold(3.0_f64, f64::max);
        round2(y)
    });

    // --- actions ----------------------------------------------------------

    // Plain closures, not `Callback`: `Callback` requires `Send + Sync`, and
    // these capture `RwSignal`s that are neither thread-safe nor need to be.
    local!(last_sample => sample_for_preset);
    let apply_preset: Rc<dyn Fn(usize)> = Rc::new(move |index: usize| {
        let pr = preset(index);
        model.update(|m| {
            m.env = pr.env;
            m.sim = p::Sim::initial();
            m.stalagmite_m = pr.stalagmite_m;
            m.chamber_m = pr.chamber_m;
        });
        history.set(vec![(0.0, p::SEED_RADIUS * 1000.0)]);
        sample_for_preset.set(0.0);
        preset_name.set(pr.name.to_string());
    });

    // Used by a button and by the `r` shortcut, so it captures only `Copy`
    // signal handles and is itself `Copy`.
    local!(last_sample => sample_for_restart);
    let restart: Rc<dyn Fn()> = Rc::new(move || {
        model.update(|m| m.sim = m.sim.reset());
        history.set(vec![(0.0, p::SEED_RADIUS * 1000.0)]);
        sample_for_restart.set(0.0);
    });

    // --- the animation loop ----------------------------------------------

    // The animation loop, started once.
    //
    // `Box<Closure>` rather than `Rc<RefCell<Option<Closure>>>`: boxing gives a
    // pointer the tick can move into its own captured copy without consuming the
    // variable, which is what keeps this closure `FnMut` — an `Effect` needs to
    // be re-runnable, and moving an `Rc` out of it would make it `FnOnce`.
    let _running_tick = running;
    // `Arc<AtomicI32>` rather than `Rc<Cell<i32>>`: `on_cleanup` requires
    // `Send + Sync`, and this is the one handle that has to cross into it.
    let handle_out = FrameId::new();
    // The same value as an atomic: `on_cleanup` requires `Send + Sync`, and the
    // `Rc` handle is neither, so this one exists purely for teardown.
    let handle_atomic = Arc::new(AtomicI32::new(-1));
    // The effect must not start a second loop if it is ever re-run.
    let started = Rc::new(Cell::new(false));

    {
        let history_sig = history;
        let model_sig = model;

        let running_tick = running;
        local!(handle_atomic => atomic_for_effect, clock => clock_for_effect);
        Effect::new(move |_| {
            let handle_atomic = &atomic_for_effect;
            let clock = &clock_for_effect;
            if started.get() {
                return;
            }
            started.set(true);

            // The tick reschedules itself through a handle it can borrow
            // mutably. `ScopedClosure` is neither `Clone` nor `Copy`, so the
            // borrow-and-return dance below is what lets one closure reschedule
            // itself from inside itself.
            let tick: Rc<RefCell<Option<Box<Closure<dyn FnMut(f64)>>>>> =
                Rc::new(RefCell::new(None));
            let inner = tick.clone();

            // The tick takes its own handles. This is a clone, not a move, and
            // that is what keeps this outer closure re-runnable.
            local!(
                clock => clock_for_tick,
                last_sample => sample_for_tick,
                handle_out => handle_for_tick,
                handle_atomic => atomic_for_tick,
            );
            // The closure captures its own clones by name; there is no
            // destructuring, because moving a captured value out would make this
            // `FnOnce` and a `FnMut` tick is the whole point.
            let cb = Box::new(Closure::<dyn FnMut(f64)>::new(move |now: f64| {
                let dt = ((now - clock_for_tick.get()) / 1000.0).clamp(0.0, 0.1);
                clock_for_tick.set(now);

                if running_tick.get_untracked() {
                    let slice = (model_sig.with_untracked(|m| m.speed) * dt).min(40_000.0);
                    if slice > 0.0 {
                        let next = model_sig.with_untracked(|m| {
                            p::advance(&p::Params::default(), &m.env, slice, &m.sim)
                        });
                        let grain = (next.years / 140.0).max(500.0);
                        if next.years - sample_for_tick.get() >= grain {
                            sample_for_tick.set(next.years);
                            let point = (
                                next.years,
                                p::profile_at_depth(p::length_of(&next), &next) * 1000.0,
                            );
                            history_sig.update(|h| {
                                h.push(point);
                                let cut = h.len().saturating_sub(140);
                                h.drain(..cut);
                            });
                        }
                        model_sig.update(|m| m.sim = next);
                    }
                }

                if let Some(win) = web_sys::window() {
                    if let Some(cb) = inner.borrow().as_ref() {
                        let func: &js_sys::Function = cb.as_js_value().unchecked_ref();
                        let id = win.request_animation_frame(func).unwrap_or(-1);
                        handle_for_tick.set(id);
                        atomic_for_tick.store(id, Ordering::Relaxed);
                    }
                }
            }));

            if let Some(win) = web_sys::window() {
                let func: &js_sys::Function = cb.as_js_value().unchecked_ref();
                let id = win.request_animation_frame(func).unwrap_or(-1);
                handle_out.set(id);
                handle_atomic.store(id, Ordering::Relaxed);
            }
            tick.borrow_mut().replace(cb);
            // The loop owns itself for the life of the page: dropping the
            // closure would leave a dangling callback behind.
            std::mem::forget(tick);
        });
    }

    // Stop the loop when this component goes away.
    // The frame id also exists as an atomic, because `on_cleanup` requires
    // `Send + Sync` and the `Rc` handle above is neither. The tick stores into
    // both; only this one is read at teardown.
    on_cleanup({
        let cleanup_handle = Arc::clone(&handle_atomic);
        move || {
            let h = cleanup_handle.load(Ordering::Relaxed);
            if h >= 0 {
                if let Some(win) = web_sys::window() {
                    let _ = win.cancel_animation_frame(h);
                }
            }
        }
    });

    // --- draw whenever the scene changes ----------------------------------

    {
        let model_sig = model;
        let show_rings_sig = show_rings;
        let clock = clock.clone();
        Effect::new(move |_| {
            // Reading the model and the view options is what subscribes this
            // effect; the clock is read untracked, so a frame that only advances
            // time repaints the canvas without touching any other closure.
            let rings = show_rings_sig.get();
            let time = clock.get();
            let drawn = model_sig.with(|m| {
                crate::scene::draw(&SceneOptions {
                    w: m.scene_w,
                    h: m.scene_h,
                    env: &m.env,
                    sim: &m.sim,
                    time,
                    chamber_m: m.chamber_m,
                    show_rings: rings,
                    stalagmite_m: m.stalagmite_m,
                })
            });
            if let Some(el) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("stage"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            {
                canvas::draw(&el, &drawn);
            }
        });
    }

    // --- keyboard shortcuts ----------------------------------------------
    {
        let model_sig = model;
        let running_sig = running;
        let rings_sig = show_rings;
        let restart_key = restart.clone();
        let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            // Never steal keys from a control the user is operating.
            if let Some(target) = ev.target().and_then(|x| x.dyn_into::<web_sys::Element>().ok()) {
                let tag = target.tag_name();
                if tag == "INPUT" || tag == "TEXTAREA" || tag == "SELECT" {
                    return;
                }
            }
            match ev.key().as_str() {
                " " | "k" => {
                    ev.prevent_default();
                    running_sig.update(|r| *r = !*r);
                }
                "r" => restart_key(),
                "y" => rings_sig.update(|r| *r = !*r),
                "ArrowRight" => {
                    ev.prevent_default();
                    model_sig.update(|m| m.speed = (m.speed * 1.5).min(2_000_000.0));
                }
                "ArrowLeft" => {
                    ev.prevent_default();
                    model_sig.update(|m| m.speed = (m.speed / 1.5).max(1000.0));
                }
                _ => {}
            }
        });
        if let Some(win) = web_sys::window() {
            let _ = win.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
        }
        cb.forget();
    }

    // --- follow the box width ---------------------------------------------
    {
        let model_sig = model;
        let cb = Closure::<dyn FnMut(js_sys::Array)>::new(move |_: js_sys::Array| {
            let width = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("stage"))
                .map(|el| el.get_bounding_client_rect().width())
                .unwrap_or(0.0);
            if width <= 0.0 {
                return;
            }
            let w = width.max(320.0).round();
            model_sig.update(|m| {
                if w == m.scene_w {
                    return;
                }
                let aspect = m.scene_h / m.scene_w;
                m.scene_w = w;
                m.scene_h = (w * aspect).round();
            });
        });
        if let Some(win) = web_sys::window() {
            if let Ok(ro) = web_sys::ResizeObserver::new(cb.as_ref().unchecked_ref()) {
                if let Some(el) = win.document().and_then(|d| d.get_element_by_id("stage")) {
                    ro.observe(&el);
                }
                // Kept alive for the life of the page; a dropped observer stops
                // firing.
                std::mem::forget(ro);
            }
        }
        cb.forget();
    }

    // --- read the URL once, on mount --------------------------------------
    {
        let model_sig = model;
        let running_sig = running;
        let rings_sig = show_rings;
        let preset_sig = preset_name;
        if let Some(scenario) = current_hash().as_deref().and_then(decode) {
            let restored = if scenario.years > 0.0 {
                p::advance(
                    &p::Params::default(),
                    &scenario.model.env,
                    scenario.years,
                    &p::Sim::initial(),
                )
            } else {
                p::Sim::initial()
            };
            model_sig.update(|m| {
                *m = scenario.model;
                m.sim = restored;
            });
            running_sig.set(scenario.running);
            rings_sig.set(scenario.show_rings);
            preset_sig.set("Custom".to_string());
        }
    }

    // --- write the scenario back to the URL -------------------------------
    Effect::new(move |_| {
        let payload = model.with(|m| encode(m, m.sim.years, running.get(), show_rings.get()));
        write_hash(&payload);
    });

    // --- slider callbacks -------------------------------------------------

    let rate_value = Signal::derive(move || {
        let rate = model.with(|m| m.env.drip_rate);
        ((rate.clamp(RATE_MIN, RATE_MAX) / RATE_MIN).cbrt() - 1.0).clamp(0.0, 1.0)
    });
    let interval_value = Signal::derive(move || {
        let interval = p::drip_interval_from_rate(model.with(|m| m.env.drip_rate));
        ((interval.clamp(INTERVAL_MIN, INTERVAL_MAX) / INTERVAL_MIN).log10()
            / (INTERVAL_MAX / INTERVAL_MIN).log10())
        .clamp(0.0, 1.0)
    });
    let interval_text = Signal::derive(move || {
        p::drip_interval_from_rate(model.with(|m| m.env.drip_rate))
    });
    let rate_note = Signal::derive(move || {
        model.with(|m| {
            format!(
                "one drop every {}  ·  {} L / yr",
                t::format_interval(p::drip_interval_from_rate(m.env.drip_rate)),
                round3(m.env.drip_rate * 525.6)
            )
        })
    });
    let speed_value = Signal::derive(move || model.with(|m| m.speed));
    let speed_text = Signal::derive(move || t::format_speed(model.with(|m| m.speed)));
    let chamber_value = Signal::derive(move || model.with(|m| m.chamber_m));

    let set_rate = move |x: f64| {
        model.update(|m| m.env.drip_rate = RATE_MIN * (1.0 + x.clamp(0.0, 1.0)).powi(3));
    };
    let set_interval = move |x: f64| {
        let seconds = INTERVAL_MIN * (INTERVAL_MAX / INTERVAL_MIN).powf(x.clamp(0.0, 1.0));
        model.update(|m| m.env.drip_rate = p::drip_rate_from_interval(seconds));
    };
    let set_speed = move |v: f64| model.update(|m| m.speed = v);
    let set_chamber = move |v: f64| model.update(|m| m.chamber_m = v);
    let set_wetness = move |v: f64| model.update(|m| m.env.wetness = v);
    let set_ca = move |v: f64| model.update(|m| m.env.ca_in = v);
    let set_pco2 = move |v: f64| model.update(|m| m.env.cave_pco2 = v);
    let set_temp = move |v: f64| model.update(|m| m.env.temp_c = v);
    let set_k = move |v: f64| model.update(|m| m.env.kinetic_k = v);
    let set_evap = move |v: f64| model.update(|m| m.env.evaporation = v);

    // --- markup -----------------------------------------------------------

    view! {
        <div style="min-height:100vh;background:#05070a;color:#e8eef6;font:13px ui-sans-serif, system-ui, sans-serif;display:flex;flex-direction:column;gap:12px;padding:16px 16px 28px;box-sizing:border-box">
            // header
            <div style="display:flex;justify-content:space-between;align-items:flex-end;gap:16px;flex-wrap:wrap;max-width:1600px;width:100%;margin:0 auto">
                <div style="display:flex;flex-direction:column;gap:3px">
                    <div style="font:600 20px ui-sans-serif, system-ui, sans-serif;letter-spacing:-0.015em">
                        "Stalactite"
                    </div>
                    <div style="font:11.5px ui-sans-serif, system-ui, sans-serif;color:#8b98a8">
                        "calcite growth solved from the calcium budget of a thin water film"
                    </div>
                </div>
                <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap">
                    <span style=t::BADGE_WATER>"elapsed "{move || elapsed.get()}</span>
                    <span style=t::BADGE_ACCENT>{move || preset_name.get()}</span>
                    <span style=t::BADGE_FAINT>"Leptos"</span>
                </div>
            </div>

            <div style="display:flex;gap:14px;align-items:flex-start;flex-wrap:wrap;justify-content:center">
                // ---- left column: controls ----
                <div style="width:312px;flex:0 0 312px;display:flex;flex-direction:column;gap:12px">
                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="Run" />
                            <div style="display:flex;flex-wrap:wrap;gap:6px">
                                <button
                                    style=move || if running.get() { t::BUTTON_ON } else { t::BUTTON_OFF }
                                    on:click=move |_| running.update(|r| *r = !*r)
                                >
                                    {move || if running.get() { "\u{275A}\u{275A}  Pause" } else { "\u{25B6}  Run" }}
                                </button>
                                <button
                                    style=t::BUTTON_OFF
                                    on:click={
                                        let restart = restart.clone();
                                        move |_| restart()
                                    }
                                >
                                    "\u{27F2}  New stalactite"
                                </button>
                                <button
                                    style=move || if show_rings.get() { t::BUTTON_ON } else { t::BUTTON_OFF }
                                    on:click=move |_| show_rings.update(|r| *r = !*r)
                                >
                                    "\u{2261}  Years"
                                </button>
                            </div>
                            <Slider
                                label="Time-lapse"
                                value=speed_value.into()
                                min=1000.0 max=2_000_000.0 step=1000.0
                                text=move |_| speed_text.get()
                                note="Simulated cave time per real second. A stalactite needs hundreds of thousands of years.".to_string()
                                on_input=set_speed
                            />
                            <Slider
                                label="Chamber height"
                                value=chamber_value.into()
                                min=0.4 max=1.6 step=0.05
                                text=metres
                                note="Ceiling to floor. When the tip reaches the floor it becomes a column.".to_string()
                                on_input=set_chamber
                            />
                        </div>
                    </div>

                    <div style="height:1px;background:#5a6675;opacity:0.4;margin:2px 0"></div>

                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="Water supply" />
                            <Slider
                                label="Drip rate"
                                value=rate_value.into()
                                min=0.0 max=1.0 step=0.005
                                text=move |_| t::format_rate_text(model.with(|m| m.env.drip_rate))
                                note=rate_note.get_untracked()
                                on_input=set_rate
                            />
                            <Slider
                                label="Drip interval"
                                value=interval_value.into()
                                min=0.02 max=1.0 step=0.005
                                text=move |_| t::format_interval(interval_text.get())
                                note="The same supply read as a rhythm. Real cave drips run from a second to a day.".to_string()
                                on_input=set_interval
                            />
                            <Slider
                                label="Surface wetness"
                                value=Signal::derive(move || model.with(|m| m.env.wetness)).into()
                                min=0.02 max=1.0 step=0.01
                                text=wetness_text
                                note="A starving stalactite carries a narrow rivulet; a well-fed one is sheathed.".to_string()
                                on_input=set_wetness
                            />
                        </div>
                    </div>
                </div>

                // ---- centre: the cave ----
                <div style="position:relative;flex:1 1 420px;min-width:320px;max-width:780px;border-radius:12px;overflow:hidden;border:1px solid #5a6675;background:#06080b">
                    <canvas id="stage" style="display:block;width:100%;height:auto"></canvas>
                    <div style="position:absolute;top:10px;left:14px;font:500 10px ui-monospace, monospace;letter-spacing:0.14em;text-transform:uppercase;color:#5a6675">
                        "cave chamber · cross-section"
                    </div>
                    <div style="position:absolute;top:10px;right:14px;font:600 10px ui-monospace, monospace;letter-spacing:0.1em;color:#8b98a8">
                        {move || if running.get() {
                            format!("\u{00D7}  {}", t::format_speed(model.with(|m| m.speed)))
                        } else {
                            "paused".to_string()
                        }}
                    </div>
                </div>

                // ---- right column: readouts ----
                <div style="width:330px;flex:0 0 330px;display:flex;flex-direction:column;gap:12px">
                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="Growth" />
                            <Row label="tip advance" value=tip_rate.into() colour=t::ACCENT />
                            <Row label="wall thickening" value=wall_rate.into() colour=t::INK />
                            <Row label="length" value=length_cm.into() colour=t::INK />
                            // Slenderness: the tip against the length. This is
                            // the number that makes "long and thin" concrete,
                            // and the shape the calcium budget supports.
                            <Row label="thickness ÷ length" value=slender.into() colour=t::DIM />
                            <Row label="tip diameter" value=tip_diameter.into() colour=t::INK />
                            <Row label="calcite deposited" value=mass.into() colour=t::INK />
                            <Row label="surface area" value=area.into() colour=t::DIM />
                            <Row label="elapsed" value=elapsed.into() colour=t::DIM />
                            <Hint text=grown_hint.into() />
                        </div>
                    </div>

                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="Water film" />
                            <Row label="thickness" value=film.into() colour=t::WATER />
                            <Row label="downslope speed" value=film_speed.into() colour=t::WATER />
                            <Row label="wetted perimeter" value=perimeter.into() colour=t::WATER />
                            <Row label="CO₂ degassing time" value=degas.into() colour=t::WATER />
                            <Row label="film residence time" value=residence.into() colour=t::WATER />
                            <Row label="Reynolds number" value=reynolds.into() colour=t::DIM />
                            <Hint text=film_hint.into() />
                        </div>
                    </div>

                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="Chemistry" />
                            <Row label="calcium arriving" value=ca_in.into() colour=t::OK />
                            <Row label="at equilibrium with cave air" value=ca_eq.into() colour=t::DIM />
                            <Row label="excess at the tip" value=apex_excess.into() colour=t::ACCENT />
                            <Row label="saturation index" value=si.into() colour=t::WATER />
                            <Row label="depletion length L = Q / kP" value=depletion.into() colour=t::ACCENT />
                            <div style="height:4px;border-radius:2px;background:rgba(255,255,255,0.07);overflow:hidden">
                                <div style=move || format!("width:{};height:100%;background:#e0b070;border-radius:2px", excess_bar.get())></div>
                            </div>
                            <Hint text=chemistry_hint.into() />
                            <div style=t::HINT>"SI = log₁₀(Ca / Ca_eq). Cave drip waters run from about 0.3 to 1.2."</div>
                        </div>
                    </div>

                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="Growth curve" />
                            <div style="position:relative;height:92px;border-left:1px solid #5a6675;border-bottom:1px solid #5a6675;margin-top:2px">
                                <div style="position:absolute;inset:0;display:flex;align-items:flex-end;gap:1px;padding:0 1px">
                                    {move || {
                                        bars.get()
                                            .into_iter()
                                            .map(|(height, alpha)| {
                                                view! {
                                                    <div style=format!("flex:1 1 0;border-radius:1px 1px 0 0;height:{height}%;background:rgba(224,176,112,{alpha})")></div>
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </div>
                                <div style="position:absolute;top:-2px;right:2px;font:600 9.5px ui-monospace, monospace;color:#5a6675">
                                    {move || format!("{} mm", chart_max.get())}
                                </div>
                            </div>
                            <div style=t::HINT>
                                "Tip diameter against elapsed time. The curve flattens as the widening tip spreads the same water over more surface."
                            </div>
                        </div>
                    </div>

                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="What the model solves" />
                            <div style=t::ROW><span style="color:#8b98a8">"supply"</span><span style=row_value(t::DIM)>"q from the drip rate"</span></div>
                            <div style=t::ROW><span style="color:#8b98a8">"film"</span><span style=row_value(t::DIM)>"Nusselt h ∝ (νq/gP)^⅓"</span></div>
                            <div style=t::ROW><span style="color:#8b98a8">"degassing"</span><span style=row_value(t::DIM)>"k = D/h²"</span></div>
                            <div style=t::ROW><span style="color:#8b98a8">"chemistry"</span><span style=row_value(t::DIM)>"carbonate system, bisected"</span></div>
                            <div style=t::ROW><span style="color:#8b98a8">"deposition"</span><span style=row_value(t::DIM)>"R = k(c − c_eq)"</span></div>
                            <div style=t::ROW><span style="color:#8b98a8">"shape"</span><span style=row_value(t::DIM)>"cone, L from deposited V"</span></div>
                            <div style=t::HINT>
                                "Equilibrium calcium is solved from the full carbonate system by geometric bisection. 160 nodes along the surface, 8 mm apart."
                            </div>
                        </div>
                    </div>

                    <div style=t::SHEET>
                        <div style=t::SECTION>
                            <Heading text="Water chemistry" />
                            <Slider
                                label="Calcium in drip water"
                                value=Signal::derive(move || model.with(|m| m.env.ca_in)).into()
                                min=0.2 max=4.0 step=0.01
                                text=calcium_text
                                note="Set by limestone dissolution along the flow path: usually 0.7–2.".to_string()
                                on_input=set_ca
                            />
                            <Slider
                                label="Cave air CO₂"
                                value=Signal::derive(move || model.with(|m| m.env.cave_pco2)).into()
                                min=2.0e-4 max=6.0e-3 step=2.5e-5
                                text=ppm_text
                                note="Drip water equilibrates to this, which fixes the calcium it cannot keep.".to_string()
                                on_input=set_pco2
                            />
                            <Slider
                                label="Temperature"
                                value=Signal::derive(move || model.with(|m| m.env.temp_c)).into()
                                min=0.0 max=30.0 step=0.5
                                text=temp_text
                                note="Calcite is less soluble in warm water, but so is CO₂.".to_string()
                                on_input=set_temp
                            />
                            <Slider
                                label="Deposition coefficient k"
                                value=Signal::derive(move || model.with(|m| m.env.kinetic_k)).into()
                                min=5.0e-9 max=5.0e-7 step=1.0e-9
                                text=k_text
                                note="Exchange velocity k. Literature values for calcite in thin films run from 0.4 to 10 mm/s; the effective rate over a whole stalactite is far smaller.".to_string()
                                on_input=set_k
                            />
                            <Slider
                                label="Evaporation"
                                value=Signal::derive(move || model.with(|m| m.env.evaporation)).into()
                                min=0.0 max=1.0 step=0.05
                                text=percent_text
                                note="Only in dry, draughty passages. A second, non-CO₂ growth path.".to_string()
                                on_input=set_evap
                            />
                        </div>
                    </div>
                </div>
            </div>

            // ---- full-width row: cave analogues ----
            <div style="max-width:1600px;width:100%;margin:0 auto">
                <div style=t::SHEET>
                    <div style=t::SECTION>
                        <Heading text="Cave analogues" />
                        <div style="display:flex;flex-wrap:wrap;gap:10px">
                            {(0..PRESET_COUNT)
                                .map(|i| {
                                    let name = PRESET_NAMES[i];
                                    // Each button takes its own handle on the
                                    // handler, so the outer closure stays `FnMut`.
                                    let apply_preset = apply_preset.clone();
                                    view! {
                                        <div style="flex:1 1 240px;display:flex;flex-direction:column;gap:4px">
                                            <button
                                                style=move || if preset_name.get() == name { t::BUTTON_ON } else { t::BUTTON_OFF }
                                                on:click=move |_| apply_preset(i)
                                            >
                                                {name}
                                            </button>
                                            <div style="font:10px ui-sans-serif, system-ui, sans-serif;color:#5a6675;line-height:1.45;padding-left:2px">
                                                {preset(i).blurb}
                                            </div>
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                        <div style="font:10px ui-sans-serif, system-ui, sans-serif;color:#5a6675;line-height:1.6;border-top:1px solid rgba(90,102,117,0.4);padding-top:8px;margin-top:4px">
                            <div>
                                <b style="color:#8b98a8">"space"</b>
                                " run / pause  "
                                <b style="color:#8b98a8">"r"</b>
                                " new stalactite"
                            </div>
                            <div>
                                <b style="color:#8b98a8">"y"</b>
                                " year bands  "
                                <b style="color:#8b98a8">"← →"</b>
                                " time-lapse speed"
                            </div>
                            <div style="margin-top:4px">"The URL carries the whole scenario — copy it to share a cave."</div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
