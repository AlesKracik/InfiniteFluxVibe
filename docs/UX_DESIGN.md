# UX Design Document: Infinite Flux

## 1. Design Philosophy

The UX must serve two competing demands: **deep complexity** for veterans and **approachable onboarding** for newcomers. The interface draws inspiration from factory-sim games (Factorio, Satisfactory) for the building layer and from EVE Online for the fleet/economy layer. All UI is diegetic where possible — screens, holograms, and data-pads rather than floating HUD elements.

---

## 2. Core Perspectives (Camera Modes)

The player seamlessly transitions between scales of play. Each scale has its own camera, HUD, and interaction model.

### 2.1 Planetary Surface View
- **Camera:** Top-down isometric / free-rotate, zoomable from individual machine level to full-base overview.
- **Primary Interaction:** Grid-based placement of buildings, belts, pipes, arms. Click-drag for belt routing.
- **HUD Elements:**
  - Toolbar (bottom): categorized building palette with search & favorites.
  - Resource overlay (top-right): real-time throughput meters (items/min) for selected belts/machines.
  - Alert panel (top-left): warnings for bottlenecks, depleted nodes, power shortages.
  - Mini-map (bottom-right): full base overview with color-coded resource flow.

### 2.2 Orbital / System View
- **Camera:** 3D orbital view centered on the current planet/moon. Zoom out to see the full star system as a stylized orrery.
- **Primary Interaction:** Click celestial bodies to inspect. Drag to create freight routes between orbital stations.
- **HUD Elements:**
  - Sidebar (left): list of owned orbital stations and active freight routes.
  - Route editor (center modal): waypoint sequencing, fuel cost estimator, cargo manifest.
  - Threat overlay (toggle): shows hostile fleet positions, Silica Swarm activity zones.

### 2.3 Galaxy Map View
- **Camera:** 2D stylized star map with 3D depth hints. Scroll/zoom to navigate.
- **Primary Interaction:** Click systems for info panel. Right-click for context actions (set destination, view sovereignty, check market prices).
- **HUD Elements:**
  - Filter bar (top): toggle overlays — sovereignty, resource richness, threat level, trade volume.
  - Route planner (right panel): multi-jump route with fuel/time estimates.
  - Sovereignty legend: color-coded alliance territories.

### 2.4 Fleet Command View
- **Camera:** Tactical 3D view centered on your fleet flagship. Zoom from ship-detail to full engagement overview.
- **Primary Interaction:** Select ships/groups → right-click targets to assign orders (orbit, approach, align, warp). Module activation via hotbar.
- **HUD Elements:**
  - Ship status panel (left): selected ship's armor, shields, capacitor, cargo, heat.
  - Fleet composition (right): ship-type groupings with health bars.
  - Tactical overlay: range rings, optimal tracking arcs, missile flight paths.
  - Ammo/fuel gauge: real-time depletion of physical munitions from cargo.

---

## 3. Key UI Screens & Panels

### 3.1 Main Menu / Character Selection
- Minimalist: rotating view of the player's last docked station or base.
- Character select (single character per account in single-shard).
- Server status indicator, patch notes panel.

### 3.2 Factory Planner (Planetary)
- **Building Palette:** Categorized grid (Extraction → Processing → Assembly → Logistics → Power → Defense → Research → Services). Categories expand as the player encounters new activities.
- **Ghost Placement:** Semi-transparent preview snaps to grid. Color-coded validity (green = valid, red = collision/missing prerequisite).
- **Belt/Pipe Routing:** Click start → drag path → click end. Auto-routing with manual override. Elevation changes for over/underpasses.
- **Copy/Paste & Blueprints:** Selection box → copy region → paste as ghost. Save selection as Blueprint with metadata (name, tags, description).
- **Statistics Dashboard:** Accessible via hotkey. Shows production/consumption graphs over time for all resources, power draw curves, throughput bottlenecks highlighted in red.

### 3.3 Blueprint Market
- **Browse/Search:** Filter by category, rating, author, price.
- **Preview:** 3D rotating preview of the blueprint layout with resource requirements listed.
- **Purchase/Download:** One-click buy → blueprint appears in personal library.
- **Publish:** Upload from personal library with pricing (one-time or royalty per use).

### 3.4 Logistics Manager (Interplanetary)
- **Route List:** Table view of all active freight routes with status indicators (active, delayed, under attack).
- **Route Designer:** Visual editor — click origin station → click destination → set cargo filters, scheduling (continuous / on-demand / threshold-triggered).
- **Fleet Assignment:** Drag freighter ships onto routes. Shows capacity utilization.

### 3.5 Market & Economy
- **Commodity Exchange:** Real-time order book (bid/ask) per item per station. Depth chart visualization. Price history graphs (1h, 24h, 7d, 30d).
- **Contract Board:** Browse/post courier contracts, manufacturing orders, mercenary bounties. Filter by type, reward, distance.
- **Corporation Finance:** Balance sheet, income statement, shareholder registry, dividend controls.
- **Stock Market:** Ticker-style display for public corporations. Buy/sell share orders. Hostile takeover progress bar.

### 3.6 Corporation & Alliance Management
- **Org Chart:** Visual hierarchy — CEO → Directors → Managers → Members. Drag to reorganize.
- **Role & Permissions:** Granular permission matrix (hangar access, wallet access, fleet command, blueprint library access).
- **Diplomacy Panel:** Alliance standings (allied, neutral, hostile). Treaty editor for NAPs, mutual defense pacts, trade agreements.

