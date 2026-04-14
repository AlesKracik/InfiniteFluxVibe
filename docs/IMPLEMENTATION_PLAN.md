# Implementation Plan: Infinite Flux

## Overview

This document outlines the phased technical implementation plan for Infinite Flux. The architecture is built on **Rust** using the **Bevy ECS** game engine, with a focus on learning Rust idioms through iterative, working milestones.

Each phase produces a playable (or testable) artifact. No phase depends on having "everything" — we build vertical slices and expand horizontally.

The game supports a wide range of emergent player activities (mining, manufacturing, trading, hauling, combat, research, politics, espionage, banking, exploration). There is no class system — players become what they do. Mining and factory-building are implemented first because they teach foundational mechanics and Rust concepts, but subsequent phases systematically introduce the remaining activities until the full breadth is available.

---

## Learning Philosophy

The primary purpose of Infinite Flux is to serve as a **practical, large-scale vehicle for the developer to learn and master the Rust programming language**. Every phase is sequenced not just to ship game features, but to introduce and deepen Rust concepts in a deliberate progression:

- **Features serve learning, not the other way around.** Each phase targets specific Rust concepts. The game feature is the *context* in which you practice them.
- **Understand before you ship.** Each phase includes **Rust Learning Checkpoints** — deliberate exercises where you implement something two ways, profile a bottleneck, or refactor to explore an idiom. These are not optional — they are the point.
- **Explicit over magical.** Prefer clear, idiomatic Rust over macro-heavy abstractions. When a macro or derive would hide what's happening, write it out by hand first, understand it, *then* decide if the macro is justified.
- **Progressive mastery.** Concepts introduced in early phases (ownership, traits, iterators) are revisited in later phases at higher complexity (async lifetimes, unsafe, custom allocators). The spiral is intentional.

---

## Rust Concepts Progression Map

This table maps core Rust concepts to the phases where they are **introduced** (I), **practiced** (P), and **deepened** (D) at higher complexity.

| Rust Concept | Ph0 | Ph1 | Ph2 | Ph3 | Ph4 | Ph5 | Ph6 | Ph7 | Ph8 | Ph9 | Ph10 | Ph11 |
|:---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| Ownership & borrowing | I | P | P | P | D | D | D | D | D | D | D | D |
| Modules & crate structure | I | P | P | P | P | P | P | P | P | P | P | P |
| Structs, enums, pattern matching | I | P | P | D | D | D | D | D | D | D | D | D |
| Traits & generics | | I | P | P | D | D | D | D | D | D | D | D |
| Iterators & closures | | I | P | P | P | D | D | D | D | D | D | D |
| Error handling (`Result`, `?`, custom errors) | I | P | P | P | P | D | D | D | D | D | D | D |
| ECS architecture (Bevy) | I | P | P | D | D | D | D | D | D | D | D | D |
| Lifetimes (explicit) | | | I | P | D | D | D | D | D | D | D | D |
| Smart pointers (`Box`, `Rc`, `Arc`) | | | | I | P | D | D | D | D | D | D | D |
| Async/await & `Future` | | | | | I | D | D | P | P | P | P | D |
| Concurrency (`Send`, `Sync`, `Mutex`, channels) | | | | | | I | D | D | D | D | D | D |
| Unsafe Rust | | | | | | | | I | P | P | P | D |
| Macros (declarative & procedural) | | | | | | | | | I | P | D | D |
| Performance profiling & optimization | | | | | | | P | I | P | P | P | D |
| FFI & advanced type system | | | | | | | | | | | I | D |

---

## Technology Stack

