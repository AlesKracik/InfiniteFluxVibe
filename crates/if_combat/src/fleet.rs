// fleet.rs: Fleet membership and FC (fleet-commander) orders.
//
// A fleet groups multiple ship entities under one `commander` and lets the
// FC broadcast tactical commands (assemble, engage, warp) that AI ships or
// UI-driven autopilots consume. The component itself is a small record
// held on a "fleet" entity or alongside the commander — either works; we
// don't enforce a particular placement.

use bevy::ecs::message::Message;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Fleet record. `commander` and `members` are `Entity` references and so
/// are skipped by serde — save/load must rebuild the linkage from names.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Default)]
pub struct Fleet {
    /// Fleet-wide id; the server rolls this and hands it out on creation.
    pub id: u32,
    pub name: String,
    #[serde(skip)]
    pub commander: Option<Entity>,
    #[serde(skip)]
    pub members: Vec<Entity>,
}

impl Fleet {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            commander: None,
            members: Vec::new(),
        }
    }

    /// Add a member. No-op if already present (idempotent).
    pub fn add_member(&mut self, entity: Entity) {
        if !self.members.contains(&entity) {
            self.members.push(entity);
        }
    }

    /// Remove a member. Returns true if the entity was in the fleet.
    pub fn remove_member(&mut self, entity: Entity) -> bool {
        if let Some(idx) = self.members.iter().position(|e| *e == entity) {
            self.members.remove(idx);
            true
        } else {
            false
        }
    }

    /// Convenience: is this entity a member?
    pub fn contains(&self, entity: Entity) -> bool {
        self.members.contains(&entity)
    }

    pub fn size(&self) -> usize {
        self.members.len()
    }
}

/// Commands a fleet commander can broadcast to the fleet. Listeners
/// (ship AI, autopilot) consume these via `MessageReader<FleetCommand>`.
#[derive(Message, Clone, Debug)]
pub enum FleetCommand {
    /// Regroup on the commander.
    Assemble,
    /// Focus fire this target.
    EngageTarget { target: Entity },
    /// Stand down; drop targets.
    Disengage,
    /// Warp the fleet to the named star system.
    WarpTo { system_index: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_entity(id: u32) -> Entity {
        Entity::from_raw_u32(id).unwrap()
    }

    #[test]
    fn new_fleet_has_no_members() {
        let f = Fleet::new(7, "Alpha");
        assert_eq!(f.id, 7);
        assert_eq!(f.name, "Alpha");
        assert_eq!(f.size(), 0);
        assert!(f.commander.is_none());
    }

    #[test]
    fn add_and_remove_members_is_idempotent() {
        let mut f = Fleet::new(1, "Test");
        let e1 = mk_entity(1);
        let e2 = mk_entity(2);
        f.add_member(e1);
        f.add_member(e1); // duplicate ignored
        f.add_member(e2);
        assert_eq!(f.size(), 2);
        assert!(f.contains(e1));
        assert!(f.remove_member(e1));
        assert!(!f.remove_member(e1)); // already gone
        assert!(!f.contains(e1));
        assert_eq!(f.size(), 1);
    }

    #[test]
    fn fleet_serde_skips_entity_refs() {
        let mut f = Fleet::new(42, "Bravo");
        f.commander = Some(mk_entity(5));
        f.add_member(mk_entity(10));
        f.add_member(mk_entity(11));
        let bytes = bincode::serialize(&f).unwrap();
        let restored: Fleet = bincode::deserialize(&bytes).unwrap();
        // Entity-valued fields drop on serde.
        assert!(restored.commander.is_none());
        assert_eq!(restored.members.len(), 0);
        // Identity fields survive.
        assert_eq!(restored.id, 42);
        assert_eq!(restored.name, "Bravo");
    }
}