### 3.7 Politics & Governance
- **System Governance Panel:** Shows current tax rates, elected council members, upcoming elections.
- **Voting Interface:** Candidate list with platforms. Cast vote with confirmation.
- **Customs Office Manager:** Set tariff rates, view revenue logs, exempt lists.

### 3.8 Research & Tech Tree
- **Tech Tree Viewer:** Zoomable node graph. Completed nodes glow, available nodes highlighted, locked nodes dimmed with prerequisite paths shown.
- **Research Queue:** Drag techs into queue. Shows resource cost, time estimate, and current feed-rate of research materials.
- **Infinite Upgrades Panel:** Slider/counter for marginal upgrades with exponential cost curve displayed.

### 3.9 Ship Fitting
- **Ship Model (center):** 3D model with hardpoint slots highlighted.
- **Module Inventory (left):** Available modules from station hangar, filterable.
- **Fitting Stats (right):** Power grid usage, CPU usage, capacitor stats, DPS, tank, speed. Real-time updates as modules are dragged onto slots.
- **Simulation Mode:** "Test fit" button — runs a simulated engagement to show effective DPS at range, tank duration, cap stability.

---

## 4. Interaction Patterns

### 4.1 Context Menus
Right-click is the universal action menu. Context-sensitive based on target:
- **Planet:** Land, scan resources, view factories, set as waypoint.
- **Ship:** Dock, target, orbit, approach, invite to fleet, inspect cargo.
- **Player:** Trade, message, invite to corp, view profile, set standing.
- **Market Order:** Buy, modify, cancel, view seller info.

### 4.2 Drag & Drop
- Modules onto ship slots (fitting).
- Items between inventories (hangar ↔ cargo ↔ market).
- Ships onto fleet groups or logistics routes.
- Buildings from palette onto the planetary grid.

### 4.3 Hotkeys & Shortcuts
- Fully rebindable. Defaults inspired by common RTS/factory-sim conventions.
- Function key groups for fleet commands (F1–F5 = fleet groups).
- Number keys for module activation in combat.
- Q/E for belt direction toggle. R for rotate building.

### 4.4 Notifications & Alerts
- **Toast Notifications:** Slide in from top-right, auto-dismiss. Low-priority (trade completed, research finished).
- **Persistent Alerts:** Pinned to alert panel until resolved. Medium-priority (factory bottleneck, route delayed).
- **Full-Screen Warnings:** Red border flash + audio klaxon. High-priority (base under attack, siege detected, hostile fleet on grid).
- **Offline Notifications:** Push notifications (mobile companion app or email) for critical events (structure attacked, hostile takeover vote initiated).

---

## 5. Onboarding & New Player Experience

### 5.1 Guided Tutorial (The "First Drill" Sequence)

Mining is the universal entry point — it teaches the foundational mechanics (grid placement, belt routing, production chains) that underpin nearly every other activity. After the tutorial, the game opens up and players naturally gravitate toward whatever interests them: trading, combat, hauling, politics, espionage, research, or deeper industrial optimization.

1. **Landing:** Player spawns on a small asteroid in a Precursor Cradle (safe zone). Cinematic flydown.
2. **Hand-Drill:** Prompted to manually mine a copper node. Teaches basic interaction.
3. **First Machine:** Build a mining drill on the node. Teaches grid placement.
4. **First Belt:** Connect drill to a smelter via conveyor. Teaches belt routing.
5. **First Product:** Smelt copper into plates. Teaches production chains.
6. **First Ship Component:** Manufacture a basic hull plate. Teaches the "everything is player-made" philosophy.
7. **Launch:** Build enough components to upgrade the starter shuttle. Player can now fly to adjacent systems.
8. **Choose Your Path:** After docking at the first station, the player is introduced to the breadth of activities — market board, contract board, recruitment ads, bounty board — and is free to pursue whatever catches their interest.

### 5.2 Progressive Disclosure
- UI panels unlock as the player encounters relevant systems (e.g., market panel activates on first station dock; fleet panel activates on first ship-to-ship encounter; governance panel activates on joining an alliance).
- This ensures the UI grows with the player's interests — a dedicated trader never needs to see combat fitting screens cluttering their interface until they choose to engage with combat.
- Tooltip system with three tiers: hover = one-line summary, hold = detailed description, click = wiki link.

### 5.3 Mentor System
- Experienced players can opt into a "Mentor" role. New players are matchmade with mentors who receive small in-game rewards for engagement.

---

## 6. Accessibility Considerations

- **Colorblind Modes:** All overlays and indicators use shape + color, not color alone. Selectable palettes (Deuteranopia, Protanopia, Tritanopia).
- **UI Scaling:** Full HUD scaling from 80%–150%.
- **Font Options:** Dyslexia-friendly font toggle.
- **Audio Cues:** All critical visual alerts have corresponding audio cues.
- **Key Remapping:** Full rebindability including mouse buttons and modifier combinations.
- **Screen Reader Hints:** Semantic labels on all interactive elements for screen reader compatibility.

---

## 7. Responsiveness & Performance

- **UI Framerate Target:** UI rendering decoupled from game world at 60fps minimum regardless of world complexity.
- **Lazy Loading:** Panels not currently visible do not tick or fetch data. Market data fetched on-open, not continuously.
- **Streaming Data:** Market order books and fleet positions stream via delta updates, not full refreshes.
- **LOD for UI:** At maximum zoom-out in factory view, individual item icons on belts are replaced with flow-direction arrows and throughput numbers.