| Layer | Technology | Rationale |
|:---|:---|:---|
| Game Engine / ECS | Bevy 0.18 | Rust-native, ECS-first, active community, good learning vehicle for Rust patterns |
| Rendering | Bevy built-in (wgpu) | Cross-platform GPU abstraction via WebGPU/Vulkan/Metal/DX12 |
| Networking | Quinn (QUIC) or Renet | Low-latency UDP-based, Rust-native. Quinn for raw QUIC, Renet for game-oriented abstraction |
| Serialization | Serde + Bincode/MessagePack | Fast binary serialization for network and save data |
| Database (server) | PostgreSQL + SQLx | Async, compile-time checked queries, good Rust ecosystem support |
| Async Runtime | Tokio | Industry-standard async runtime for server-side tasks |
| UI Framework | Bevy UI or egui (via bevy_egui) | egui for rapid prototyping; migrate to Bevy UI for final polish |
| Build/CI | Cargo workspaces + GitHub Actions | Workspace for multi-crate organization; CI for automated testing |

---

## Workspace Structure

```
infinite-flux/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── if_common/                # Shared types, components, constants
│   ├── if_world/                 # World simulation (resources, grid, ticking)
│   ├── if_factory/               # Factory mechanics (belts, machines, recipes)
│   ├── if_logistics/             # Interplanetary freight routes
│   ├── if_economy/               # Market, contracts, banking
│   ├── if_combat/                # Fleet combat, damage, targeting
│   ├── if_politics/              # Sovereignty, governance, espionage
│   ├── if_tech/                  # Research, tech tree, upgrades
│   ├── if_client/                # Bevy app: rendering, UI, input
│   ├── if_server/                # Authoritative server: networking, persistence
│   └── if_protocol/              # Shared network message definitions
├── assets/                       # Art, audio, configs
├── docs/                         # Design documents
└── tools/                        # Dev tools, map editors, etc.
```

Each crate is a Bevy plugin (or set of plugins) that can be developed and tested in isolation.

---

## Phase 0: Foundation (Weeks 1–3)

**Goal:** Establish the Rust project structure, basic Bevy app, and developer workflow.

**Rust Learning Focus:** Cargo workspaces, module system, basic ECS (Entity, Component, System), the borrow checker.

### Tasks:
- [ ] **P0.1** Set up workspace with `if_common`, `if_client`, and `if_world` crates
- [ ] **P0.2** Create a Bevy app that opens a window with a 2D camera
- [ ] **P0.3** Implement a tile-based grid system in `if_world` (2D array of tiles with terrain types)
- [ ] **P0.4** Render the grid as colored quads or sprites in `if_client`
- [ ] **P0.5** Implement basic camera controls (pan, zoom) with keyboard/mouse
- [ ] **P0.6** Set up CI pipeline: `cargo clippy`, `cargo test`, `cargo fmt --check`
- [ ] **P0.7** Write unit tests for grid system (tile access, bounds checking)

### Rust Learning Checkpoints:
- [ ] **P0.L1** Deliberately trigger a borrow checker error (e.g., mutable + immutable borrow of the grid), then fix it. Document *why* the compiler rejects it and what the fix teaches about ownership.
- [ ] **P0.L2** Implement the grid using both `Vec<Vec<Tile>>` and a flat `Vec<Tile>` with index math. Compare ergonomics and understand how Rust's ownership model differs between the two.
- [ ] **P0.L3** Experiment with module visibility: make a struct field `pub` vs `pub(crate)` vs private, and observe the compiler errors from other crates. Write a short comment in code explaining when to use each.

### Deliverable:
A window showing a colored grid (representing a planetary surface) that the player can pan and zoom around.

---

## Phase 1: Core Factory Mechanics (Weeks 4–8)

**Goal:** Build the fundamental factory simulation — placing machines, routing belts, producing items.

**Rust Learning Focus:** Traits, generics, the type system, `HashMap`/`BTreeMap`, iterators, ECS queries.

### Tasks:
- [ ] **P1.1** Define core components in `if_common`: `Position`, `GridCell`, `ItemStack`, `Recipe`
- [ ] **P1.2** Implement resource node entities (spawn on grid, have yield + depletion rate)
- [ ] **P1.3** Implement mining drill entity: extracts items from resource node per tick
- [ ] **P1.4** Implement transport line entity: moves items between grid cells per tick
  - Line routing: direction, speed, item slots
  - Splitters and mergers
