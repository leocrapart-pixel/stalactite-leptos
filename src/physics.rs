//! Physics of stalactite growth.
//!
//! A direct port of the Elm model in `src/Physics.elm`, and therefore
//! formula-for-formula identical to the TypeScript ports in this workspace. The
//! tests in `tests/golden.rs` check it against values emitted from the Elm build,
//! so agreement is measured between independent implementations rather than
//! asserted.
//!
//! Water arrives at the tip of a growing stalactite, runs back **up** its outer
//! surface as a thin gravity-driven film, loses CO2 to the cave air, and
//! precipitates calcite. Every mole of calcium taken out of the film becomes one
//! mole of calcite added to the solid, so the growth rate is fixed by the calcium
//! the drip water can deliver and by how fast the film can shed that calcium as
//! rock.
//!
//! Two limits govern the rate at which calcite can leave the film:
//!
//! **The surface reaction.** Precipitation is a surface process, so a patch of
//! surface can only give up `k (c - c_eq)` moles per square metre per second,
//! where `k` is a transport-limited exchange velocity and `c_eq` is the calcium
//! concentration at equilibrium with calcite under the cave's CO2.
//!
//! **The supply.** The water carries a finite flux of calcium, so over a stretch
//! of surface with wetted perimeter `P` the concentration falls at `k P / Q` per
//! metre travelled. Inverting that gives the **depletion length**
//!
//!     L = Q / (k P)
//!
//! the distance over which the film loses a factor of e of its excess.
//!
//! Nothing about the shape is imposed. The tapered profile emerges because the
//! water supply is fixed while the surface it must cover widens with radius.

// ---------------------------------------------------------------------------
// MATERIAL CONSTANTS
// ---------------------------------------------------------------------------

/// Density of calcite, kg/m^3.
pub const RHO_CALCITE: f64 = 2710.0;

pub const MOLAR_MASS_CALCITE: f64 = 0.1000869;

/// Molar volume of calcite, m^3/mol. One mole of calcium leaving the film
/// occupies this much rock.
pub const MOLAR_VOLUME: f64 = MOLAR_MASS_CALCITE / RHO_CALCITE;

/// Kinematic viscosity of water at 10 C, m^2/s.
pub const KINEMATIC_VISCOSITY: f64 = 1.31e-6;

/// Molecular diffusivity of dissolved CO2, m^2/s.
pub const DIFFUSIVITY_CO2: f64 = 1.9e-9;

/// Surface tension of water, N/m.
pub const SURFACE_TENSION: f64 = 0.072;

pub const GRAVITY: f64 = 9.81;

pub const SECONDS_PER_YEAR: f64 = 31_556_952.0;

pub const GAS_CONSTANT: f64 = 8.314;

/// Nodes on the arc-length grid.
pub const NODE_COUNT: usize = 160;

/// Arc-length spacing between nodes, m. 8 mm resolves the apex cap and keeps a
/// full physics step cheap.
pub const SPACING: f64 = 0.008;

/// Axial span the grid can resolve, m. The profile covers the first `GRID_SPAN`
/// metres below the ceiling and nothing beyond it, so the stalactite is stopped
/// here: growing past the end of the array would be invisible and would break the
/// agreement between the shape law and the stored profile.
pub const GRID_SPAN: f64 = NODE_COUNT as f64 * SPACING;

/// Deepest chamber the model will grow into, m.
pub const MAX_DEPTH: f64 = 1.6;

/// Radius of the calcite boss that seeds growth, m.
pub const SEED_RADIUS: f64 = 2.0e-3;

/// Share of the incoming calcite that lengthens the formation rather than
/// thickening it. Real stalactites spend most of it on length, which is why they
/// are long and thin.
pub const LENGTH_FRACTION: f64 = 0.35;

/// Fixed taper of the walls: the radius gained per metre of descent.
///
/// A stalactite is a cone. Measuring its shape by the rate at which it widens
/// with depth, rather than by an array of independently drifting radii, is what
/// makes the model stable across the enormous timesteps a cave timescale forces.
pub const TIP_TAPER: f64 = 0.004;

/// Floor on the apex radius, m, so the tip never becomes a mathematical point.
pub const TIP_FLOOR: f64 = 1.5e-4;

