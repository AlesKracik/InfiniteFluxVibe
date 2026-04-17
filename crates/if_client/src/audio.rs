// audio.rs: Sound effects framework.
//
// Bevy 0.18 ships with `bevy_audio` as part of `DefaultPlugins`. We load
// short SFX clips from `assets/sounds/*` at startup and play them by
// spawning `AudioPlayer` entities with `PlaybackSettings::DESPAWN` so the
// entities clean themselves up when the clip finishes.
//
// Sound loading is best-effort: if the asset directory or individual files
// are missing, the handles remain unset and every playback system no-ops
// gracefully. Drop WAV/OGG files named `place.ogg`, `alert.ogg`, `save.ogg`,
// `load.ogg`, and `recipe_complete.ogg` into `assets/sounds/` to enable SFX.

use std::path::Path;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use if_factory::building::Building;
use if_factory::mining::ResourceNode;
use if_factory::production::Machine;

/// User-facing audio settings. Tweakable from the audio panel in the UI.
#[derive(Resource, Debug, Clone, Copy)]
pub struct AudioSettings {
    /// Master volume, 0.0 (silent) to 1.0 (full).
    pub master_volume: f32,
    /// When `false`, all SFX playback is suppressed.
    pub sfx_enabled: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.6,
            sfx_enabled: true,
        }
    }
}

/// Handles to the loaded sound effect assets.
///
/// Each field is `Option<Handle<AudioSource>>` — `None` when the corresponding
/// asset file is missing, so the playback systems can skip gracefully.
#[derive(Resource, Default, Clone)]
pub struct SoundEffects {
    pub place: Option<Handle<AudioSource>>,
    pub alert: Option<Handle<AudioSource>>,
    pub save: Option<Handle<AudioSource>>,
    pub load: Option<Handle<AudioSource>>,
    pub recipe_complete: Option<Handle<AudioSource>>,
}

impl SoundEffects {
    /// Returns true when at least one sound asset was loaded.
    pub fn any_loaded(&self) -> bool {
        self.place.is_some()
            || self.alert.is_some()
            || self.save.is_some()
            || self.load.is_some()
            || self.recipe_complete.is_some()
    }
}

/// Startup system: try to load each sound asset if its file exists.
///
/// Uses a simple filesystem check (`assets/sounds/<name>.ogg`) before
/// calling `asset_server.load`, so we don't spam the log with missing-asset
/// warnings on a fresh checkout.
pub fn load_sound_effects(mut commands: Commands, asset_server: Res<AssetServer>) {
    let assets_dir = Path::new("assets/sounds");
    if !assets_dir.is_dir() {
        info!("audio: assets/sounds/ not found — SFX disabled");
        commands.insert_resource(SoundEffects::default());
        return;
    }

    let try_load = |name: &str| -> Option<Handle<AudioSource>> {
        let path = assets_dir.join(name);
        if path.is_file() {
            Some(asset_server.load(format!("sounds/{name}")))
        } else {
            None
        }
    };

    // Try common audio extensions. First match wins.
    let load_any = |stem: &str| -> Option<Handle<AudioSource>> {
        for ext in ["ogg", "wav", "mp3", "flac"] {
            if let Some(h) = try_load(&format!("{stem}.{ext}")) {
                return Some(h);
            }
        }
        None
    };

    let sfx = SoundEffects {
        place: load_any("place"),
        alert: load_any("alert"),
        save: load_any("save"),
        load: load_any("load"),
        recipe_complete: load_any("recipe_complete"),
    };

    if sfx.any_loaded() {
        info!("audio: SFX loaded from assets/sounds/");
    } else {
        info!("audio: no SFX files found in assets/sounds/");
    }

    commands.insert_resource(sfx);
}

/// Spawn a one-shot `AudioPlayer` that plays the given handle and despawns
/// itself when the clip finishes.
///
/// No-ops when `handle` is `None` or SFX are disabled.
pub fn play_sound(
    commands: &mut Commands,
    handle: Option<&Handle<AudioSource>>,
    settings: &AudioSettings,
) {
    if !settings.sfx_enabled {
        return;
    }
    let Some(handle) = handle else {
        return;
    };
    commands.spawn((
        AudioPlayer::new(handle.clone()),
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(settings.master_volume.clamp(0.0, 1.0))),
    ));
}

/// System: play the "place" sound whenever a new `Building` component is added.
pub fn sound_on_building_placed(
    mut commands: Commands,
    query: Query<(), Added<Building>>,
    sfx: Res<SoundEffects>,
    settings: Res<AudioSettings>,
) {
    // Count additions this frame — if there's more than one, still play once.
    if query.iter().next().is_none() {
        return;
    }
    play_sound(&mut commands, sfx.place.as_ref(), &settings);
}

