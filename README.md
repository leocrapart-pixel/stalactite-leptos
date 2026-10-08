# Stalactite — Leptos

A Leptos port of the Elm app in the parent directory. Same physics, same scene,
same instrument panel; the model is reproduced formula-for-formula and pinned to
the Elm build by a parity test.

One of six implementations of the same thing, alongside
[Elm](https://github.com/leocrapart-pixel/stalactite),
[SvelteKit](https://github.com/leocrapart-pixel/stalactite-svelte),
[Angular](https://github.com/leocrapart-pixel/stalactite-angular),
[Vue](https://github.com/leocrapart-pixel/stalactite-vue) and
[Dioxus](https://github.com/leocrapart-pixel/stalactite-dioxus).

Live: <https://stalactite-leptos.stalactite.workers.dev>

## Size

**120.6 kB gzipped** (112.0 kB wasm + 7.9 kB glue + 0.7 kB shell).

That is the reason this port exists. The Dioxus port of the same app — same
model, same renderer, same controls — is 230.6 kB, and the difference is almost
entirely the mutation protocol: Dioxus ships an interpreter (`sledgehammer` +
`dioxus-interpreter-js`) whose bytecode `run()` loop, interning tables and
template store pull in **328 host imports**. Leptos clones templates and
manipulates nodes directly, and needs a fraction of that.

Measured floors, same profile and same `wasm-opt -Oz` pass, for context:

| build | gzipped |
| --- | --- |
| wasm-bindgen, no framework, no model | 11.7 kB |
| Leptos, one signal and one button | 33.7 kB |
| **Leptos, this app** | **120.6 kB** |
| Dioxus, this app | 230.6 kB |
| Plain Svelte + Vite | 27.2 kB |
| Vue 3 | 36.0 kB |
| Elm | 43.9 kB |

A Rust UI framework has a floor of roughly 34 kB before it has drawn anything:
the reactive runtime, the `web-sys` bindings and `std` are not free. Leptos is
the cheapest way to keep a framework and still be under 130 kB; dropping the
framework entirely is the only way to reach the JavaScript ports' band.

## The physics

Water arrives at the tip, runs back **up** the outer surface as a thin
gravity-driven film, loses CO2 to the cave air, and precipitates calcite. Every
mole of calcium taken out of the film becomes one mole of calcite added to the
solid, so the growth rate is fixed by the calcium the drip water can deliver and
by how fast the film can shed it as rock.

* the film: Nusselt `h = (3νq / gP)^⅓`, speed `u = gh² / 3ν`, Reynolds
  `Re = 4q / Pν`
* degassing: `k = D / h²`, so a thick film is a slow one
* chemistry: Henry's law, K1, K2 and the calcite solubility product, closed with
  electroneutrality and solved by **geometric** bisection over `1e-10 … 1e-6`
* the budget: deposition `R = k(c − c_eq)` over 160 nodes 8 mm apart, and a
  **depletion length** `L = Q / kP`
* the shape: a cone of fixed taper whose length is inverted from the deposited
  volume, so mass is conserved by construction

Nothing about the shape is imposed. The taper emerges because the water supply is
fixed while the surface that water must cover widens with radius.

## Run it

The Rust toolchain and target directory live outside this directory: the
workspace is capped at 2 GB and a toolchain does not fit in it.

```sh
export RUSTUP_HOME=/tmp/rustenv/rustup CARGO_HOME=/tmp/rustenv/cargo
export PATH="$CARGO_HOME/bin:$PATH"

cargo test --lib                       # 10 physics parity tests, native, no browser
./scripts/build-web.sh                 # -> dist/  (cargo + wasm-bindgen + wasm-opt)
python3 -m http.server -d dist 8080    # then open http://localhost:8080
npx wrangler deploy                    # publish dist/ to Cloudflare
```

`.cargo/config.toml` pins `target-dir = /tmp/lx-target` for the same reason.

## What is ported

| Elm | Leptos |
| --- | --- |
| `src/Physics.elm` | `src/physics.rs` |
| `src/Canvas.elm` | `src/canvas.rs` |
| `src/Scene.elm` | `src/scene.rs` |
| `src/Ui.elm` | `src/theme.rs` |
| `src/Main.elm` | `src/app.rs` |

`physics`, `canvas` and `scene` carry no Leptos dependency at all, so the model
and the renderer are tested natively. The parity suite lives in `src/lib.rs`
under `#[cfg(test)]` rather than in `tests/`: the crate is a `cdylib` only, which
keeps the wasm about 13 kB gzipped smaller than `["cdylib", "rlib"]` does, and an
integration test cannot link a `cdylib`.

## Three things Leptos makes you think about

**Fine-grained means the tree is built once.** A signal write re-runs only the
closures that read it, so every readout is a `Signal::derive` and the 44 reactive
text slots update individually. Nothing re-renders the component.

**`Callback` wants `Send + Sync`.** The obvious way to pass a handler down to a
child component is `Callback<f64>`, and it does not compile here: it requires
`Send + Sync`, and the handlers capture `RwSignal`s, which are neither. The
component takes a generic `F: Fn(f64) + Copy + 'static` instead. The one place
the bound is real is `on_cleanup`, which is why the animation-frame id exists
twice — once as an `Rc` handle the tick writes, once as an `Arc<AtomicI32>` the
cleanup reads.

**Shared handles are cloned at each capture site.** `Rc<Cell<f64>>` is `Clone`,
not `Copy`, so every closure that needs one takes its own clone, and the clone
has to happen *outside* the closure that will consume it. The `local!` macro in
`app.rs` makes those sites visible rather than implied.

## Keyboard

| key | |
| --- | --- |
| `space` | run / pause |
| `r` | new stalactite |
| `y` | year bands |
| `←` `→` | time-lapse speed |

The URL hash carries the whole scenario, so a link reproduces the cave exactly —
including how far the formation had grown.