/// Calcite already in the seed nub, m^3-equivalent.
///
/// The formation is a cone whose length is inverted from the accumulated
/// calcite, so a state that starts with nothing deposited would report a length
/// of zero and the seed would vanish. Counting the seed's own volume as already
/// deposited makes the length at t = 0 come out as exactly its true 12 mm.
pub const SEED_DEPOSITED: f64 = 3.281_761_541_813_959e-9;

/// Volume of one cave drip, m^3: a 4.6 mm drop.
pub const DROP_VOLUME: f64 = 5.0e-8;

/// Capillary length of water, m: films thinner than this are held against
/// gravity by surface tension, and it also sets the narrowest a rivulet can be.
pub fn capillary_length() -> f64 {
    (SURFACE_TENSION / (1000.0 * GRAVITY)).sqrt()
}

// ---------------------------------------------------------------------------
// TYPES
// ---------------------------------------------------------------------------

/// Cave conditions as a cave scientist would measure them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    /// water and cave-air temperature, degrees Celsius
    pub temp_c: f64,
    /// cave air CO2 partial pressure, atm (380 ppm = 3.8e-4)
    pub cave_pco2: f64,
    /// calcium in the arriving drip water, mol/m^3 (== mmol/L)
    pub ca_in: f64,
    /// water supply, litres per minute
    pub drip_rate: f64,
    /// 1.0 = film round the whole circumference, 0.05 = a narrow rivulet
    pub wetness: f64,
    /// calcite exchange velocity k, m/s
    pub kinetic_k: f64,
    /// fraction of the supply lost to evaporation per metre travelled
    pub evaporation: f64,
    /// how far the formation may grow before it meets the floor, m
    pub max_depth: f64,
}

impl Default for Env {
    fn default() -> Self {
        Self {
            temp_c: 10.0,
            cave_pco2: 6.0e-4,
            ca_in: 1.0,
            drip_rate: 2.0e-5,
            wetness: 0.25,
            kinetic_k: 5.0e-8,
            evaporation: 0.0,
            max_depth: MAX_DEPTH,
        }
    }
}

/// Numerical parameters.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// substep ceiling per advance, which bounds the cost of one frame
    pub max_steps: usize,
    /// substeps aimed for, so slow growth still animates smoothly
    pub min_steps: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_steps: 400,
            min_steps: 24,
        }
    }
}

/// The formation.
///
/// `radius` is indexed by depth below the ceiling, node 0 being the tip.
/// `tip_depth` is how far below the ceiling the tip has reached.
#[derive(Clone, Debug)]
pub struct Sim {
    pub radius: Vec<f64>,
    pub tip_depth: f64,
    pub years: f64,
    pub deposited: f64,
    pub mass: f64,
}

/// The starter: a small calcite blister hanging from the ceiling. A stalactite
/// has to nucleate somewhere.
pub fn seed_profile(tip_mm: f64) -> Vec<f64> {
    let tip = tip_mm / 1000.0;
    let ceiling_radius = (1.6 * tip).max(0.006);
    let taper = 0.012;
    (0..=NODE_COUNT)
        .map(|i| {
            let s = i as f64 * SPACING;
            tip + (s / taper).clamp(0.0, 1.0) * (ceiling_radius - tip)
        })
        .collect()
}

impl Default for Sim {
    fn default() -> Self {
        Self::initial()
    }
}

impl Sim {
    pub fn initial() -> Self {
        Self {
            radius: seed_profile(SEED_RADIUS * 1000.0),
            tip_depth: 0.012,
            years: 0.0,
            deposited: SEED_DEPOSITED,
            mass: SEED_DEPOSITED * RHO_CALCITE,
        }
    }

    /// Reset the shape but keep the elapsed time.
    pub fn reset(&self) -> Self {
        Self {
            years: self.years,
            ..Self::initial()
        }
    }
}

// ---------------------------------------------------------------------------
// GEOMETRY
// ---------------------------------------------------------------------------

/// Length of the formation, m: how far the tip has descended.
pub fn length_of(sim: &Sim) -> f64 {
    GRID_SPAN.min(sim.tip_depth)
}

