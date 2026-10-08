//! Stalactite — shared modules.
//!
//! `physics`, `canvas` and `scene` are deliberately free of any Leptos or DOM
//! dependency, so the model and the renderer can be unit-tested natively
//! without a browser.

pub mod app;
pub mod canvas;
pub mod physics;
pub mod scene;
pub mod theme;

pub use app::App;

/// Entry point, called by the page once the module has been instantiated.
///
/// An explicit export rather than `#[wasm_bindgen(start)]`: this crate is both a
/// `cdylib` (for the browser) and an `rlib` (so `tests/golden.rs` can link the
/// model), and with both crate types present wasm-bindgen does not emit the
/// start function — the module instantiates and mounts nothing.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start() {
    leptos::mount::mount_to_body(App);
}

#[cfg(test)]
mod parity {
    //! The parity suite, kept inside the library.
    //!
    //! `crate-type = ["cdylib"]` keeps the wasm module about 13 kB gzipped
    //! smaller than `["cdylib", "rlib"]` does, but an integration test in
    //! `tests/` cannot link a `cdylib`. A unit-test module can, and it has the
    //! same access to the model.

    use crate::physics as p;

    const ENV: p::Env = p::Env {
        temp_c: 10.0,
        cave_pco2: 6.0e-4,
        ca_in: 1.0,
        drip_rate: 2.0e-5,
        wetness: 0.25,
        kinetic_k: 5.0e-8,
        evaporation: 0.0,
        max_depth: 1.6,
    };

    /// Floats are compared relatively: these are physics results carried through
    /// `exp` and `cbrt`, so bit-exactness is not a fair bar, but anything above one
    /// part in 1e12 means the formulas differ rather than the arithmetic.
    #[track_caller]
    fn near(actual: f64, expected: f64, label: &str) {
        let scale = expected.abs().max(1e-300);
        let err = (actual - expected).abs() / scale;
        assert!(
            err <= 1e-12,
            "{label}: expected {expected}, got {actual} (relative {err:e})"
        );
    }

    #[test]
    fn constants_match() {
        near(p::MOLAR_VOLUME, 3.6932435424354245e-05, "molarVolume");
        near(
            p::capillary_length(),
            0.0027091418459143854,
            "capillaryLength",
        );
        near(p::SEED_RADIUS, 0.002, "seedRadius");
    }

    #[test]
    fn film_family_matches() {
        near(
            p::film_thickness(1.0e-9, 0.005),
            4.311064423428694e-05,
            "filmThickness",
        );
        near(
            p::film_velocity(1.0e-9, 0.005),
            0.004639225498767553,
            "filmVelocity",
        );
        near(p::degas_coeff(1.0e-4), 0.19, "degasCoeff");
        near(p::degas_time(1.0e-4), 5.2631578947368425, "degasTime");
        near(
            p::perimeter_of(&ENV, 0.002),
            0.005418283691828771,
            "perimeterOf",
        );
        near(p::reynolds_of(1.0e-9, 0.005), 0.15267175572519112, "reynolds");
        near(p::flux_of(&ENV), 3.333333333333333e-10, "fluxOf");
    }

    #[test]
    fn chemistry_matches() {
        near(p::k_eq(10.0), 2.394929434587765e-09, "kEq(10)");
        near(p::k_eq(25.0), 3.3e-09, "kEq(25)");
        near(p::henry_co2(10.0), 0.03135007217149941, "henryCO2(10)");
        near(p::k1_co2(10.0), 3.750668616971175e-07, "k1CO2(10)");
        near(p::k2_co2(10.0), 3.410984657418793e-11, "k2CO2(10)");

        // The bisection is the delicate part: a wrong bracket or a non-geometric
        // midpoint still converges, just to a different root.
        near(p::ca_eq(10.0, 6.0e-4), 0.502098364789946, "caEq(10, 600ppm)");
        near(p::ca_eq(10.0, 4.0e-4), 0.43968569480768954, "caEq(10, 400ppm)");
        near(p::ca_eq(10.0, 3.0e-3), 0.8543527698576954, "caEq(10, 3000ppm)");
        near(p::ca_eq(0.0, 6.0e-4), 0.47522605973758325, "caEq(0, 600ppm)");
        near(p::ca_eq(20.0, 6.0e-4), 0.5285606890701612, "caEq(20, 600ppm)");
        near(p::ca_eq(22.0, 9.0e-4), 0.609843333348286, "caEq(22, 900ppm)");
        near(
            p::ca_eq(25.0, 4.0e-4),
            0.47442966088716193,
            "caEq(25, 400ppm)",
        );

        near(p::saturation_index(&ENV), 0.29921119301294574, "saturationIndex");
        near(
            p::supersaturation_of(&ENV),
            0.497901635210054,
            "supersaturationOf",
        );
    }