- [ ] **P1.5** Implement processing machine entity: consumes input items, produces output items per recipe
- [ ] **P1.6** Implement building placement system:
  - Ghost preview with validity checking
  - Grid-snapped placement via mouse click
  - Building removal
- [ ] **P1.7** Implement basic item rendering: icons on belts, item counts on machines
- [ ] **P1.8** Implement power system: generators produce power, machines consume power, grid balancing
- [ ] **P1.9** Create 5–10 basic recipes (ore → ingot → plate → component)
- [ ] **P1.10** Implement statistics overlay: items/min throughput per belt segment
- [ ] **P1.11** Implement skill progression system: `SkillLevel` newtype with diminishing-returns bonus curve, use-based XP gain, per-player skill map
- [ ] **P1.12** Apply skill bonuses to mining drill yield and machine processing speed

### Rust Learning Checkpoints:
- [ ] **P1.L1** Define a `Processable` trait with a method `fn process(&self, input: &[ItemStack]) -> Option<ItemStack>`. Implement it for different machine types. Understand trait dispatch and when to use `dyn Trait` vs generics.
- [ ] **P1.L2** Rewrite the transport line item-movement system using iterator chains (`.filter().map().collect()`) instead of manual loops. Compare readability and understand zero-cost abstractions.
- [ ] **P1.L3** Implement recipe lookup using both `HashMap` and `BTreeMap`. Benchmark with `cargo bench` and understand when ordered vs unordered maps are appropriate.
- [ ] **P1.L4** Implement `SkillLevel` as a newtype wrapper around `u32`. Implement `Display`, and a `bonus(&self) -> f32` method using the diminishing-returns formula. Understand the newtype pattern: why wrapping a primitive prevents mixing "mining level" with "trading level" at the type level.

### Deliverable:
A playable factory builder where you can place drills, belts, and machines to produce items in a chain. No UI menus yet — hotkeys only.

---

## Phase 2: UI & Player Interaction (Weeks 9–12)

**Goal:** Build the essential UI layer so the game is actually usable.

**Rust Learning Focus:** Bevy UI or egui integration, event systems, state management, `Option`/`Result` patterns, lifetimes (first encounter).

### Tasks:
- [ ] **P2.1** Integrate `bevy_egui` for rapid UI prototyping
- [ ] **P2.2** Build the building palette toolbar (categorized, searchable)
- [ ] **P2.3** Build the resource overlay panel (throughput meters, alerts)
- [ ] **P2.4** Build the statistics dashboard (production/consumption graphs)
- [ ] **P2.5** Implement tooltip system (hover, hold, click tiers)
- [ ] **P2.6** Implement notification/alert system (toast, persistent, full-screen)
- [ ] **P2.7** Add sound effects for placement, production, alerts
- [ ] **P2.8** Implement save/load system: serialize world state to disk (Serde + Bincode)
- [ ] **P2.9** Implement Blueprint system: select region → save → load → paste as ghost
- [ ] **P2.10** Add the tutorial sequence (guided first-drill walkthrough)

### Rust Learning Checkpoints:
- [ ] **P2.L1** Implement a custom error type using `thiserror` for save/load failures. Chain errors with context. Understand the difference between `anyhow` (application) and `thiserror` (library) patterns.
- [ ] **P2.L2** In the Blueprint system, encounter your first explicit lifetime: a `BlueprintView<'a>` that borrows from the world without cloning. If the borrow checker fights you, work through *why* — this is the learning moment.
- [ ] **P2.L3** Implement the notification system using Bevy's `Event<T>` and `EventReader`. Then compare with a manual `Vec<Notification>` resource approach. Understand ECS event patterns vs shared state.

### Deliverable:
A polished single-player factory builder with full UI, save/load, blueprints, and a tutorial. This is the **first publicly demoable milestone**. At this stage, mining and manufacturing are the primary activities; later phases add trading (Phase 6), combat (Phase 7), politics (Phase 8), and more — progressively building toward the full breadth of emergent player activities.