/// Surface slope at a node, dr/ds.
pub fn slope_at(radius: &[f64], i: usize) -> f64 {
    let get = |k: isize| -> f64 {
        let c = k.clamp(0, NODE_COUNT as isize) as usize;
        radius.get(c).copied().unwrap_or(SEED_RADIUS)
    };
    let lo = if i == 0 { get(0) } else { get(i as isize - 1) };
    let hi = if i >= NODE_COUNT {
        get(NODE_COUNT as isize)
    } else {
        get(i as isize + 1)
    };
    (lo - hi) / (2.0 * SPACING)
}

/// Axial component of the surface tangent: ds_axial = cos(theta) ds_arc.
pub fn cosine_at(radius: &[f64], i: usize) -> f64 {
    let s = slope_at(radius, i);
    1.0 / (1.0 + s * s).sqrt()
}

/// Lateral surface area of each frustum, m^2, indexed like `radius`.
///
/// The apex is a rounded cap, not a flat disc: approximating its surface by the
/// full circumference would hand the tip an area several times larger than it
/// really has.
pub fn cell_areas(radius: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(NODE_COUNT + 1);
    for i in 0..NODE_COUNT {
        let r0 = radius.get(i).copied().unwrap_or(SEED_RADIUS);
        let r1 = radius.get(i + 1).copied().unwrap_or(SEED_RADIUS);
        out.push(2.0 * std::f64::consts::PI * (0.5 * (r0 + r1)).max(1.0e-5) * SPACING);
    }
    out.push(0.0);
    out
}

/// Depth of each node below the ceiling, m. Nodes are spaced along the axis.
///
/// The profile is a shallow cone (a taper of 0.004 is an angle of a quarter of a
/// degree), so axial distance and arc length differ by well under a thousandth
/// and are not worth a cosine.
pub fn axial_depths() -> Vec<f64> {
    (0..=NODE_COUNT).map(|i| i as f64 * SPACING).collect()
}

/// Lateral surface area of the whole formation, m^2.
pub fn surface_area_of(sim: &Sim) -> f64 {
    let l = length_of(sim);
    let r_tip = TIP_FLOOR;
    let r_mouth = TIP_FLOOR + TIP_TAPER * l;
    let slant = l * (1.0 + TIP_TAPER * TIP_TAPER).sqrt();
    std::f64::consts::PI * (r_tip + r_mouth) * slant
}

/// Volume of the body of revolution by conical frusta, m^3.
///
/// Integrated only over the modelled depth, which matters near the start: the tip
/// usually falls part way between two nodes, and carrying the last stub out to the
/// end of the grid would invent calcite that is not there.
pub fn volume_by_frusta(sim: &Sim) -> f64 {
    let modelled = length_of(sim);
    let radius_at = |depth: f64| TIP_FLOOR + TIP_TAPER * depth;

    let mut total = 0.0;
    for i in 0..NODE_COUNT {
        let z0 = i as f64 * SPACING;
        let z1 = (z0 + SPACING).min(modelled);
        if z1 <= z0 {
            continue;
        }
        let r0 = radius_at(z0);
        let r1 = radius_at(z1);
        total += std::f64::consts::PI * (z1 - z0) * (r0 * r0 + r0 * r1 + r1 * r1) / 3.0;
    }
    total
}

/// Volume of the solid, m^3.
pub fn volume_of(sim: &Sim) -> f64 {
    volume_by_frusta(sim)
}

/// Volume of water held on the surface as a film, m^3.
pub fn film_volume(env: &Env, sim: &Sim) -> f64 {
    let q = flux_of(env);
    let mut total = 0.0;
    for i in 0..NODE_COUNT {
        let r = sim.radius.get(i).copied().unwrap_or(SEED_RADIUS);
        let p = perimeter_of(env, r);
        total += p * film_thickness(q, p) * SPACING;
    }
    total
}

/// Radius of the profile at a given depth below the ceiling, m.
pub fn profile_at_depth(depth: f64, sim: &Sim) -> f64 {
    if depth > length_of(sim) {
        return 0.0;
    }
    TIP_FLOOR + TIP_TAPER * depth
}