    #[test]
    fn geometry_matches() {
        let s1 = p::advance(&p::Params::default(), &ENV, 1000.0, &p::Sim::initial());
        near(p::surface_area_of(&s1), 0.021795287553752662, "surfaceAreaOf");
        near(p::volume_of(&s1), 3.8316908132801065e-5, "volumeOf");
        near(
            p::film_volume(&ENV, &s1),
            2.1361557314955797e-7,
            "filmVolume",
        );
        near(p::length_of(&s1), 1.28, "lengthOf");
    }

    #[test]
    fn integration_matches() {
        let params = p::Params::default();
        let steps = [
            p::Sim::initial(),
            p::advance(&params, &ENV, 1000.0, &p::Sim::initial()),
            p::advance(&params, &ENV, 50000.0, &p::Sim::initial()),
            p::advance(&params, &ENV, 1_000_000.0, &p::Sim::initial()),
        ];

        // (years, tipDepth, deposited, mass, radiusAtTip, tipGrowth, depletion)
        let expected = [
            (
                0.0,
                0.012,
                3.281761541813959e-9,
                8.89357377831583e-6,
                1.98e-4,
                549.6839867402391,
                1.2304019216861168,
            ),
            (
                1000.0,
                1.28,
                1.9343393311762743e-4,
                0.5242059587487703,
                5.27e-3,
                0.7759294497932996,
                1.2304019216861168,
            ),
            (
                50000.0,
                1.28,
                9.671535849565913e-3,
                26.209862152323623,
                5.27e-3,
                0.7759294497932996,
                1.2304019216861168,
            ),
            (
                1_000_000.0,
                1.28,
                0.19343065463784745,
                524.1970740685666,
                5.27e-3,
                0.7759294497932996,
                1.2304019216861168,
            ),
        ];

        for (i, (sim, e)) in steps.iter().zip(expected.iter()).enumerate() {
            near(sim.years, e.0, &format!("steps[{i}].years"));
            near(sim.tip_depth, e.1, &format!("steps[{i}].tipDepth"));
            near(sim.deposited, e.2, &format!("steps[{i}].deposited"));
            near(sim.mass, e.3, &format!("steps[{i}].mass"));
            near(
                p::profile_at_depth(sim.tip_depth, sim),
                e.4,
                &format!("steps[{i}].radiusAtTip"),
            );
            near(
                p::tip_growth_mm_per_year(&ENV, sim),
                e.5,
                &format!("steps[{i}].tipGrowth"),
            );
            near(
                p::depletion_length_of(&ENV, sim),
                e.6,
                &format!("steps[{i}].depletion"),
            );
        }
    }

    #[test]
    fn animation_loop_matches() {
        // Reproduce the app's per-frame call: 800 kyr/s at 10 ms per frame, so each
        // frame advances 800 years.
        let run = |n: usize| {
            let mut s = p::Sim::initial();
            for _ in 0..n {
                s = p::advance(&p::Params::default(), &ENV, 800.0, &s);
            }
            s
        };

        let expected = [
            (800.0, 1.5474780284641118e-4, 1.28, 0.4193665457137743),
            (8000.0, 1.5474484926101635e-3, 1.28, 4.193585414973543),
            (48000.0, 9.284674546855477e-3, 1.28, 25.161468021978344),
            (240000.0, 0.04642335960729838, 1.28, 125.80730453577861),
        ];

        for (n, e) in [1usize, 10, 60, 300].iter().zip(expected.iter()) {
            let s = run(*n);
            near(s.years, e.0, &format!("loop[{n}].years"));
            near(s.deposited, e.1, &format!("loop[{n}].deposited"));
            near(s.tip_depth, e.2, &format!("loop[{n}].tipDepth"));
            near(s.mass, e.3, &format!("loop[{n}].mass"));
        }
    }