---

## Phase 3: Planetary Expansion & Multi-Surface (Weeks 13–17)

**Goal:** Support multiple planets/moons, each with their own factory grids. Introduce orbital view.

**Rust Learning Focus:** Enums with data, `match` exhaustiveness, resource management (Assets), scene management, `Box` and smart pointers (first encounter).

### Tasks:
- [ ] **P3.1** Implement celestial body data model: planets, moons, asteroids with properties (gravity, atmosphere, resource distribution)
- [ ] **P3.2** Implement star system model: collection of celestial bodies with orbital positions
- [ ] **P3.3** Implement the orbital/system camera view (3D orrery-style)
- [ ] **P3.4** Implement seamless transition: system view → click planet → surface view
- [ ] **P3.5** Implement varying resource distributions per planet type
- [ ] **P3.6** Implement environmental modifiers: gravity affects belt speed, atmosphere affects power gen
- [ ] **P3.7** Implement orbital station entity: storage hub in orbit, docking point
- [ ] **P3.8** Implement basic ship entity: can fly between celestial bodies in-system
- [ ] **P3.9** Implement cargo transfer: ship ↔ station ↔ planet surface

### Rust Learning Checkpoints:
- [ ] **P3.L1** Model `CelestialBody` as a rich enum: `Planet { gravity, atmosphere }`, `Asteroid { composition }`, `Moon { parent }`. Use `match` everywhere and let the compiler enforce exhaustiveness when you add a new variant.
- [ ] **P3.L2** Encounter `Box<dyn Trait>` for the first time: use it to store heterogeneous environmental modifiers in a collection. Understand heap allocation, trait objects, and vtable dispatch.
- [ ] **P3.L3** Implement cargo transfer with a state machine enum: `CargoTransfer { Requested, InTransit, Delivered, Failed(String) }`. Practice the `enum`-as-FSM pattern that will be central in Phase 4.

### Deliverable:
A multi-planet factory game where you manage factories on several surfaces and move goods between them via ships.

---

## Phase 4: Interplanetary Logistics (Weeks 18–22)

**Goal:** Automate freight routes between locations. Introduce the galaxy map.

**Rust Learning Focus:** Async patterns (first encounter), state machines (route states), the `enum` as FSM pattern, lifetimes in complex data structures, graph data structures.

### Tasks:
- [ ] **P4.1** Implement the galaxy map (2D star map with systems as nodes, warp lanes as edges)
- [ ] **P4.2** Implement warp travel: ship moves between systems via warp gates (travel time based on distance)
- [ ] **P4.3** Implement the Logistics Manager UI panel
- [ ] **P4.4** Implement freight route data model: origin, destination, cargo filters, schedule
- [ ] **P4.5** Implement autonomous freighter AI: ship follows route, loads/unloads, repeats
- [ ] **P4.6** Implement fuel system: warp consumes fuel proportional to mass and distance
- [ ] **P4.7** Implement route optimization hints: fuel cost display, travel time estimates
- [ ] **P4.8** Resource depletion: nodes slowly deplete, forcing players to find new sources

### Rust Learning Checkpoints:
- [ ] **P4.L1** Implement the galaxy graph with explicit lifetimes: `struct Galaxy<'a>` holding references to `StarSystem` nodes. Fight the borrow checker on graph data structures — then understand *why* graphs are hard in Rust (cyclic references) and refactor to use indices or `petgraph`.
- [ ] **P4.L2** Model the entire freight route lifecycle as an enum FSM with 6+ states. Implement state transitions as `impl RouteState { fn advance(self) -> RouteState }` — note the `self` by value, consuming the old state. Understand why this prevents invalid state transitions at compile time.
- [ ] **P4.L3** Write your first `async fn` to simulate a long-running route calculation. Understand what the compiler transforms it into (a state machine!), connecting it to the FSM pattern you just built by hand.

