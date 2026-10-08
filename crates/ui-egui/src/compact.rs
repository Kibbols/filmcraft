//! Compact (phone-width) layout. Below [`ENTER_WIDTH`] points the docked panel tree does not fit,
//! so the Edit mode shows one *view* at a time (a panel, or Program over Timeline) with a scrolling
//! tab bar along the bottom to switch between them. The dock tree itself is left alone: widen the
//! window and the saved workspace comes back exactly as it was.

use egui::{Rect, ScrollArea, Sense, Stroke, pos2, vec2};

use crate::FilmcraftApp;
use crate::dock::PanelKind;
use crate::panels;
use crate::theme::Tokens;

/// The window is compact below this width (points)...
pub const ENTER_WIDTH: f32 = 760.0;
/// ...and stays compact until it is this wide, so a window resized across the line doesn't flicker.
pub const LEAVE_WIDTH: f32 = 800.0;
/// Height of the bottom tab bar: a comfortable touch target.
pub const TAB_BAR_H: f32 = 52.0;
/// Smallest height of a control egui draws while compact (points).
const TOUCH_TARGET: f32 = 34.0;

/// One entry of the tab bar: panels stacked top to bottom, each with a share of the height.
pub struct View {
    pub label: &'static str,
    pub panels: &'static [(PanelKind, f32)],
}

pub const VIEWS: [View; 8] = [
    View { label: "Edit", panels: &[(PanelKind::Program, 0.42), (PanelKind::Timeline, 0.58)] },
    View { label: "Project", panels: &[(PanelKind::Project, 1.0)] },
    View { label: "Source", panels: &[(PanelKind::Source, 1.0)] },
    View { label: "Controls", panels: &[(PanelKind::EffectControls, 1.0)] },
    View { label: "Effects", panels: &[(PanelKind::Effects, 1.0)] },
    View { label: "Color", panels: &[(PanelKind::LumetriColor, 1.0)] },
    View { label: "Audio", panels: &[(PanelKind::AudioTrackMixer, 1.0)] },
    View { label: "Text", panels: &[(PanelKind::Text, 1.0)] },
];

/// Whether a window `width` points wide uses the compact layout, given whether it does now.
pub fn wants_compact(width: f32, currently: bool) -> bool {
    if !width.is_finite() {
        return currently;
    }
    if currently { width < LEAVE_WIDTH } else { width < ENTER_WIDTH }
}

/// Called once per frame with the window width: switches the layout and makes egui's own widgets
/// (buttons, fields, sliders) big enough for a finger while it is on.
pub fn update(app: &mut FilmcraftApp, ctx: &egui::Context, width: f32) {
    let now = wants_compact(width, app.compact);
    if now == app.compact {
        return;
    }
    app.compact = now;
    if now {
        ctx.global_style_mut(|s| {
            app.compact_saved_interact = Some(s.spacing.interact_size);
            s.spacing.interact_size.y = s.spacing.interact_size.y.max(TOUCH_TARGET);
        });
    } else if let Some(saved) = app.compact_saved_interact.take() {
        ctx.global_style_mut(|s| s.spacing.interact_size = saved);
    }
    ctx.request_repaint();
}

/// Brings `p` on screen in the compact layout (a no-op in the docked one). A panel that is in no
/// tab (Markers, History...) is shown on its own until a tab is tapped.
pub fn reveal(app: &mut FilmcraftApp, p: PanelKind) {
    if !app.compact {
        return;
    }
    match VIEWS.iter().position(|v| v.panels.iter().any(|(q, _)| *q == p)) {
        Some(i) => {
            app.compact_view = i;
            app.compact_extra = None;
        }
        None => app.compact_extra = Some(p),
    }
}

