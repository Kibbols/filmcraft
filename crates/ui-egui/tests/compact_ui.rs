//! Headless UI test of the compact (phone-width) layout: the real `FilmcraftApp` in a 390 x 844
//! window shows one view at a time with a bottom tab bar, the tabs switch views, and widening the
//! window brings the docked layout back untouched.
//!
//! Set `FILMCRAFT_UI_SNAPSHOT_DIR=<dir>` to render the window offscreen with wgpu and write
//! `compact-*.png` there.

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
    snapshots: Option<std::path::PathBuf>,
}

impl Driver {
    fn phone() -> Self {
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let snapshots = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR").map(std::path::PathBuf::from);
        let mut b = Harness::builder().with_size(egui::vec2(390.0, 844.0)).with_max_steps(10_000);
        if snapshots.is_some() {
            b = b.wgpu();
        }
        let harness = b.build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx, snapshots };
        d.frames(6);
        d
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
                return v["result"].clone();
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn click(&mut self, id: &str) {
        self.ok("ui.click", json!({"id": id}));
        self.frames(3);
    }

    fn rect(&mut self, id: &str) -> Option<[f32; 4]> {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        v.as_array()?.iter().find(|e| e["id"] == id).map(|e| {
            let r = &e["rect"];
            [r[0].as_f64().unwrap() as f32, r[1].as_f64().unwrap() as f32, r[2].as_f64().unwrap() as f32, r[3].as_f64().unwrap() as f32]
        })
    }

    fn has(&mut self, id: &str) -> bool {
        self.rect(id).is_some()
    }

