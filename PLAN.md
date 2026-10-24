# re3-rs plan

Rust rewrite of re3 (GTA III, `re3-REDUX/re3-master`) and reVC (Vice City, branch `main-miami`, dir `re3-miami`).

## Decisions
- Incremental: port bottom-up, link into the C++ build over a C ABI, drop C++ when the last piece lands.
- One engine, games selected by Cargo features `gta3` / `vc`; one fixed PC config (`FIX_BUGS` on, no PS2 paths).
- Rendering via `wgpu` (Vulkan/GL/DX12/Metal); windowing via `winit`. Linux + Windows.
- Binaries (gta3.exe / gta-vc.exe) are checked later with REA to find missing functions.

## Releases
One `main` branch, three releases from it, named Re3 (GTA III), re3 Miami (Vice City) and Re3 LA (San Andreas): tags `re3-vX.Y.Z`, `re3-miami-vX.Y.Z`, `re3-la-vX.Y.Z`, each a GitHub Release with that game's binaries (built by `--features` / game flag in the phase 8 binary crates). C++ reference lives outside the repo in ~/Projects/re3-cpp-reference.

## Phases
0. Workspace + golden tests against the C++ build
1. math (use `glam`), text/GXT
2. Formats: dir/IMG, DAT/IDE/IPL, TXD/DFF/COL, saves
3. collision, animation, modelinfo
4. entities, pools, objects, vehicles, weapons
5. peds, control (script VM), save
6. audio
7. renderer + RenderWare replacement
8. core/skel: main loop, input, frontend

## Done (phases 1-2, all checked against the real GTA III and Vice City files)
- `dir` + `Archive`: .dir/.img index and entry reader
- `gxt`: text (GTA III and VC mission tables)
- `datafile`: gta.dat/default.dat directives, IDE (objs/tobj/hier/cars/peds/weap/path/2dfx), IPL (inst/zone/cull/pick/occl/path), GTA3 + VC layouts
- `handling`, `ped` (ped.dat, pedstats, pedgrp), `carcols`, `combat` (weapon.dat, fistfite.dat), `world` (object.dat, surface.dat, water.dat, waterpro.dat, timecyc.dat, particle.cfg, track/flight files)
- `rw` (chunk reader), `txd` (PC textures), `dff` (models: frames, geometry, materials, bin mesh, skin, hanim, lights), `col`
- math: use the `glam` crate, nothing to port

## Phase 3 (done): crate `re3-core`
- `math`: Matrix (4x3), Quat, Rect, with the original's operation order
- `collision` + `surface` + `temp_col`: all shape tests and line/sphere/model queries, surface/adhesion tables, hardcoded col models. Checked on ~380k real triangles
- `anim`: IFP runtime (sequences, blend nodes, associations, clump update, manager, groups, velocity extraction), GTA III and VC timing differences. Group tables generated from the C++ (tools/gen_anim_tables.py); every group resolves against real ped.ifp and the real cop skeleton
- `modelinfo`: registry, TXD slots, LOD/time-model logic, IDE loader, model-index tables and predicates (generated: tools/gen_model_indices.py, gen_name_tables.py). Every real IDE loads within the original table limits
- `re3-formats::ifp`: animation file parser (183 real files)

## Phase 4 (done, GTA III): entities, pools, vehicles, weapons
- done: `pool` (generation handles), `general` (RNG), `timer`, `entity` (Entity + flag packing, Physical state, force maths, process_control)
- done (2026-10-24): `world` (pools, sector grid, add/remove/remove_and_add, line-of-sight, vertical line, ground Z, sphere/range/cube queries; `entity/ent.rs` holds the `Ent` record)
- done: `entity/object.rs` (object.dat table, damage events, create/sweep helpers), `entity/collide.rs` (collision response + friction, GTA III)
- done: `vehicle/` (handling, transmission, damage, door, floater, base = CVehicle) and `world/physics.rs` (ProcessShift/ProcessCollision and sector processors)
- done: Automobile, Boat, Heli, Plane, Train, Cranes, CarGen (`vehicle/*`, `world/{cars,boats,helis,planes,trains,cranes,cargens}.rs`)
- done: `weapons/` (weapon.dat table, CWeapon ammo/clip/state and Fire dispatch, ShotInfo, BulletInfo, ProjectileInfo, Explosion); the shots themselves (instant hit, shotgun, sniper, melee, aiming) need peds, camera and pad, so they sit behind `FireEnv`/`BulletEnv` for phase 5
- done: `world/process.rs` (CWorld::Process loop, TriggerExplosion, object ProcessControl, fire/extinguish/clear/fallen/reposition/restart helpers); ped, path and population parts are `WorldHooks`
- deferred: Bike and every Vice City variant of the physics/vehicle code; renderer-only parts (PreRender/Render, dust, coronas, shadows -> phase 7); save/load; call-off-chase, angled-box and mission-entity queries (need peds/autopilot missions)

## Left for later phases
- phase 4+: entities, pools, vehicles, weapons, peds, script VM, saves, audio, renderer (see top)
- raster pixel decoding (DXT etc.) -> phase 7
- typed particle.cfg / timecyc.dat field names, handling unit conversion -> with their systems
- ColStore / streaming-driven collision loading and level switching -> phase 4 (streaming)
- clump RenderWare objects (atomics, materials), vehicle colour/component code, ped hit col models -> phase 7
- cullzone.dat, train.dat, animviewer.dat, CAPS.DAT are not read by re3; skipped
- known file quirks: 4 empty model entries and 1 PS2 texture in GTA III's gta3.img; waterpro.dat has one bogus extra level
- unverified without the C++ build: Euler quaternion pair (round trip permutes components); skeleton child order assumes file order

## Generated files (regenerate with tools/)
- `re3-core/src/anim/tables_*.rs`, `modelinfo/indices_*.rs`, `modelinfo/names_*.rs`