### Deliverable:
A single-player space logistics game with automated trade routes across a small galaxy.

---

## Phase 5: Networking Foundation (Weeks 23–30)

**Goal:** Transform from single-player to client-server architecture. This is the hardest technical phase.

**Rust Learning Focus:** Async/await with Tokio, `Arc<Mutex<>>`, channels (`mpsc`), network serialization, error handling at scale, the `Send + Sync` trait bounds. **This is the hardest Rust learning phase** — concurrency is where Rust's guarantees shine and where the learning curve is steepest.

### Tasks:
- [ ] **P5.1** Create `if_protocol` crate: define all client↔server messages with Serde
- [ ] **P5.2** Create `if_server` crate: headless Bevy app running the authoritative simulation
- [ ] **P5.3** Implement basic networking with Quinn/Renet: client connects, receives world state
- [ ] **P5.4** Implement client-side prediction for building placement
- [ ] **P5.5** Implement server-authoritative factory ticking: server runs simulation, sends state deltas
- [ ] **P5.6** Implement player authentication (simple token-based initially)
- [ ] **P5.7** Implement persistence: server saves world state to PostgreSQL via SQLx
- [ ] **P5.8** Implement area-of-interest: clients only receive updates for nearby entities
- [ ] **P5.9** Stress test: multiple clients connected, building factories simultaneously
- [ ] **P5.10** Implement basic chat system

### Rust Learning Checkpoints:
- [ ] **P5.L1** Deliberately try to share a `HashMap` between two Tokio tasks without `Arc<Mutex<>>`. Read the compiler error about `Send + Sync` carefully. Then add `Arc<Mutex<>>` and understand what each layer provides: `Arc` = shared ownership across threads, `Mutex` = exclusive access.
- [ ] **P5.L2** Implement message passing between client handler and game loop using `tokio::sync::mpsc`. Then try the same with `std::sync::mpsc`. Understand why mixing async and sync channels causes problems and when to use each.
- [ ] **P5.L3** Write an SQLx query with compile-time checked SQL (`sqlx::query!`). Intentionally write a wrong column name and see the *compile-time* error. Understand how Rust's macro system enables this.
- [ ] **P5.L4** Profile the server under load with `cargo flamegraph`. Identify a hot path and optimize it. Document what you learned about Rust's zero-cost abstractions (or where they aren't zero-cost).

### Deliverable:
Multiple players can connect to a server, see each other, and build factories on the same planet.

---

## Phase 6: Economy & Market (Weeks 31–36)

**Goal:** Implement the player-driven economy.

**Rust Learning Focus:** Complex data structures (order books), database transactions, concurrent mutation patterns, numerical precision (fixed-point for currency), `unsafe` (first encounter for performance-critical matching engine).

### Tasks:
- [ ] **P6.1** Implement station-local market with order book (bid/ask matching engine)
- [ ] **P6.2** Implement the Market UI: order book, price history charts, search
- [ ] **P6.3** Implement player wallet and credit system (fixed-point arithmetic, no floats for money)
- [ ] **P6.4** Implement buy/sell order placement, matching, settlement
- [ ] **P6.5** Implement courier contracts: post, accept, complete, fail (collateral)
- [ ] **P6.6** Implement price history storage and charting
- [ ] **P6.7** Implement manufacturing contracts: request items be built for payment
- [ ] **P6.8** Implement corporation wallet with access controls

### Rust Learning Checkpoints:
- [ ] **P6.L1** Implement fixed-point currency by creating a `Credits` newtype wrapper around `i64`. Implement `Add`, `Sub`, `Mul`, `Display` manually (not via derive). Understand operator overloading via traits and why newtypes prevent mixing units.
- [ ] **P6.L2** Implement the order book matching engine. Then profile it. If there's a hot loop, write a small `unsafe` block to skip bounds checking — then wrap it in a safe API. Understand the `unsafe` contract: what invariants you must uphold, and why safe Rust can't express them.
- [ ] **P6.L3** Implement database transactions for order settlement where two wallets are updated atomically. Handle every `Result` — no `.unwrap()` in production paths. Practice the `?` operator chains and understand early returns.