    /// A finger down at (`x`, `y`), moved by (`dx`, `dy`) over `steps` frames, then lifted.
    fn swipe(&mut self, x: f32, y: f32, dx: f32, dy: f32, steps: usize) {
        let mut p = egui::pos2(x, y);
        let push = |d: &mut Self, e: egui::Event| d.harness.input_mut().events.push(e);
        push(self, egui::Event::PointerMoved(p));
        self.frames(1);
        push(self, egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        self.frames(1);
        for _ in 0..steps {
            p.x += dx / steps as f32;
            p.y += dy / steps as f32;
            push(self, egui::Event::PointerMoved(p));
            self.frames(1);
        }
        push(self, egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        self.frames(2);
    }

    fn snapshot(&mut self, name: &str) {
        let Some(dir) = self.snapshots.clone() else { return };
        self.frames(2);
        match self.harness.render() {
            Ok(img) => {
                std::fs::create_dir_all(&dir).unwrap();
                let path = dir.join(format!("{name}.png"));
                img.save(&path).unwrap();
                eprintln!("snapshot: {}", path.display());
            }
            Err(e) => eprintln!("snapshot {name} skipped: {e}"),
        }
    }
}

#[test]
fn a_phone_window_shows_one_view_with_a_tab_bar() {
    let mut d = Driver::phone();
    assert!(d.harness.state().compact, "390 pt wide is compact");
    // the Edit view stacks Program over Timeline, both inside the window
    let program = d.rect("panel.Program").expect("Program is shown");
    let timeline = d.rect("panel.Timeline").expect("Timeline is shown");
    assert!(program[1] < timeline[1], "Program above Timeline");
    for r in [program, timeline] {
        assert!(r[0] >= 0.0 && r[0] + r[2] <= 390.0 + 0.5, "panel fits the width: {r:?}");
    }
    assert!(!d.has("panel.Project"), "other panels are not drawn at the same time");
    // the tab bar is big enough for a finger and sits below the panels
    let tab = d.rect("compact.tab.edit").expect("tab bar");
    assert!(tab[3] >= 44.0, "tab height {}", tab[3]);
    assert!(tab[1] > timeline[1] + timeline[3] - 1.0, "tab bar below the panels");
    d.snapshot("compact-edit");
}

#[test]
fn tabs_switch_the_view() {
    let mut d = Driver::phone();
    d.click("compact.tab.project");
    assert!(d.has("panel.Project"));
    assert!(!d.has("panel.Timeline"));
    d.snapshot("compact-project");
    d.click("compact.tab.controls");
    assert!(d.has("panel.EffectControls"));
    assert!(!d.has("panel.Project"));
    d.snapshot("compact-controls");
    d.click("compact.tab.edit");
    assert!(d.has("panel.Program") && d.has("panel.Timeline"));
}

#[test]
fn the_header_fits_and_keeps_the_menu_reachable() {
    let mut d = Driver::phone();
    for id in ["header.mode.import", "header.mode.edit", "header.mode.export", "header.quickExport", "header.workspaces", "header.menu"] {
        let r = d.rect(id).unwrap_or_else(|| panic!("no {id}"));
        assert!(r[0] >= 0.0 && r[0] + r[2] <= 390.0 + 0.5, "{id} inside the window: {r:?}");
    }
    assert!(!d.has("header.discord"), "no room for the Discord button");
    d.click("header.menu");
    d.snapshot("compact-menu");
}

#[test]
fn widening_the_window_restores_the_docked_layout() {
    let mut d = Driver::phone();
    d.ok("ui.resize", json!({"width": 1600, "height": 980}));
    d.frames(6);
    assert!(!d.harness.state().compact);
    assert!(!d.has("compact.tab.edit"));
    assert!(d.has("panel.Timeline") && d.has("panel.Project") && d.has("panel.Program"), "every docked panel is back");
    assert!(d.has("header.discord"));
}

#[test]
fn track_headers_are_a_badge_and_a_three_dot_menu() {
    let mut d = Driver::phone();
    // the per-track switches are not on screen all the time...
    for gone in ["timeline.track.V1.locked", "timeline.track.V1.syncLock", "timeline.track.V1.enabled", "timeline.track.A1.muted", "timeline.track.A1.solo"] {
        assert!(!d.has(gone), "{gone} should be inside the menu");
    }
    // ...the badge and the menu button are, and the rows are low
    let badge = d.rect("timeline.track.V1.target").expect("V1 badge");
    let menu = d.rect("timeline.track.V1.menu").expect("V1 menu button");
    assert!(menu[0] + menu[2] < 70.0, "the header column is narrow: {menu:?}");
    assert!(badge[3] <= 44.0 && menu[3] <= 44.0, "rows are short: {badge:?} {menu:?}");
    let v2 = d.rect("timeline.track.V2.menu").expect("V2 menu button");
    assert!((menu[1] - v2[1]).abs() <= 46.0, "video rows are at most ~42 pt apart: {menu:?} {v2:?}");
    d.snapshot("compact-timeline");
    // the menu opens on tap and holds the switches
    d.click("timeline.track.V1.menu");
    for item in ["sourcePatch", "locked", "syncLock", "enabled"] {
        assert!(d.has(&format!("timeline.track.V1.{item}")), "V1 menu lacks {item}");
    }
    d.snapshot("compact-track-menu");
    // using one runs the same command as the full header's button: one undo step
    let steps = |d: &mut Driver| {
        let h = d.ok("engine.execute", json!({"command": "history.list", "params": {}}));
        h["undo"].as_array().map_or(0, Vec::len)
    };
    let before = steps(&mut d);
    d.click("timeline.track.V1.locked");
    assert_eq!(steps(&mut d), before + 1, "locking the track from the menu is one undo step");
}

#[test]
fn tracks_scroll_up_and_down_with_a_finger() {
    let mut d = Driver::phone();
    // a short window: not every track fits
    d.ok("ui.resize", json!({"width": 390, "height": 520}));
    d.frames(6);
    assert!(d.harness.state().compact);
    let lane_y = {
        let v2 = d.rect("timeline.track.V2.menu").expect("V2 menu button");
        v2[1] + v2[3] / 2.0
    };
    let start = d.harness.state().ui.timeline.v_scroll;
    // drag down on empty lane: the content follows, bringing the higher video tracks into view
    d.swipe(340.0, lane_y, 0.0, 30.0, 6);
    let after = d.harness.state().ui.timeline.v_scroll;
    assert!(after > start, "a drag on empty space scrolls the video tracks ({start} -> {after})");
    d.snapshot("compact-scrolled");
    // dragging the header column does the same
    let before = d.harness.state().ui.timeline.v_scroll;
    d.swipe(30.0, lane_y, 0.0, -20.0, 5);
    let after = d.harness.state().ui.timeline.v_scroll;
    assert!(after < before, "a drag on a track header scrolls too ({before} -> {after})");
}

/// Drives two fingers over the timeline: `hold` frames still, then `steps` frames moving the
/// centre by (`step_x`, `step_y`) a frame with the spacing wobbling by a point or so.
fn two_fingers(d: &mut Driver, hold: usize, steps: usize, step_x: f32, step_y: f32, spread_per_step: f32) {
    use egui::{Event, TouchDeviceId, TouchId, TouchPhase, pos2};
    let lane_y = {
        let v2 = d.rect("timeline.track.V2.menu").expect("V2 menu button");
        v2[1] + v2[3] / 2.0
    };
    let touch = |d: &mut Driver, id: u64, phase: TouchPhase, x: f32, y: f32| {
        d.harness.input_mut().events.push(Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos: pos2(x, y), force: None });
    };
    let (cx, cy) = (250.0_f32, lane_y);
    // (a browser reports the pointer for touches too; egui starts a gesture only with a pointer position)
    d.harness.input_mut().events.push(Event::PointerMoved(pos2(cx, cy)));
    touch(d, 1, TouchPhase::Start, cx - 25.0, cy);
    touch(d, 2, TouchPhase::Start, cx + 25.0, cy);
    d.frames(1);
    for _ in 0..hold {
        touch(d, 1, TouchPhase::Move, cx - 25.0, cy);
        touch(d, 2, TouchPhase::Move, cx + 25.0, cy);
        d.frames(1);
    }
    for k in 1..=steps {
        let wobble = if k % 2 == 0 { 1.5 } else { -1.5 };
        let (x, y) = (cx + step_x * k as f32, cy + step_y * k as f32);
        let half = 25.0 + spread_per_step * k as f32 + wobble;
        touch(d, 1, TouchPhase::Move, x - half, y);
        touch(d, 2, TouchPhase::Move, x + half, y);
        d.frames(1);
    }
    let (x, y) = (cx + step_x * steps as f32, cy + step_y * steps as f32);
    touch(d, 1, TouchPhase::End, x - 25.0, y);
    touch(d, 2, TouchPhase::End, x + 25.0, y);
    d.frames(3);
}

fn short_phone() -> Driver {
    let mut d = Driver::phone();
    d.ok("ui.resize", json!({"width": 390, "height": 520}));
    d.frames(6);
    d
}

/// Hold two fingers still (a frame is a quarter second in the test harness), then move: scrolls,
/// however much the fingers wobble.
#[test]
fn two_fingers_held_then_moved_scroll_the_timeline() {
    let mut d = short_phone();
    let (pps, start) = {
        let tv = &d.harness.state().ui.timeline;
        (tv.pps, tv.scroll)
    };
    two_fingers(&mut d, 3, 20, -5.0, 0.0, 0.0); // 100 pt to the left
    let tv = &d.harness.state().ui.timeline;
    let moved = (tv.scroll - start) * pps;
    assert!(moved > 80.0, "held then moved: the timeline followed {moved:.1} of 100 points");
    assert!((tv.pps - pps).abs() < 1e-9, "and did not zoom ({pps} -> {})", tv.pps);
}

/// Moving straight away without a pinch does nothing, wobble or not: no scroll, no zoom.
#[test]
fn two_fingers_swiped_at_once_do_nothing() {
    let mut d = short_phone();
    let (pps, start) = {
        let tv = &d.harness.state().ui.timeline;
        (tv.pps, tv.scroll)
    };
    // (a test frame is a quarter second, so a real swipe is a few big steps)
    two_fingers(&mut d, 0, 5, -20.0, 0.0, 0.0);
    let tv = &d.harness.state().ui.timeline;
    assert_eq!(tv.target_pps, pps, "no zoom");
    assert!((tv.target_scroll - start).abs() < 1e-9, "no scroll ({start} -> {})", tv.target_scroll);
}

/// Spreading the fingers is a pinch: it zooms, and never scrolls.
#[test]
fn two_fingers_moved_at_once_zoom_and_do_not_scroll() {
    let mut d = short_phone();
    let (pps, start) = {
        let tv = &d.harness.state().ui.timeline;
        (tv.pps, tv.scroll)
    };
    two_fingers(&mut d, 0, 12, -3.0, 0.0, 4.0); // spreading, while the centre drifts left
    let tv = &d.harness.state().ui.timeline;
    assert!(tv.target_pps > pps * 1.5, "spreading fingers zoom in ({pps} -> {})", tv.target_pps);
    assert!((tv.target_scroll - start).abs() * pps < 5.0 || tv.target_pps != pps, "and the drift is not a scroll");
    let v = tv.v_scroll;
    assert!(v.abs() < 1.0, "nor does it scroll up or down");
}
