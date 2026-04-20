# Implementation Plan: Infinite Flux Vibe

## Overview

Vibe-coded version of Infinite Flux. Same game design, same architecture, but optimized for speed with AI agents doing parallel work. No learning checkpoints — just ship features.

Each phase produces a playable artifact. Phases are compressed and can overlap when agents work in parallel.

---

## Technology Stack

| Layer | Technology |
|:---|:---|
| Game Engine / ECS | Bevy 0.18 |
| Rendering | Bevy built-in (wgpu) |
| Networking | Quinn (QUIC) or Renet |
| Serialization | Serde + Bincode |
| Database (server) | PostgreSQL + SQLx |
| Async Runtime | Tokio |
| UI Framework | bevy_egui → Bevy UI |
| Build/CI | Cargo workspaces + GitHub Actions |

---

## Workspace Structure

```
infinite-flux-vibe/
├── Cargo.toml
├── crates/
│   ├── if_common/        # Shared types, components, constants
│   ├── if_world/         # World simulation (resources, grid, ticking)
│   ├── if_factory/       # Factory mechanics (transport, machines, recipes)
│   ├── if_logistics/     # Interplanetary freight routes
│   ├── if_economy/       # Market, contracts, banking
│   ├── if_combat/        # Fleet combat, damage, targeting
│   ├── if_politics/      # Sovereignty, governance, espionage
│   ├── if_tech/          # Research, tech tree, upgrades
│   ├── if_client/        # Bevy app: rendering, UI, input
│   ├── if_server/        # Authoritative server: networking, persistence
│   └── if_protocol/      # Shared network message definitions
├── assets/
├── docs/
└── tools/
```

---

## Phase 0: Foundation ✅ DONE

Workspace setup, Bevy app, grid system, camera controls, CI pipeline.

---

## Phase 1: Core Factory ✅ DONE

Items, recipes, mining, transport lines, machines, power grid, building placement, HUD, throughput stats, skill progression.

---

## Phase 2: UI & Polish ✅ DONE

**Goal:** Full UI layer — the game becomes actually usable and demoable.

- [x] Integrate bevy_egui for UI panels
- [x] Building palette toolbar (categorized, clickable)
- [x] Resource overlay panel (throughput meters, power status)
- [x] Statistics dashboard (building counts, stock, throughput)
- [x] Tooltip system (building hover details)
- [x] Notification/alert system (placement, depletion, power alerts)
- [x] Sound effects (framework, graceful fallback when assets missing)
- [x] Save/load (Serde + Bincode, F5/F9)
- [x] Blueprint system (copy/paste factory layouts, B hotkey)
- [x] Tutorial sequence (6-step guided onboarding)

---

## Phase 3: Planetary Expansion ✅ DONE

**Goal:** Multiple planets/moons, orbital view, ships.

- [x] Celestial body data model (planets, moons, asteroids)
- [x] Star system model with orbital positions
- [x] Orbital/system camera view (M hotkey toggle)
- [x] Seamless transition: system view → surface view
- [x] Varying resource distributions per planet
- [x] Environmental modifiers (gravity, atmosphere, temperature)
- [x] Orbital stations
- [x] Ship entities + inter-body travel (fuel, speed, state machine)
- [x] Cargo transfer (ship ↔ station ↔ surface)

---

## Phase 4: Interplanetary Logistics ✅ DONE

**Goal:** Automated freight routes, galaxy map.

- [x] Galaxy map (2D star map, warp lanes)
- [x] Warp travel between systems (with fuel cost)
- [x] Logistics Manager UI (L hotkey, route editor)
- [x] Freight route data model + autonomous AI (waypoints, Load/Unload/Visit actions)
- [x] Fuel system (refuel helpers, warp consumption)
- [x] Resource depletion tracking (DepletionStats + events)

---

## Phase 5: Networking

**Goal:** Client-server split. Hardest technical phase.

