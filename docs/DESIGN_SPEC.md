# Game Design Specification: Infinite Flux

## Meta-Directive: Project Goals & LLM Instructions

### Developer Objective
The primary purpose of **Infinite Flux** is to serve as a practical, large-scale vehicle for the developer to learn and master the Rust programming language.

### LLM Instructions
Any AI or LLM assisting with this project must operate in **Pair-Programming Mode**. This means:
- **Explain why:** Do not just write the code; explain why it is written that way in idiomatic Rust.
- **Teaching Focus:** Focus heavily on teaching Rust concepts (borrow checker, lifetimes, traits, concurrency) as they apply to the game's architecture.
- **Expert Suggestions:** Suggest appropriate Rust crates (e.g., Bevy for ECS, Tokio for networking) and architectural patterns suitable for a high-performance simulation.

---

## Elevator Pitch
A single-shard MMO where players start with a hand-drill on a barren asteroid and grow into whatever they choose to become — factory magnates, fleet commanders, market traders, freight haulers, political leaders, corporate spies, or any combination. Every bullet fired, ship flown, and factory built is manufactured by a player. The ultimate goal is infinite optimization, galactic economic dominance, and territorial supremacy.

---

## Emergent Player Activities

There is no class system or formal occupation selection in Infinite Flux. **Players become what they do.** A player who spends their time building factories *is* a manufacturer. A player who runs freight routes *is* a hauler. Identity emerges from practice, not from a menu.

Mining is the universal starting point — every player begins with a hand-drill on an asteroid, learning the foundational mechanics of extraction and production. But the game opens up rapidly from there. The breadth of activities a player can pursue includes:

| Activity | Description |
| :--- | :--- |
| **Mining & Extraction** | Harvesting raw materials from planetary nodes — the entry point for all players. |
| **Manufacturing** | Designing and optimizing factory layouts to process raw materials into components, ships, ammunition, and equipment. |
| **Trading & Speculation** | Buying low and selling high across station markets. Analyzing price histories, anticipating demand shifts, cornering commodity markets. |
| **Hauling & Logistics** | Running freight routes — either manually or via automated fleets — moving goods between stations for profit. |
| **Combat & Mercenary Work** | Fleet command, bounty hunting, system defense, or hired muscle. Fighting for pay or for territory. |
| **Research & Science** | Feeding research hubs with manufactured materials to unlock new technologies for a corporation. |
| **Politics & Governance** | Running for elected office, setting tax policy, managing alliances, negotiating treaties. |
| **Espionage & Intelligence** | Infiltrating rival corporations, stealing blueprints, sabotaging defenses, selling information. |
| **Banking & Finance** | Running a player bank, issuing loans, managing a corporation's IPO, or orchestrating hostile takeovers on the stock market. |
| **Exploration & Scouting** | Charting new systems, identifying resource-rich planets, mapping hostile territory for allied fleets. |

These activities are not mutually exclusive. A player might manufacture during peacetime and switch to combat during a siege. A trader might dabble in espionage to get an information edge. **The game does not constrain players to a single path — the economy and social systems reward specialization naturally**, because doing one thing well is more profitable than doing everything poorly.

### Skill Progression: Diminishing Returns

Every activity has an associated skill that improves through practice. There are no XP tables, quest turn-ins, or skill point menus — **you get better at what you do**.

**Core mechanic:** Skill effectiveness follows a **diminishing returns curve**. Early levels give large bonuses; later levels give progressively smaller ones.

```
Skill Bonus = f(level) — a sublinear curve (e.g., logarithmic or square-root)

Level  1  →  Bonus ~1.0   (baseline — new player is immediately useful)
Level 10  →  Bonus ~3.2   (rapid early gains — new players ramp up fast)
Level 50  →  Bonus ~7.1   (veteran has a meaningful edge)
Level 100 →  Bonus ~10.0  (diminishing gap — a level-50 player is ~70% as effective as a level-100)
```

*(Exact formula will be tuned during implementation. Values above are illustrative using a square-root curve.)*

**Design intent:**
- **New-player friendly:** A fresh player can become useful in a few hours of play, not weeks of grinding. This lowers the barrier to entry for the single-shard economy — new haulers, traders, and fighters are always entering the market.
- **No hard ceiling, but soft plateau:** There is no maximum level, but the gains flatten so aggressively that grinding past a certain point is not worth it compared to broadening into other skills.
- **Breadth over depth:** Since deep specialization yields diminishing returns, players are naturally encouraged to diversify — which creates more well-rounded participants in the economy and more interesting emergent gameplay.
- **No power gap:** A dedicated new player can compete with a veteran in a specific activity within a reasonable timeframe. Veterans have breadth and experience, not overwhelming statistical dominance.

**What skills do NOT control:**
- Skills do not gate access to content. A level-1 miner can mine anywhere — they're just slower.
- Skills do not unlock recipes, ships, or equipment. Technology progression is handled by the Research & Tech Tree system (corporation-level, not individual).
- Skills do not grant exclusive abilities. There is no "only a level-50 Spy can do espionage."

---

## 1. Pillar One: Industry & Automation
The moment-to-moment gameplay on a planetary or orbital scale plays like a deep automation simulator. The goal is continuous replayability through infinite optimization.