/// The Edit-mode body in the compact layout: the current view above the tab bar.
pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, body: Rect) {
    let t = app.tokens;
    let bar = Rect::from_min_max(pos2(body.min.x, (body.max.y - TAB_BAR_H).max(body.min.y)), body.max);
    let content = Rect::from_min_max(body.min, pos2(body.max.x, (bar.min.y - 2.0).max(body.min.y)));
    let view_index = app.compact_view.min(VIEWS.len() - 1);

    // the panels
    let extra = [(app.compact_extra.unwrap_or(PanelKind::Project), 1.0)];
    let stack: &[(PanelKind, f32)] = if app.compact_extra.is_some() { &extra } else { VIEWS[view_index].panels };
    let total: f32 = stack.iter().map(|(_, w)| w.max(0.0)).sum::<f32>().max(f32::EPSILON);
    let gap = 2.0;
    let usable = (content.height() - gap * stack.len().saturating_sub(1) as f32).max(0.0);
    let mut y = content.min.y;
    for (p, weight) in stack {
        let h = usable * weight.max(0.0) / total;
        let r = Rect::from_min_size(pos2(content.min.x, y), vec2(content.width(), h));
        y += h + gap;
        if r.height() < 1.0 {
            continue;
        }
        ui.painter().rect_filled(r, t.radius, t.panel_bg);
        // a touch in a panel focuses it, as a click does in the dock
        if ui.input(|i| i.pointer.any_pressed()) && ui.input(|i| i.pointer.interact_pos()).is_some_and(|q| r.contains(q)) {
            app.ui.focused = *p;
        }
        app.auto.add(&format!("panel.{}", p.id()), r, p.title());
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).id_salt(("panel", p.id())));
        child.set_clip_rect(r);
        panels::show(app, &mut child, *p, r);
    }

    // the tab bar
    ui.painter().rect_filled(bar, 0.0, t.header_bg);
    ui.painter().line_segment([bar.left_top(), bar.right_top()], Stroke::new(1.0, egui::Color32::BLACK));
    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar).id_salt("compact-tabs").layout(egui::Layout::left_to_right(egui::Align::Center)));
    bar_ui.set_clip_rect(bar);
    let mut picked = None;
    ScrollArea::horizontal().id_salt("compact-tabs-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(
        &mut bar_ui,
        |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (i, v) in VIEWS.iter().enumerate() {
                    let active = app.compact_extra.is_none() && i == view_index;
                    let color = if active { t.tab_text_active } else { t.tab_text };
                    let galley = ui.painter().layout_no_wrap(v.label.to_string(), Tokens::ui(14.0), color);
                    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 32.0, TAB_BAR_H), Sense::click());
                    app.auto.add(&format!("compact.tab.{}", v.label.to_ascii_lowercase()), r, v.label);
                    if resp.is_pointer_button_down_on() {
                        ui.painter().rect_filled(r, 0.0, t.hover);
                    }
                    if active {
                        ui.painter().line_segment([r.left_top() + vec2(8.0, 1.0), r.right_top() + vec2(-8.0, 1.0)], Stroke::new(2.0, t.accent));
                    }
                    ui.painter().galley(r.center() - galley.size() / 2.0, galley, color);
                    if resp.clicked() {
                        picked = Some(i);
                    }
                }
            });
        },
    );
    if let Some(i) = picked {
        app.compact_view = i;
        app.compact_extra = None;
        if let Some((p, _)) = VIEWS[i].panels.last() {
            app.ui.focused = *p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_are_compact_desktops_are_not() {
        assert!(wants_compact(390.0, false));
        assert!(wants_compact(700.0, false));
        assert!(!wants_compact(1280.0, false));
        assert!(!wants_compact(760.0, false));
    }

    #[test]
    fn the_layout_does_not_flicker_near_the_line() {
        assert!(wants_compact(780.0, true), "stays compact until LEAVE_WIDTH");
        assert!(!wants_compact(780.0, false), "stays docked until below ENTER_WIDTH");
        assert!(!wants_compact(800.0, true));
    }

    #[test]
    fn nonsense_widths_change_nothing() {
        assert!(wants_compact(f32::NAN, true));
        assert!(!wants_compact(f32::NAN, false));
    }

    #[test]
    fn every_view_has_panels_with_positive_weight() {
        for v in &VIEWS {
            assert!(!v.panels.is_empty(), "{}", v.label);
            assert!(v.panels.iter().all(|(_, w)| *w > 0.0), "{}", v.label);
        }
    }
}