- [x] if_protocol crate (message definitions)
- [x] if_server crate (headless Bevy app, TCP listener)
- [ ] Quinn/Renet networking (TCP stub in place; upgrade path documented)
- [ ] Client-side prediction
- [ ] Server-authoritative simulation (basic echo/chat only for now)
- [ ] Authentication
- [ ] PostgreSQL persistence (SQLx)
- [ ] Area-of-interest filtering
- [ ] Stress testing
- [x] Chat system (client egui panel + server broadcast)

---

## Phase 6: Economy & Market ✅ DONE

**Goal:** Player-driven economy.

- [x] Order book matching engine (price-time priority, FIFO within level)
- [x] Market UI (K hotkey — order book, buy/sell, mini chart)
- [x] Fixed-point currency (Credits newtype, i64 cents, saturating math)
- [x] Buy/sell orders, matching, settlement (resting-order pricing)
- [x] Courier contracts (J hotkey — job board)
- [x] Price history + charting (ring buffer + egui painter mini-chart)
- [x] Manufacturing + Mercenary contracts
- [x] Corporation wallets (with dividend distribution)

---

## Phase 7: Combat

**Goal:** Tactical fleet combat.

- [ ] Ship fitting (modules, hardpoints, power/CPU)
- [ ] Weapons, shields, armor, capacitor
- [ ] Targeting, damage model, falloff
- [ ] Heat system, ammunition
- [ ] Fleet grouping + FC broadcast
- [ ] Electronic warfare
- [ ] Ship destruction + loot
- [ ] Fleet Command UI
- [ ] Silica Swarm NPC enemies

---

## Phase 8: Politics & Governance

**Goal:** Corps, alliances, territorial control.

- [ ] Corporation system (roles, permissions, shared assets)
- [ ] Alliance system
- [ ] Sovereignty + territory control
- [ ] Customs offices (tariffs)
- [ ] Stock market + hostile takeovers
- [ ] Elected councils + voting
- [ ] Banking (loans, defaults, bounties)
- [ ] Audit logs + counter-intelligence
- [ ] Governance UI

---

## Phase 9: Zone System

**Goal:** Three zone types defining risk/reward.

- [ ] Zone classification
- [ ] Precursor Cradles (safe, no PvP)
- [ ] Fracture Sectors (PvE, Silica Swarm waves)
- [ ] Silent Void (full PvP, richest resources)
- [ ] Zone-specific effects (visual, audio, gameplay)
- [ ] Balance resource yields
- [ ] Siege mechanics

---

## Phase 10: Research & Megastructures

**Goal:** Tech tree, late-game goals.

- [ ] Research hubs
- [ ] Tech tree (nodes, prerequisites, costs)
- [ ] Tech tree UI
- [ ] Infinite marginal upgrades
- [ ] Unlock effects (recipes, machines, modules)
- [ ] Megastructures (Dyson Sphere, wormholes)

---

## Phase 11: Polish & Launch

**Goal:** Ship it.

- [ ] Performance profiling + optimization
- [ ] Network optimization
- [ ] Server scaling
- [ ] Anti-cheat
- [ ] Accessibility
- [ ] Audio polish
- [ ] Visual polish
- [ ] Load testing
- [ ] Tutorial refinement
- [ ] Player documentation

---

## Agent Strategy

This project is designed for **parallel AI agent execution**. See AGENTS.md for the agent roster and their responsibilities.

---

## Architecture Principles

1. **Server is truth** — all state mutations server-side
2. **ECS everywhere** — entities + components + systems, no inheritance
3. **Crate boundaries = API boundaries** — clean plugin interfaces
4. **Test critical systems** — simulation, economy, combat. Don't block on UI tests
5. **Data-driven** — recipes, tech trees, stats from data files (RON/JSON)
6. **Fixed-point for economy** — integer-based currency, never floats
7. **Ship fast, refactor later** — working code first, clean code second