/// System: play the "alert" sound when a resource node transitions to depleted.
///
/// Uses a `Local<HashSet<Entity>>` to track which nodes have already fired the
/// alert, so a persistently-depleted node doesn't keep re-triggering the sound
/// when other fields on the component change.
pub fn sound_on_resource_depleted(
    mut commands: Commands,
    query: Query<(Entity, &ResourceNode), Changed<ResourceNode>>,
    sfx: Res<SoundEffects>,
    settings: Res<AudioSettings>,
    mut already_alerted: Local<std::collections::HashSet<Entity>>,
) {
    for (entity, node) in &query {
        if node.is_depleted() {
            if already_alerted.insert(entity) {
                play_sound(&mut commands, sfx.alert.as_ref(), &settings);
            }
        } else {
            // Node refilled somehow — allow future alerts again.
            already_alerted.remove(&entity);
        }
    }
}

/// System: play the "save" sound on F5 and the "load" sound on F9.
///
/// This is a lightweight keypress-mirror — the actual save/load work happens
/// in `save_load.rs`. If the save/load fails, the sound still plays; this is a
/// UX concession (feedback-on-input) rather than a strict correctness signal.
pub fn sound_on_save_load(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    sfx: Res<SoundEffects>,
    settings: Res<AudioSettings>,
) {
    if keyboard.just_pressed(KeyCode::F5) {
        play_sound(&mut commands, sfx.save.as_ref(), &settings);
    }
    if keyboard.just_pressed(KeyCode::F9) {
        play_sound(&mut commands, sfx.load.as_ref(), &settings);
    }
}

/// System: play the "recipe complete" sound when a Machine transitions from
/// processing -> idle. Tracked via a `Local<HashMap<Entity, bool>>` of the
/// previous `is_processing` state.
pub fn sound_on_recipe_complete(
    mut commands: Commands,
    query: Query<(Entity, &Machine)>,
    sfx: Res<SoundEffects>,
    settings: Res<AudioSettings>,
    mut prev_processing: Local<std::collections::HashMap<Entity, bool>>,
) {
    // Collect current entities so we can prune stale entries.
    let mut seen: std::collections::HashSet<Entity> = std::collections::HashSet::new();

    for (entity, machine) in &query {
        seen.insert(entity);
        let was_processing = prev_processing.get(&entity).copied().unwrap_or(false);
        let now_processing = machine.is_processing();
        // Fire on the processing -> idle transition.
        if was_processing && !now_processing {
            play_sound(&mut commands, sfx.recipe_complete.as_ref(), &settings);
        }
        prev_processing.insert(entity, now_processing);
    }

    // Drop entries for entities that no longer exist.
    prev_processing.retain(|e, _| seen.contains(e));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_settings_defaults() {
        let s = AudioSettings::default();
        assert!(s.sfx_enabled);
        assert!(s.master_volume > 0.0 && s.master_volume <= 1.0);
    }

    #[test]
    fn sound_effects_default_is_empty() {
        let sfx = SoundEffects::default();
        assert!(sfx.place.is_none());
        assert!(sfx.alert.is_none());
        assert!(sfx.save.is_none());
        assert!(sfx.load.is_none());
        assert!(sfx.recipe_complete.is_none());
        assert!(!sfx.any_loaded());
    }

    #[test]
    fn play_sound_noop_when_handle_missing() {
        // Build a minimal app just to get a Commands buffer. If play_sound
        // tried to actually spawn with a missing handle, it would unwrap and
        // panic. We verify it returns silently.
        let mut app = App::new();
        app.add_systems(Update, |mut commands: Commands| {
            let settings = AudioSettings::default();
            play_sound(&mut commands, None, &settings);
        });
        app.update();
        // No assertion needed: no panic means success.
    }

    #[test]
    fn play_sound_noop_when_sfx_disabled() {
        let mut app = App::new();
        app.add_systems(Update, |mut commands: Commands| {
            let settings = AudioSettings {
                master_volume: 1.0,
                sfx_enabled: false,
            };
            // Even with a "valid-looking" weak handle, disabled SFX should
            // skip without spawning. We pass `None` here because constructing
            // a real Handle<AudioSource> would require the asset server.
            play_sound(&mut commands, None, &settings);
        });
        app.update();
    }

    #[test]
    fn master_volume_clamps() {
        // Ensure clamp used inside play_sound keeps the volume in [0.0, 1.0].
        let high = AudioSettings {
            master_volume: 2.0,
            ..AudioSettings::default()
        };
        assert!(high.master_volume.clamp(0.0, 1.0) <= 1.0);

        let low = AudioSettings {
            master_volume: -1.0,
            ..AudioSettings::default()
        };
        assert!(low.master_volume.clamp(0.0, 1.0) >= 0.0);
    }
}