### Deliverable:
A functioning in-game economy where players trade goods at station markets.

---

## Phase 7: Combat System (Weeks 37–44)

**Goal:** Implement tactical fleet combat.

**Rust Learning Focus:** Fixed-timestep physics, spatial data structures (for targeting/range), ECS system ordering and dependencies, performance profiling, `unsafe` for spatial indexing.

### Tasks:
- [ ] **P7.1** Implement ship fitting system: modules, hardpoints, power grid, CPU
- [ ] **P7.2** Implement combat components: weapons, shields, armor, capacitor
- [ ] **P7.3** Implement targeting system: lock time, signature radius, scan resolution
- [ ] **P7.4** Implement damage model: damage types, resistances, falloff
- [ ] **P7.5** Implement heat system: overheating, burnout risk
- [ ] **P7.6** Implement ammunition: physical cargo consumption per shot
- [ ] **P7.7** Implement fleet grouping: formation, fleet commander broadcast
- [ ] **P7.8** Implement electronic warfare: jamming, dampening, webifying
- [ ] **P7.9** Implement ship destruction: wreck entity, lootable cargo
- [ ] **P7.10** Implement Fleet Command UI: tactical view, target selection, module activation
- [ ] **P7.11** Implement basic NPC enemies (Silica Swarm) for PvE content

### Rust Learning Checkpoints:
- [ ] **P7.L1** Implement a spatial hash grid for range queries (which ships are within weapon range). Understand how to choose between `HashMap<(i32,i32), Vec<Entity>>` vs a dedicated spatial crate. Profile both approaches with `cargo flamegraph`.
- [ ] **P7.L2** Use Bevy's system ordering (`.before()`, `.after()`, system sets) to enforce that damage is applied *after* targeting resolves. Intentionally break the ordering and observe the race condition. Understand why ECS system ordering is Rust's answer to implicit dependency bugs.
- [ ] **P7.L3** Implement the ship fitting validator using the type system: make it impossible to mount a Large weapon on a Small hardpoint at compile time (hint: phantom types or const generics). Compare with a runtime check. Understand when compile-time guarantees are worth the complexity.

### Deliverable:
Players can fit ships, form fleets, and engage in tactical combat against NPCs and other players.

---

## Phase 8: Politics & Governance (Weeks 45–50)

**Goal:** Implement territorial control, corporations, and governance.

**Rust Learning Focus:** Complex permission systems via the type system, state machines for elections, event-driven architecture, declarative macros for reducing boilerplate.

### Tasks:
- [ ] **P8.1** Implement corporation system: founding, roles, permissions, shared assets
- [ ] **P8.2** Implement alliance system: groups of corps with shared standings
- [ ] **P8.3** Implement sovereignty: claim structures, territory control
- [ ] **P8.4** Implement customs offices: automated tariff collection at warp gates
- [ ] **P8.5** Implement the Stock Market: IPO, share trading, ownership tracking
- [ ] **P8.6** Implement hostile takeover mechanics: controlling interest → CEO transfer
- [ ] **P8.7** Implement elected councils: nomination, voting, policy levers
- [ ] **P8.8** Implement banking: loans, interest, default → bounty system
- [ ] **P8.9** Implement audit logs for counter-intelligence
- [ ] **P8.10** Implement the Governance UI panels

### Rust Learning Checkpoints:
- [ ] **P8.L1** Model the permission system using Rust's type system: create a `Permission<Action, Role>` type where invalid combinations (e.g., `Permission<DeclareWar, Recruit>`) are unrepresentable. Compare with a runtime bitflag approach. Understand typestate pattern.
- [ ] **P8.L2** Write your first `macro_rules!` macro to reduce boilerplate in the election state machine (e.g., `define_election_phase!(Nomination, Voting, Tallying, Concluded)`). Understand macro hygiene, repetition patterns, and when macros hurt readability.
- [ ] **P8.L3** Implement the audit log as an append-only `Vec<AuditEntry>` wrapped in a newtype that only exposes `push()` and iteration — no `remove()`, no `&mut [T]`. Practice encapsulation as a safety guarantee.