*   **Planetary Construction:** Players seamlessly land on planets, asteroids, and moons to build factory networks. You lay conveyor belts, pipes, robotic arms, and assembly machines to harvest and process raw materials.
*   **Interplanetary Logistics:** The "conveyor belt" extends into space. Players design automated freight routes, optimizing for fuel consumption, warp-spooling times, and cargo capacity.
*   **Blueprint Ecosystem:** Players can design highly optimized factory layouts, save them as Blueprints, and either share them internally with your corporation or sell them on the open market for royalties.
*   **Continuous Replayability:** Resource nodes slowly deplete or shift over time. A factory is never truly "finished"; it must constantly be torn down, rebuilt, and optimized to meet shifting market demands and changing supply lines.

---

## 2. Pillar Two: The Galactic Economy
The economy is a fully functional, unregulated financial sector where information is as valuable as titanium. There are no NPC vendors selling end-game gear.

*   **Smart Logistics Contracts:** Mining corporations can post public "Courier Contracts." Specialized player-run Freight Corporations take these jobs, using heavily optimized, automated trade routes to move goods for a cut of the profit.
*   **The Stock Market & Hostile Takeovers:** Mega-corporations can go public to raise capital. However, rival alliances can quietly buy up a controlling interest in your company on the open market, stealing your infrastructure without firing a single shot.
*   **Banking & Asset Seizure:** Credits have a physical data-drive equivalent when moved in bulk. Player-run banks can offer loans for factory start-ups. If a debtor defaults, the bank can post mercenary bounties to forcibly seize the player's automated assets.

---

## 3. Pillar Three: Politics & Governance
Space is vast, but valuable real estate is scarce. Players must form governments to control territory, and those governments will inevitably clash.

*   **Territorial Taxation:** When an alliance claims a star system, they control the infrastructure. Alliances can set up automated Customs Offices on warp gates, skimming a physical percentage of cargo (or a credit fee) from anyone passing through.
*   **Player-Elected Councils:** In safe systems, tax rates, market transaction fees, and the deployment of environmental shields are determined by a parliament of elected player delegates.
*   **Corporate Espionage:** Players can infiltrate enemy corporations to steal proprietary Factory Blueprints, leak fleet movement data, or quietly alter the logic circuits in an enemy's automated defense grid so it shuts down during an invasion.

---

## 4. Pillar Four: Warfare & Conflict (Tactical & Logistical)
While wars are ultimately decided by who has the deepest supply chain, battles are won by tactical brilliance. Combat is not an arcade shooter; it is a methodical, Real-Time Strategy (RTS) experience.

*   **Tactical Piloting:** You are a fleet commander, not a fighter pilot. Combat involves locking targets, setting optimal orbit distances, managing power grids, and timing module activations. You set the tactics, and the ship executes them.
*   **The Ship as a System:** Your vessel operates like a miniature, mobile factory. You must manage heat dissipation, capacitor energy flow, and physical ammunition feeds. Firing a barrage of heavy missiles means watching your physical cargo hold deplete in real-time.

### The Astro-Topography: Environments of Risk
Player safety is dictated by environmental physics and alien ecology, naturally separating playstyles without breaking immersion.

| Zone Type | The Environment | Rules of Engagement | Core Gameplay Loop |
| :--- | :--- | :--- | :--- |
| **The Precursor Cradles** | Ancient shielded sectors bathed in an EM Dampening Field. | Ship weapons and offensive modules cannot draw power. PvP is impossible. | **Pure Builder:** Safe sandbox. Extremely limited real estate and low yields require mathematical perfection to turn a profit. |
| **The Fracture Sectors** | Resource-rich systems infested by the "Silica Swarm" (rogue machine race). | Firing weapons at players aggros Leviathan-class Swarm hives that wipe out everyone. | **PvE / Logistics:** Build heavily fortified factories to withstand endless alien waves. Clear orbital hives to secure mining claims. |
| **The Silent Void** | Dead, quiet space with raw physics and the richest exotic nodes in the galaxy. | Zero restrictions. Free-for-all. | **PvP / Empire Building:** Alliances wage war over territory and choke points. Massive server-altering sieges occur here. |

### The Anatomy of a Siege
Warfare relies on the physical constraints of both fleet tactics and factory mechanics. Destruction is permanent.

1.  **Blockade:** Attacker deploys a fleet to interdict warp gates. Defender reroutes supply chains and powers down non-essential factories to save fuel.
2.  **Orbital Siege:** Attacker positions dreadnoughts to clear orbital platforms. Defender fires automated planetary artillery and scrambles interceptor drones.
3.  **Ground Assault:** Attacker constructs a Forward Operating Base (FOB) factory to produce local ammo. Defender reroutes all production to repair bots and shields.
4.  **The Breach:** Attacker overwhelms the power-grid with sustained fleet DPS. Shields fall when the defender's factory physically runs out of fuel or coolant.

---

## 5. Pillar Five: Technology & Progression
There are no traditional "XP levels." Individual progression is through use-based skills with diminishing returns (see *Skill Progression* above). Collective progression is purely technological and material.

*   **Research Hubs:** Science requires massive amounts of manufactured items fed into server-wide or corporation-wide research facilities.
*   **Infinite Tech Tree:** Corporations can invest in infinite marginal upgrades (e.g., +1% Conveyor Speed, +0.5% Missile Tracking) that cost exponentially more resources, ensuring a late-game material sink.
*   **Megastructures:** Late-game goals involve server-altering projects, like corporations building Dyson Spheres to power entire sectors or artificial wormholes to bypass dangerous space.
