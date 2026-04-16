// notifications.rs: Temporary notification messages displayed at the top of the screen.
//
// Notifications are short-lived messages that inform the player about events
// like building placement, resource depletion, and power shortages. They
// fade out after a few seconds.

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use if_factory::mining::ResourceNode;
use if_factory::power::PowerGrid;

/// How long a notification stays visible (seconds).
const NOTIFICATION_LIFETIME: f32 = 5.0;

/// Maximum number of notifications displayed at once.
const MAX_VISIBLE: usize = 5;

/// A single notification message with a timestamp for expiry.
#[derive(Clone, Debug)]
pub struct Notification {
    pub message: String,
    pub kind: NotificationKind,
    /// Time (in seconds since app start) when this notification was created.
    pub created_at: f32,
}

/// Visual category for notifications (affects color).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Warning,
    Alert,
}

impl NotificationKind {
    fn color(self) -> egui::Color32 {
        match self {
            NotificationKind::Info => egui::Color32::from_rgb(180, 220, 255),
            NotificationKind::Warning => egui::Color32::from_rgb(255, 220, 100),
            NotificationKind::Alert => egui::Color32::from_rgb(255, 100, 100),
        }
    }
}

/// Resource holding the queue of active notifications.
#[derive(Resource, Default)]
pub struct Notifications {
    queue: VecDeque<Notification>,
}

impl Notifications {
    /// Push a new notification.
    pub fn push(&mut self, message: String, kind: NotificationKind, time: f32) {
        self.queue.push_back(Notification {
            message,
            kind,
            created_at: time,
        });
        // Cap the queue size to prevent unbounded growth
        while self.queue.len() > 20 {
            self.queue.pop_front();
        }
    }

    /// Remove expired notifications and return those still visible.
    fn visible(&mut self, current_time: f32) -> Vec<Notification> {
        // Remove expired
        while let Some(front) = self.queue.front() {
            if current_time - front.created_at > NOTIFICATION_LIFETIME {
                self.queue.pop_front();
            } else {
                break;
            }
        }
        // Return up to MAX_VISIBLE from the back (most recent)
        self.queue
            .iter()
            .rev()
            .take(MAX_VISIBLE)
            .rev()
            .cloned()
            .collect()
    }
}

/// System: render notifications at the top-center of the screen.
pub fn notification_display_system(
    mut contexts: EguiContexts,
    mut notifications: ResMut<Notifications>,
    time: Res<Time>,
    mut warmup: Local<u8>,
) {
    if *warmup < 3 {
        *warmup += 1;
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let current_time = time.elapsed_secs();
    let visible = notifications.visible(current_time);

    if visible.is_empty() {
        return;
    }

    egui::Area::new(egui::Id::new("notifications"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                for notif in &visible {
                    let age = current_time - notif.created_at;
                    // Fade out in the last second
                    let alpha = if age > NOTIFICATION_LIFETIME - 1.0 {
                        ((NOTIFICATION_LIFETIME - age) / 1.0).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };

                    let base_color = notif.kind.color();
                    let color = egui::Color32::from_rgba_unmultiplied(
                        base_color.r(),
                        base_color.g(),
                        base_color.b(),
                        (alpha * 255.0) as u8,
                    );

                    let bg_alpha = (alpha * 180.0) as u8;
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, bg_alpha))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(12, 4))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&notif.message).color(color).size(13.0));
                        });
                    ui.add_space(2.0);
                }
            });
        });
}

/// System: fire "Building placed" notifications when buildings are placed.
///
/// Detects newly added Building components.
pub fn notify_building_placed(
    query: Query<&if_factory::building::Building, Added<if_factory::building::Building>>,
    mut notifications: ResMut<Notifications>,
    time: Res<Time>,
) {
    for building in &query {
        notifications.push(
            format!("Building placed: {}", building.building_type),
            NotificationKind::Info,
            time.elapsed_secs(),
        );
    }
}

/// System: fire "Resource depleted!" notifications when a ResourceNode hits 0.
///
/// Uses a Changed filter to only fire when remaining actually changes.
pub fn notify_resource_depleted(
    query: Query<&ResourceNode, Changed<ResourceNode>>,
    mut notifications: ResMut<Notifications>,
    time: Res<Time>,
) {
    for node in &query {
        if node.is_depleted() {
            notifications.push(
                format!("Resource depleted! ({} node exhausted)", node.resource),
                NotificationKind::Alert,
                time.elapsed_secs(),
            );
        }
    }
}

/// System: fire "Power shortage!" notifications when power_ratio drops below 50%.
///
/// Uses a Local to track previous state and avoid spamming every frame.
pub fn notify_power_shortage(
    power_grid: Res<PowerGrid>,
    mut notifications: ResMut<Notifications>,
    time: Res<Time>,
    mut was_low: Local<bool>,
) {
    let is_low = power_grid.power_ratio < 0.5 && power_grid.total_consumption > 0.0;

    if is_low && !*was_low {
        let pct = (power_grid.power_ratio * 100.0) as u32;
        notifications.push(
            format!("Power shortage! {pct}% efficiency"),
            NotificationKind::Warning,
            time.elapsed_secs(),
        );
    }
    *was_low = is_low;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_push_and_visible() {
        let mut notifs = Notifications::default();
        notifs.push("test".to_string(), NotificationKind::Info, 0.0);
        assert_eq!(notifs.visible(0.5).len(), 1);
    }

    #[test]
    fn notification_expires_after_lifetime() {
        let mut notifs = Notifications::default();
        notifs.push("old".to_string(), NotificationKind::Info, 0.0);
        let visible = notifs.visible(NOTIFICATION_LIFETIME + 1.0);
        assert!(visible.is_empty());
    }

    #[test]
    fn notification_max_visible_cap() {
        let mut notifs = Notifications::default();
        for i in 0..10 {
            notifs.push(format!("msg {i}"), NotificationKind::Info, i as f32);
        }
        let visible = notifs.visible(4.5);
        assert!(visible.len() <= MAX_VISIBLE);
    }

    #[test]
    fn notification_queue_cap() {
        let mut notifs = Notifications::default();
        for i in 0..30 {
            notifs.push(format!("msg {i}"), NotificationKind::Info, i as f32);
        }
        // Queue should be capped at 20
        assert!(notifs.queue.len() <= 20);
    }

    #[test]
    fn notification_kind_colors_differ() {
        let info = NotificationKind::Info.color();
        let warning = NotificationKind::Warning.color();
        let alert = NotificationKind::Alert.color();
        assert_ne!(info, warning);
        assert_ne!(warning, alert);
    }
}
