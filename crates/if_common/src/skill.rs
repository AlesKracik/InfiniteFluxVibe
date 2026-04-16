// skill.rs: Skill types and the diminishing-returns progression system.
//
// Skills improve through use. The bonus curve flattens as you level up,
// so new players ramp up quickly and veterans plateau. This keeps the
// power gap small and encourages breadth over deep specialization.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Every distinct skill in the game.
///
/// Skills map roughly to activities — mining ore improves Mining,
/// running a smelter improves Smelting, etc. There's no formal
/// occupation system; your "occupation" is just whichever skills
/// you've invested the most time in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillType {
    Mining,
    Smelting,
    Fabrication,
    Logistics,
}

impl fmt::Display for SkillType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkillType::Mining => write!(f, "Mining"),
            SkillType::Smelting => write!(f, "Smelting"),
            SkillType::Fabrication => write!(f, "Fabrication"),
            SkillType::Logistics => write!(f, "Logistics"),
        }
    }
}

/// A skill level — newtype wrapper around u32.
///
/// The inner value represents accumulated experience points. The "level"
/// and "bonus" are derived from this value through the diminishing
/// returns formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SkillLevel(u32);

/// How many XP points constitute one "level" for display purposes.
/// The actual bonus is continuous (not stepped), but levels give the
/// player a sense of progress.
const XP_PER_LEVEL: u32 = 100;

impl SkillLevel {
    pub fn new(xp: u32) -> Self {
        Self(xp)
    }

    /// Raw XP value.
    pub fn xp(&self) -> u32 {
        self.0
    }

    /// Display level (integer, for UI). This is just XP / XP_PER_LEVEL.
    pub fn level(&self) -> u32 {
        self.0 / XP_PER_LEVEL
    }

    /// Add XP from performing an activity.
    pub fn add_xp(&mut self, amount: u32) {
        self.0 = self.0.saturating_add(amount);
    }

    /// The bonus multiplier from this skill level.
    ///
    /// Uses a square-root curve for diminishing returns:
    ///   bonus = 1.0 + sqrt(level)
    ///
    /// Examples:
    ///   Level  0 → bonus 1.0  (baseline — new player)
    ///   Level  1 → bonus 2.0  (big early jump)
    ///   Level  4 → bonus 3.0
    ///   Level  9 → bonus 4.0  (slowing down)
    ///   Level 25 → bonus 6.0
    ///   Level 100→ bonus 11.0 (veteran — only ~5.5x a level-4 player)
    ///
    /// The 1.0 base means even a level-0 player has a 1x multiplier
    /// (not zero — everything works, just at baseline speed).
    pub fn bonus(&self) -> f32 {
        1.0 + (self.level() as f32).sqrt()
    }
}

impl fmt::Display for SkillLevel {
    /// Shows as "Lv.5 (1.23x)" — level and current bonus.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lv.{} ({:.2}x)", self.level(), self.bonus())
    }
}

/// Tracks a player's skill levels across all skill types.
///
/// Used as a Bevy `Resource` for single-player. Will be refactored to
/// a per-player `Component` when multiplayer is added.
#[derive(Resource, Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlayerSkills {
    skills: HashMap<SkillType, SkillLevel>,
}

impl PlayerSkills {
    /// Get a reference to the inner skills map (for serialization).
    pub fn skills_map(&self) -> &HashMap<SkillType, SkillLevel> {
        &self.skills
    }

    /// Create from a skills map (for deserialization).
    pub fn from_map(skills: HashMap<SkillType, SkillLevel>) -> Self {
        Self { skills }
    }
}

impl PlayerSkills {
    /// Get the bonus multiplier for a given skill type.
    /// Returns 1.0 (baseline) if the skill has never been used.
    pub fn get_bonus(&self, skill_type: SkillType) -> f32 {
        self.skills
            .get(&skill_type)
            .map_or(1.0, |level| level.bonus())
    }

    /// Add XP to a skill type.
    pub fn add_xp(&mut self, skill_type: SkillType, amount: u32) {
        self.skills.entry(skill_type).or_default().add_xp(amount);
    }

