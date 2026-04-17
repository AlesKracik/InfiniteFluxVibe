// hud.rs: Simple text HUD showing current selection and controls.

use crate::orbital_view::ViewMode;
use crate::placement::BuildingPlacement;
use bevy::prelude::*;
use if_factory::power::PowerGrid;

/// Marker for the HUD text entity.
#[derive(Component)]
pub struct HudText;

/// Startup system: spawn a text display in the top-left corner.
pub fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        HudText,
    ));
}

/// Update system: refresh the HUD text each frame.
pub fn update_hud(
    selected: Res<BuildingPlacement>,
    power_grid: Res<PowerGrid>,
    view: Res<ViewMode>,
    mut hud_q: Query<&mut Text, With<HudText>>,
) {
    let Ok(mut text) = hud_q.single_mut() else {
        return;
    };

    let view_text = match *view {
        ViewMode::Surface => "View: Surface".to_string(),
        ViewMode::System => "View: System".to_string(),
    };

    let selection_text = match selected.building_type {
        Some(bt) => format!("Selected: {bt}"),
        None => "No selection".to_string(),
    };

    let power_text = if power_grid.total_generation == 0.0 && power_grid.total_consumption == 0.0 {
        "Power: No power grid".to_string()
    } else {
        let power_pct = (power_grid.power_ratio * 100.0) as u32;
        format!(
            "Power: {:.0}/{:.0} ({power_pct}%)",
            power_grid.total_generation, power_grid.total_consumption
        )
    };

    **text = format!(
        "{view_text}\n\
         {selection_text}\n\
         {power_text}\n\n\
         [1] Mining Drill\n\
         [2] Transport Line\n\
         [3] Smelter\n\
         [4] Assembler\n\
         [5] Generator\n\
         [Esc] Deselect\n\
         [Tab] Toggle stats\n\
         [M] Galaxy Map\n\
         [LMB] Place  [RMB] Remove\n\
         [WASD] Pan  [Scroll] Zoom"
    );
}