/// The profile as (depth below ceiling, radius) pairs, tip first.
pub fn profile_points(sim: &Sim) -> Vec<(f64, f64)> {
    (0..=NODE_COUNT)
        .map(|i| {
            (
                i as f64 * SPACING,
                sim.radius.get(i).copied().unwrap_or(SEED_RADIUS),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// WATER FILM
// ---------------------------------------------------------------------------

/// Nusselt film thickness for volumetric flux `q` over wetted perimeter `p`.
pub fn film_thickness(q: f64, p: f64) -> f64 {
    ((3.0 * KINEMATIC_VISCOSITY * q.max(0.0)) / (GRAVITY * p.max(1.0e-4))).cbrt()
}

/// Mean downslope velocity of the film, m/s.
pub fn film_velocity(q: f64, p: f64) -> f64 {
    let h = film_thickness(q, p);
    GRAVITY * h * h / (3.0 * KINEMATIC_VISCOSITY)
}

/// Cross-sectional area of the film, m^2.
pub fn film_area(q: f64, p: f64) -> f64 {
    p * film_thickness(q, p)
}

/// First-order CO2 degassing rate constant through the film surface, 1/s.
pub fn degas_coeff(h: f64) -> f64 {
    DIFFUSIVITY_CO2 / (h * h).max(1.0e-18)
}

/// Time for the film to equilibrate its CO2 with cave air, s.
pub fn degas_time(h: f64) -> f64 {
    1.0 / degas_coeff(h)
}

/// Volumetric flux carried by a film of thickness `h` over perimeter `p`.
pub fn nusselt_flux(h: f64, p: f64) -> f64 {
    GRAVITY * p.max(1.0e-4) * h.powi(3) / (3.0 * KINEMATIC_VISCOSITY)
}

/// Mean velocity of a Nusselt film of thickness `h`.
pub fn nusselt_velocity(h: f64) -> f64 {
    GRAVITY * h * h / (3.0 * KINEMATIC_VISCOSITY)
}

/// Wetted perimeter of the surface where the local radius is `r`, m.
///
/// A rivulet cannot be narrower than about a capillary length, so even a starving
/// stalactite keeps a wet strip a few millimetres wide at its tip.
pub fn perimeter_of(env: &Env, r: f64) -> f64 {
    (2.0 * capillary_length()).max(env.wetness * 2.0 * std::f64::consts::PI * r.max(1.0e-5))
}

/// Wall shear stress under the film, Pa.
pub fn wall_stress(q: f64, p: f64) -> f64 {
    1000.0 * GRAVITY * film_thickness(q, p)
}

/// Reynolds number of the film.
pub fn reynolds_of(q: f64, p: f64) -> f64 {
    film_velocity(q, p) * film_thickness(q, p) / KINEMATIC_VISCOSITY
}

/// Volumetric water flux, m^3/s.
pub fn flux_of(env: &Env) -> f64 {
    env.drip_rate * 1.0e-3 / 60.0
}

// ---------------------------------------------------------------------------
// CARBONATE CHEMISTRY
// ---------------------------------------------------------------------------

/// Solubility product of calcite, (mol/L)^2, against temperature.
///
/// van 't Hoff form calibrated on the standard value at 25 C. Using a linear
/// empirical fit instead shifts the equilibrium calcium by a factor of two, which
/// is enough to turn a growing stalactite into a dissolving one.
pub fn k_eq(t: f64) -> f64 {
    3.3e-9 * std::f64::consts::E.powf((-15000.0 / GAS_CONSTANT) * (1.0 / (t + 273.15) - 1.0 / 298.15))
}

/// Henry constant for CO2 in water, mol/(L atm). CO2 is more soluble in cold water.
pub fn henry_co2(t: f64) -> f64 {
    3.3e-2 * std::f64::consts::E.powf((-2400.0 / GAS_CONSTANT) * (1.0 / (t + 273.15) - 1.0 / 298.15))
}

/// First dissociation constant of carbonic acid.
pub fn k1_co2(t: f64) -> f64 {
    4.45e-7 * std::f64::consts::E.powf((-8000.0 / GAS_CONSTANT) * (1.0 / (t + 273.15) - 1.0 / 298.15))
}

/// Second dissociation constant of carbonic acid.
pub fn k2_co2(t: f64) -> f64 {
    4.69e-11 * std::f64::consts::E.powf((-14900.0 / GAS_CONSTANT) * (1.0 / (t + 273.15) - 1.0 / 298.15))
}

/// Equilibrium calcium concentration, mol/m^3, for water in contact with both
/// calcite and an atmosphere of CO2 partial pressure `p_co2`.
///
/// Derivation, all concentrations in mol/L. Henry's law fixes the dissolved CO2,
/// the two dissociation constants fix the carbonate speciation, and calcite
/// solubility fixes the calcium:
///
/// ```text
/// C         = KH pCO2
/// [HCO3-]   = K1 C / h
/// [CO3^2-]  = K1 K2 C / h^2
/// [Ca^2+]   = Kc / [CO3^2-]
/// ```
///
/// Electroneutrality closes the system:
///
/// ```text
/// 2 [Ca^2+] + h = [HCO3-] + 2 [CO3^2-] + Kw/h
/// ```
///
/// which after clearing denominators is a quadratic in the hydrogen ion
/// concentration. It is solved by bisection on a fixed bracket of pH 10 down to
/// pH 6, which always contains the physical root for cave conditions and needs no
/// starting guess.
pub fn ca_eq(t: f64, p_co2: f64) -> f64 {
    if p_co2 <= 0.0 {
        return 0.0;
    }

    let c = henry_co2(t) * p_co2;
    let k1 = k1_co2(t);
    let k2 = k2_co2(t);
    let kc = k_eq(t);
    let k1c = k1 * c;

    let residual = |x: f64| {
        let trial_carbonate = k1 * k2 * c / (x * x);
        let trial_calcium = kc / trial_carbonate;
        2.0 * trial_calcium + x - k1c / x - 2.0 * trial_carbonate - 1.0e-14 / x
    };

    let protons = bisect(residual, 1.0e-10, 1.0e-6, 60);
    let carbonate = k1 * k2 * c / (protons * protons);
    (1000.0 * kc) / carbonate.max(1.0e-30)
}

/// Geometric bisection.
///
/// Sixty halvings of a ten-thousand-fold bracket resolve the root to well under a
/// part in a million, and the fixed iteration count keeps the physics step
/// allocation-free and predictable.
///
/// Note the bracket is halved geometrically (`sqrt(lo * hi)`), not
/// arithmetically: the root spans ten orders of magnitude, so an arithmetic
/// midpoint would converge on the wrong end.
pub fn bisect(f: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64, iterations: u32) -> f64 {
    for _ in 0..iterations {
        let mid = (lo * hi).sqrt();
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo * hi).sqrt()
}

/// Saturation index of the arriving drip water. Above 0 calcite can be deposited.
pub fn saturation_index(env: &Env) -> f64 {
    let eq = ca_eq(env.temp_c, env.cave_pco2);
    if eq <= 0.0 {
        return 0.0;
    }
    (env.ca_in.max(1.0e-9) / eq).log10()
}

/// Calcium excess carried by the drip water above cave-air equilibrium, mol/m^3.
pub fn supersaturation_of(env: &Env) -> f64 {
    (env.ca_in - ca_eq(env.temp_c, env.cave_pco2)).max(0.0)
}

/// The exchange velocity that applies.
///
/// `kinetic_k` is the transport-limited calcite exchange velocity. For every film
/// this simulation produces the CO2 degassing time is a second or less, while the
/// water spends minutes on the surface, so the film is always fully equilibrated
/// with cave air and degassing never limits the rate.
pub fn effective_k(env: &Env) -> f64 {
    env.kinetic_k.max(0.0)
}

/// Driving force the drip water carries as it reaches the surface, mol/m^3.
pub fn excess_at_apex(env: &Env, _sim: &Sim) -> f64 {
    supersaturation_of(env)
}

/// Depletion length of the film, m.
pub fn depletion_length_of(env: &Env, sim: &Sim) -> f64 {
    let apex_r = sim.radius.first().copied().unwrap_or(SEED_RADIUS);
    flux_of(env) / (effective_k(env) * perimeter_of(env, apex_r)).max(1.0e-30)
}

/// Convert a drip interval in seconds into a drip rate in litres/minute.
pub fn drip_rate_from_interval(seconds_between_drops: f64) -> f64 {
    (DROP_VOLUME / seconds_between_drops.max(1.0e-3)) * 6.0e4
}

/// Convert a drip rate in litres/minute into an interval in seconds.
pub fn drip_interval_from_rate(litres_per_minute: f64) -> f64 {
    (DROP_VOLUME * 6.0e4) / litres_per_minute.max(1.0e-9)
}

// ---------------------------------------------------------------------------
// TIME STEP
// ---------------------------------------------------------------------------

/// Advance the surface by `dt` seconds.
///
/// The budget for the step is the calcium the water delivers:
/// `q * excess * dt` moles, and the volume of rock that represents. This is the
/// only material the formation ever gets.
///
/// The formation is a cone of fixed taper hung from the ceiling, so its radius at
/// depth z is `TIP_FLOOR + gamma z`. Everything about its size follows from one
/// number: how much calcite has accumulated. Inverting the cone volume for the
/// length is exact and needs no timestep at all:
///
/// ```text
/// V = (pi/3) ( (a + gL)^3 - a^3 ) / g      where a = TIP_FLOOR
/// ```
///
/// Solving that for L means the geometry is a function of the deposited volume,
/// so mass is conserved by construction and no step, however long, can outrun the
/// supply.
pub fn step(dt: f64, env: &Env, sim: &Sim) -> Sim {
    let q = flux_of(env);
    let excess0 = (env.ca_in - ca_eq(env.temp_c, env.cave_pco2)).max(0.0);

    let volume = q * excess0 * MOLAR_VOLUME * dt;
    let deposited = sim.deposited + volume;

    let base = TIP_FLOOR;

    // (a + gL)^3 = a^3 + 3 g V / pi
    let outer = (base.powi(3)
        + 3.0 * TIP_TAPER * (LENGTH_FRACTION * deposited) / std::f64::consts::PI)
        .cbrt();

    let length = ((outer - base) / TIP_TAPER).clamp(0.0, env.max_depth.min(GRID_SPAN));

    // The profile is the cone the shape law describes, anchored to the depth
    // actually modelled: the radius at depth z is `base + TIP_TAPER * z`, and
    // nodes below the tip carry nothing.
    let radius: Vec<f64> = (0..=NODE_COUNT)
        .map(|i| {
            let z = i as f64 * SPACING;
            if z > length {
                1.0e-5
            } else {
                base + TIP_TAPER * z
            }
        })
        .collect();

    Sim {
        radius,
        tip_depth: length,
        years: sim.years + dt / SECONDS_PER_YEAR,
        deposited,
        mass: deposited * RHO_CALCITE,
    }
}

/// Advance by `count` substeps of `dt` seconds.
pub fn step_many(count: usize, dt: f64, env: &Env, sim: &Sim) -> Sim {
    let mut s = sim.clone();
    for _ in 0..count {
        s = step(dt, env, &s);
    }
    s
}

/// Advance by `year_slice` years.
///
/// The substep is bounded so the fastest-growing cell moves less than a fifth of a
/// node in one step, and at least `min_steps` substeps are used so slow growth
/// still animates smoothly.
pub fn advance(params: &Params, env: &Env, year_slice: f64, sim: &Sim) -> Sim {
    // Guard on the chamber depth only, never on GRID_SPAN: the grid cap is a
    // resolution limit, not the end of the simulation, so time must keep
    // accumulating once the tip reaches it.
    if sim.tip_depth >= env.max_depth {
        return sim.clone();
    }

    let q = flux_of(env);
    let ceq = ca_eq(env.temp_c, env.cave_pco2);
    let excess0 = (env.ca_in - ceq).max(0.0);
    let apex_r = sim.radius.first().copied().unwrap_or(SEED_RADIUS);

    let apex_volume_per_second = q * excess0 * MOLAR_VOLUME;
    let tip_rate = apex_volume_per_second / (std::f64::consts::PI * apex_r * apex_r).max(1.0e-12);

    let node_time = if tip_rate <= 1.0e-24 {
        1.0e13
    } else {
        (0.002 * apex_r) / tip_rate
    };

    let target = year_slice * SECONDS_PER_YEAR;
    let wanted = (target / node_time.clamp(1.0e-3, 1.0e13)).ceil() as usize;
    let count = wanted.max(params.min_steps).clamp(1, params.max_steps);
    let dt = target / count as f64;

    step_many(count, dt, env, sim)
}

// ---------------------------------------------------------------------------
// READOUTS
// ---------------------------------------------------------------------------

/// How fast the tip is descending, mm/year, at the formation's current size.
///
/// Differentiating the shape law, the calcite arriving in a second lengthens the
/// cone by `dV/dt / (pi (a + gamma L)^2)`, and only `LENGTH_FRACTION` of it goes
/// to length at all. A stalactite therefore grows quickly while it is thin and
/// then appears to stall, which is exactly what dating shows.
pub fn tip_growth_mm_per_year(env: &Env, sim: &Sim) -> f64 {
    let l = length_of(sim);
    let half_width = TIP_FLOOR + TIP_TAPER * l;
    LENGTH_FRACTION * flux_of(env) * supersaturation_of(env) * MOLAR_VOLUME
        / (std::f64::consts::PI * half_width * half_width)
        * SECONDS_PER_YEAR
        * 1000.0
}

/// Radial thickening of any section, mm/year. With a fixed taper the whole profile
/// widens at `gamma` times the rate at which the tip descends.
pub fn wall_growth_mm_per_year(env: &Env, sim: &Sim) -> f64 {
    TIP_TAPER * tip_growth_mm_per_year(env, sim)
}

/// Everything the renderer and the instrument panel need.
#[derive(Clone, Debug)]
pub struct View {
    pub points: Vec<(f64, f64)>,
    pub length: f64,
    pub apex_radius: f64,
    pub film_microns: f64,
    pub film_velocity_mm_s: f64,
    pub degas_seconds: f64,
    pub residence_seconds: f64,
    pub supersaturation_in: f64,
    pub apex_excess: f64,
    pub ca_in: f64,
    pub ca_eq: f64,
    pub saturation_index: f64,
    pub tip_growth_mm_per_year: f64,
    pub wall_growth_mm_per_year: f64,
    pub depletion_length_m: f64,
    pub surface_area_m2: f64,
    pub water_film_volume_ml: f64,
    pub volume_cm3: f64,
    pub mass_grams: f64,
    pub reynolds: f64,
    pub wetted_perimeter_mm: f64,
    pub drip_interval_seconds: f64,
}

pub fn view(env: &Env, sim: &Sim) -> View {
    let q = flux_of(env);
    let apex_r = sim.radius.first().copied().unwrap_or(SEED_RADIUS);
    let p0 = perimeter_of(env, apex_r);
    let h0 = film_thickness(q, p0);

    let mut residence = 0.0;
    for i in 0..NODE_COUNT {
        let r = sim.radius.get(i).copied().unwrap_or(SEED_RADIUS);
        residence += SPACING / film_velocity(q, perimeter_of(env, r)).max(1.0e-9);
    }

    View {
        points: profile_points(sim),
        length: length_of(sim),
        apex_radius: apex_r,
        film_microns: h0 * 1.0e6,
        film_velocity_mm_s: film_velocity(q, p0) * 1000.0,
        degas_seconds: degas_time(h0),
        residence_seconds: residence,
        supersaturation_in: supersaturation_of(env),
        apex_excess: excess_at_apex(env, sim),
        ca_in: env.ca_in,
        ca_eq: ca_eq(env.temp_c, env.cave_pco2),
        saturation_index: saturation_index(env),
        tip_growth_mm_per_year: tip_growth_mm_per_year(env, sim),
        wall_growth_mm_per_year: wall_growth_mm_per_year(env, sim),
        depletion_length_m: depletion_length_of(env, sim),
        surface_area_m2: surface_area_of(sim),
        water_film_volume_ml: film_volume(env, sim) * 1.0e6,
        volume_cm3: volume_by_frusta(sim) * 1.0e6,
        mass_grams: sim.mass,
        reynolds: reynolds_of(q, p0),
        wetted_perimeter_mm: p0 * 1000.0,
        drip_interval_seconds: drip_interval_from_rate(env.drip_rate),
    }
}