    /// Get the current skill level for a skill type.
    /// Returns a default (0 XP) level if the skill has never been used.
    pub fn get_level(&self, skill_type: SkillType) -> SkillLevel {
        self.skills.get(&skill_type).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_skill_is_level_zero() {
        let skill = SkillLevel::default();
        assert_eq!(skill.level(), 0);
        assert_eq!(skill.xp(), 0);
    }

    #[test]
    fn baseline_bonus_is_one() {
        let skill = SkillLevel::new(0);
        assert_eq!(skill.bonus(), 1.0);
    }

    #[test]
    fn bonus_increases_with_level() {
        let low = SkillLevel::new(1 * XP_PER_LEVEL);
        let mid = SkillLevel::new(25 * XP_PER_LEVEL);
        let high = SkillLevel::new(100 * XP_PER_LEVEL);

        assert!(low.bonus() < mid.bonus());
        assert!(mid.bonus() < high.bonus());
    }

    #[test]
    fn diminishing_returns() {
        // Going from level 0→1 should give a bigger boost than 99→100
        let gain_0_to_1 = SkillLevel::new(1 * XP_PER_LEVEL).bonus() - SkillLevel::new(0).bonus();
        let gain_99_to_100 = SkillLevel::new(100 * XP_PER_LEVEL).bonus()
            - SkillLevel::new(99 * XP_PER_LEVEL).bonus();

        assert!(
            gain_0_to_1 > gain_99_to_100,
            "Early levels should give bigger bonuses: 0→1 gained {gain_0_to_1}, 99→100 gained {gain_99_to_100}"
        );
    }

    #[test]
    fn add_xp_increases_level() {
        let mut skill = SkillLevel::default();
        assert_eq!(skill.level(), 0);

        skill.add_xp(XP_PER_LEVEL);
        assert_eq!(skill.level(), 1);

        skill.add_xp(XP_PER_LEVEL * 4);
        assert_eq!(skill.level(), 5);
    }

    #[test]
    fn display_format() {
        let skill = SkillLevel::new(4 * XP_PER_LEVEL);
        let display = format!("{skill}");
        assert_eq!(display, "Lv.4 (3.00x)");
    }

    #[test]
    fn new_player_vs_veteran_gap() {
        // A level-50 player should be at least 70% as effective as level-100.
        // This validates the "no power gap" design intent.
        let mid = SkillLevel::new(50 * XP_PER_LEVEL).bonus();
        let top = SkillLevel::new(100 * XP_PER_LEVEL).bonus();
        let ratio = mid / top;
        assert!(
            ratio >= 0.7,
            "Level 50 should be >=70% of level 100, got {ratio:.2} ({mid:.2} / {top:.2})"
        );
    }

    // --- PlayerSkills tests ---

    #[test]
    fn player_skills_default_bonus_is_one() {
        let skills = PlayerSkills::default();
        assert_eq!(skills.get_bonus(SkillType::Mining), 1.0);
        assert_eq!(skills.get_bonus(SkillType::Smelting), 1.0);
    }

    #[test]
    fn player_skills_add_xp_and_get_level() {
        let mut skills = PlayerSkills::default();
        skills.add_xp(SkillType::Mining, 250);
        assert_eq!(skills.get_level(SkillType::Mining).xp(), 250);
        assert_eq!(skills.get_level(SkillType::Mining).level(), 2);
        // Smelting should still be at 0
        assert_eq!(skills.get_level(SkillType::Smelting).level(), 0);
    }

    #[test]
    fn player_skills_bonus_increases_with_xp() {
        let mut skills = PlayerSkills::default();
        let before = skills.get_bonus(SkillType::Fabrication);
        skills.add_xp(SkillType::Fabrication, 400); // level 4
        let after = skills.get_bonus(SkillType::Fabrication);
        assert!(after > before, "Bonus should increase: {before} -> {after}");
        // Level 4 bonus = 1.0 + sqrt(4) = 3.0
        assert!((after - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn player_skills_accumulate_xp() {
        let mut skills = PlayerSkills::default();
        skills.add_xp(SkillType::Logistics, 50);
        skills.add_xp(SkillType::Logistics, 60);
        assert_eq!(skills.get_level(SkillType::Logistics).xp(), 110);
        assert_eq!(skills.get_level(SkillType::Logistics).level(), 1);
    }
}