### Deliverable:
Player corporations can claim territory, trade shares, run elections, and engage in economic warfare.

---

## Phase 9: The Astro-Topography (Weeks 51–56)

**Goal:** Implement the three zone types that define risk/reward.

**Rust Learning Focus:** Advanced trait bounds, conditional compilation (`cfg` attributes), the `where` clause in complex generics, procedural macros (first encounter).

### Tasks:
- [ ] **P9.1** Implement zone classification system on star systems
- [ ] **P9.2** **Precursor Cradles:** EM Dampening Field — disable weapon modules, enforce no-PvP
- [ ] **P9.3** **Fracture Sectors:** Silica Swarm AI — wave spawning, hive mechanics, aggro on PvP
- [ ] **P9.4** **Silent Void:** No restrictions — full PvP, richest resources
- [ ] **P9.5** Implement zone-specific environmental effects (visual, audio, gameplay)
- [ ] **P9.6** Balance resource yields across zones (risk = reward)
- [ ] **P9.7** Implement siege mechanics: blockade, orbital bombardment, FOB, breach sequence

### Rust Learning Checkpoints:
- [ ] **P9.L1** Implement zone rules using trait bounds: `fn fire_weapon<Z: Zone>(zone: &Z, ...) where Z: AllowsCombat`. Make the Precursor Cradle zone *not* implement `AllowsCombat`, so calling `fire_weapon` in that zone is a compile-time error. Understand how trait bounds encode business rules.
- [ ] **P9.L2** Use conditional compilation to create a `#[cfg(feature = "dev_tools")]` module with debug commands for zone-testing (teleport, spawn waves, toggle zone rules). Understand feature flags and how they eliminate dead code at compile time.
- [ ] **P9.L3** Write a simple procedural derive macro: `#[derive(ZoneEffect)]` that auto-generates the environmental modifier boilerplate. Set up a `proc-macro` crate. Understand tokenstream manipulation and the proc-macro compilation model.

### Deliverable:
The game world has meaningful geographic variety with distinct risk/reward profiles.

---

## Phase 10: Research & Megastructures (Weeks 57–62)

**Goal:** Implement the tech tree and late-game goals.

**Rust Learning Focus:** Advanced generics with const generics, FFI basics (if integrating external libs), custom allocators, advanced type system patterns.

### Tasks:
- [ ] **P10.1** Implement research hub building: consumes manufactured items for research points
- [ ] **P10.2** Implement tech tree data model: nodes, prerequisites, costs, unlock effects
- [ ] **P10.3** Implement tech tree UI: zoomable node graph
- [ ] **P10.4** Implement infinite marginal upgrades: exponential cost formula
- [ ] **P10.5** Implement tech unlock effects: new recipes, better machines, new ship modules
- [ ] **P10.6** Implement megastructure framework: multi-phase server-wide construction projects
- [ ] **P10.7** Implement Dyson Sphere: multi-corporation project, sector-wide power bonus
- [ ] **P10.8** Implement artificial wormholes: create new connections in the galaxy map

### Rust Learning Checkpoints:
- [ ] **P10.L1** Implement the exponential tech cost formula using const generics: `struct TechLevel<const TIER: u32>` where the cost multiplier is computed at compile time. Understand how const generics differ from runtime values and where they are/aren't appropriate.
- [ ] **P10.L2** The megastructure system involves massive entity counts. Implement a custom arena allocator for megastructure components. Understand `GlobalAlloc`, alignment, and why Rust makes you think about allocation explicitly.
- [ ] **P10.L3** If integrating an external library (e.g., a graph algorithm in C), write an FFI binding with `extern "C"`. Wrap it in a safe Rust API. Understand the `unsafe` boundary at FFI and how to minimize the unsafe surface area.