    #[test]
    fn view_matches() {
        let s1 = p::advance(&p::Params::default(), &ENV, 1000.0, &p::Sim::initial());
        let v = p::view(&ENV, &s1);

        near(v.film_microns, 29.101375451652636, "view.filmMicrons");
        near(v.film_velocity_mm_s, 2.1139927281620046, "view.filmVelocityMmS");
        near(v.residence_seconds, 640.8467194486728, "view.residenceSeconds");
        near(v.reynolds, 0.046961905407867134, "view.reynolds");
        near(v.wetted_perimeter_mm, 5.418283691828771, "view.wettedPerimeterMm");
        near(v.depletion_length_m, 1.2304019216861168, "view.depletionLengthM");
        near(v.ca_eq, 0.502098364789946, "view.caEq");
        near(v.saturation_index, 0.29921119301294574, "view.saturationIndex");
        near(
            v.water_film_volume_ml,
            0.21361557314955798,
            "view.waterFilmVolumeMl",
        );
        near(v.volume_cm3, 38.31690813280107, "view.volumeCm3");
        near(v.mass_grams, 0.5242059587487703, "view.massGrams");
    }

    #[test]
    fn conservation_holds() {
        let params = p::Params::default();

        // The solid can never hold more calcite than the water delivered.
        for years in [1000.0, 5000.0, 20000.0, 400000.0] {
            let sim = p::advance(&params, &ENV, years, &p::Sim::initial());
            let ratio = p::volume_of(&sim) / sim.deposited;
            assert!(
                ratio <= 1.0 + 1e-6,
                "solid volume exceeds delivered calcite at {years} yr: {ratio}"
            );
        }

        // The stored profile and the closed-form shape law describe the same cone.
        let a = p::TIP_FLOOR;
        let g = p::TIP_TAPER;
        for years in [100.0, 1000.0, 10000.0] {
            let sim = p::advance(&params, &ENV, years, &p::Sim::initial());
            let modelled = p::length_of(&sim);
            let closed = std::f64::consts::PI / 3.0 * ((a + g * modelled).powi(3) - a.powi(3)) / g;
            let rel = (p::volume_of(&sim) / closed - 1.0).abs();
            assert!(rel < 1e-9, "profile vs shape law at {years} yr: {rel:e}");
        }

        // Length is monotonic and never exceeds the chamber.
        let mut prev = 0.0;
        for years in [1000.0, 10000.0, 100000.0, 400000.0] {
            let sim = p::advance(&params, &ENV, years, &p::Sim::initial());
            assert!(sim.tip_depth >= prev, "length went backwards at {years}");
            assert!(sim.tip_depth <= ENV.max_depth + 1e-12, "left the chamber");
            prev = sim.tip_depth;
        }
    }

    #[test]
    fn undersaturated_deposits_nothing() {
        let dry = p::Env {
            cave_pco2: 3.0e-3,
            ca_in: 0.7,
            ..ENV
        };
        let sim = p::advance(&p::Params::default(), &dry, 1.0e6, &p::Sim::initial());

        // The seed's own calcite is carried in `deposited`, so nothing may be added.
        near(sim.deposited, p::SEED_DEPOSITED, "undersaturated deposited");
        near(sim.tip_depth, 0.012, "undersaturated did not grow");
    }

    #[test]
    fn scene_primitive_counts_match() {
        use crate::canvas as c;
        use crate::scene as sc;

        let sim = p::advance(&p::Params::default(), &ENV, 1000.0, &p::Sim::initial());
        let scene = sc::draw(&sc::SceneOptions {
            w: 620.0,
            h: 720.0,
            env: &ENV,
            sim: &sim,
            time: 1.5,
            chamber_m: 1.6,
            show_rings: true,
            stalagmite_m: 0.05,
        });

        let counts = c::count_shapes(&scene.shapes);
        // Same numbers as the Elm and TypeScript builds, which is a strong signal the
        // scene graph is structurally identical.
        assert_eq!(counts.get("rect"), Some(&3), "rects: {counts:?}");
        assert_eq!(counts.get("group"), Some(&5), "groups: {counts:?}");
        assert_eq!(counts.get("seg"), Some(&32), "segments: {counts:?}");
        assert_eq!(counts.get("poly"), Some(&6), "polys: {counts:?}");
        assert_eq!(counts.get("ellipse"), Some(&8), "ellipses: {counts:?}");
        assert_eq!(counts.get("label"), Some(&2), "labels: {counts:?}");
    }
}