### Deliverable:
Corporations can research technology and collaborate on galaxy-altering megastructures.

---

## Phase 11: Polish, Optimization & Launch Prep (Weeks 63–72)

**Goal:** Performance, polish, and prepare for public release.

**Rust Learning Focus:** Advanced profiling, SIMD intrinsics, cache-friendly data layout, the culmination of everything — this phase is about mastery through optimization.

### Tasks:
- [ ] **P11.1** Performance profiling and optimization: ECS query optimization, rendering LOD
- [ ] **P11.2** Network optimization: delta compression, priority queuing, bandwidth management
- [ ] **P11.3** Server scaling: sharding strategy for single-shard feel with horizontal scaling
- [ ] **P11.4** Anti-cheat: server-authoritative validation, rate limiting
- [ ] **P11.5** Accessibility implementation: colorblind modes, UI scaling, key remapping
- [ ] **P11.6** Audio: ambient soundscapes, combat audio, UI sounds
- [ ] **P11.7** Visual polish: particle effects, lighting, post-processing
- [ ] **P11.8** Load testing: simulate thousands of concurrent players
- [ ] **P11.9** Onboarding polish: tutorial refinement, mentor system
- [ ] **P11.10** Documentation: player guide, API docs for modding hooks

### Rust Learning Checkpoints:
- [ ] **P11.L1** Use `cargo flamegraph` and `perf` to identify the top 3 CPU hotspots. Optimize each one differently: one with better algorithms, one with cache-friendly data layout (SoA vs AoS), one with SIMD via `std::arch` or `packed_simd`. Document the before/after.
- [ ] **P11.L2** Audit the entire codebase for `.unwrap()` and `.expect()` in non-test code. Replace each with proper error handling or document why a panic is acceptable. This is a mastery exercise in Rust's error philosophy.
- [ ] **P11.L3** Write a retrospective: for each phase, note the Rust concept that was hardest, the "aha moment" that made it click, and what you'd do differently. This is not code — it's the capstone of the learning journey.

### Deliverable:
A launch-ready MMO.

---

## Risk Register

| Risk | Impact | Mitigation |
|:---|:---|:---|
| Single-shard networking at scale | High | Design area-of-interest from Phase 5. Plan for spatial sharding with seamless handoff. |
| Bevy API breaking changes | Medium | Pin Bevy version per phase. Upgrade between phases, not during. |
| Economy exploits (duplication, market manipulation) | High | Server-authoritative everything. Fixed-point currency. Transaction logging. |
| Scope creep | High | Each phase has a concrete deliverable. Cut scope within phases, not across. |
| Player retention before content critical mass | Medium | Ensure Phase 2 deliverable (solo factory builder) is fun standalone. |

---

## Architecture Principles

1. **Server is truth:** All game state mutations happen server-side. Clients are dumb renderers with prediction.
2. **ECS everywhere:** Model all game objects as entities with components. Logic lives in systems. No inheritance.
3. **Crate boundaries = API boundaries:** Each crate exposes a clean plugin interface. Internal details are private.
4. **Test at every level:** Unit tests in each crate. Integration tests for cross-crate interactions. Load tests for networking.
5. **Data-driven:** Recipes, tech trees, ship stats, and resource distributions are loaded from data files (RON/JSON), not hardcoded.
6. **Fixed-point for economy:** Never use `f32`/`f64` for currency. Use integer-based fixed-point arithmetic.
7. **Explicit over magical:** Prefer clear, idiomatic Rust over macro-heavy abstractions. Write it by hand first, understand it, then decide if a macro or derive is justified. Clarity for learning over brevity.
8. **Learning checkpoints are not optional:** Each phase's Rust Learning Checkpoint tasks are first-class deliverables, not afterthoughts. They are the mechanism by which game features become Rust mastery.
