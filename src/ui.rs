use crate::input::{Shared, Status, open_apps};
use crate::model::{self, ARM_VK, Bind, Config, Console, Mode, TARGETS, Target, WHEEL_DOWN, WHEEL_UP};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Vec2};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

const W: f32 = 1180.0;
const H: f32 = 700.0;
const DRIVER_URL: &str = "https://github.com/nefarius/ViGEmBus/releases/latest";

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}
const BG: Color32 = rgb(0x0b0b0e);
const PANEL: Color32 = rgb(0x121217);
const KEY: Color32 = rgb(0x1a1a21);
const HOVER: Color32 = rgb(0x26262f);
const EDGE: Color32 = rgb(0x23232b);
const TEXT: Color32 = rgb(0xecebe6);
const DIM: Color32 = rgb(0x8b8a85);
const MUTE: Color32 = rgb(0x5a5956);
const GREEN: Color32 = rgb(0x3ddc84);
const AMBER: Color32 = rgb(0xffb454);
const RED: Color32 = rgb(0xff6b6b);

fn color(t: Target, c: Console) -> Color32 {
    use Target::*;
    let ps = c == Console::Ps;
    rgb(match t {
        Cross => if ps { 0x7fa7ff } else { 0x5fd068 },
        Circle => if ps { 0xff6b6b } else { 0xff5f5f },
        Square => if ps { 0xf08fd6 } else { 0x4fa3ff },
        Triangle => if ps { 0x3fdcb0 } else { 0xffd23f },
        L1 | R1 => 0xc9b8ff,
        L2 | R2 => 0xffb86b,
        L3 | R3 | LsUp | LsDown | LsLeft | LsRight => 0xe8e4d8,
        DUp | DDown | DLeft | DRight => 0x9fd36b,
        Options | Share | Touchpad | Ps => 0xb8b4aa,
        Switch => 0x6fa8ff,
    })
}

/// Keyboard rows: (key code, width in key units). Code 0 = a key padwire leaves alone.
const ROWS: [&[(u16, f32)]; 6] = [
    &[(0x1B, 1.0), (0, -1.0), (0x70, 1.0), (0x71, 1.0), (0x72, 1.0), (0x73, 1.0), (0, -0.5), (0x74, 1.0),
      (0x75, 1.0), (0x76, 1.0), (0x77, 1.0), (0, -0.5), (0x78, 1.0), (ARM_VK, 1.0), (0x7A, 1.0), (0x7B, 1.0)],
    &[(0xC0, 1.0), (0x31, 1.0), (0x32, 1.0), (0x33, 1.0), (0x34, 1.0), (0x35, 1.0), (0x36, 1.0), (0x37, 1.0),
      (0x38, 1.0), (0x39, 1.0), (0x30, 1.0), (0xBD, 1.0), (0xBB, 1.0), (0x08, 2.0)],
    &[(0x09, 1.5), (0x51, 1.0), (0x57, 1.0), (0x45, 1.0), (0x52, 1.0), (0x54, 1.0), (0x59, 1.0), (0x55, 1.0),
      (0x49, 1.0), (0x4F, 1.0), (0x50, 1.0), (0xDB, 1.0), (0xDD, 1.0), (0xDC, 1.5)],
    &[(0x14, 1.75), (0x41, 1.0), (0x53, 1.0), (0x44, 1.0), (0x46, 1.0), (0x47, 1.0), (0x48, 1.0), (0x4A, 1.0),
      (0x4B, 1.0), (0x4C, 1.0), (0xBA, 1.0), (0xDE, 1.0), (0x0D, 2.25)],
    &[(0xA0, 2.25), (0x5A, 1.0), (0x58, 1.0), (0x43, 1.0), (0x56, 1.0), (0x42, 1.0), (0x4E, 1.0), (0x4D, 1.0),
      (0xBC, 1.0), (0xBE, 1.0), (0xBF, 1.0), (0xA1, 2.75)],
    &[(0xA2, 1.5), (0, 1.25), (0, 1.25), (0x20, 6.25), (0, 1.25), (0, -2.0), (0xA3, 1.5)],
];
const LEFT_ALONE: [&str; 3] = ["Win", "Alt", "Alt"];

pub struct App {
    sh: Arc<Shared>,
    cfg: Config,
    path: PathBuf,
    pick: Option<u16>,
    adding: Option<Vec<String>>,
    shot: Option<PathBuf>,
    frames: u32,
}

/// Paints in design pixels (1180 x 700), scaled to the window.
struct Cv<'a> {
    ui: &'a egui::Ui,
    o: Pos2,
    s: f32,
    /// False while the picker is open: everything behind it is drawn but can't be clicked.
    active: bool,
}

impl Cv<'_> {
    fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.o + Vec2::new(x, y) * self.s, Vec2::new(w, h) * self.s)
    }

    fn fill(&self, r: Rect, radius: f32, c: Color32) {
        self.ui.painter().rect_filled(r, radius * self.s, c);
    }

    fn outline(&self, r: Rect, radius: f32, c: Color32) {
        self.ui.painter().rect_stroke(r, radius * self.s, Stroke::new(1.0, c), StrokeKind::Inside);
    }

    fn text(&self, p: Pos2, a: Align2, t: &str, size: f32, c: Color32) -> Rect {
        self.ui.painter().text(p, a, t, FontId::proportional(size * self.s), c)
    }

    /// Text that shrinks to fit `max` design pixels.
    fn fit(&self, p: Pos2, a: Align2, t: &str, size: f32, max: f32, c: Color32) {
        let font = FontId::proportional(size * self.s);
        let w = self.ui.painter().layout_no_wrap(t.to_string(), font, c).size().x;
        let k = (max * self.s / w).min(1.0);
        self.text(p, a, t, size * k, c);
    }

    fn hit(&self, r: Rect, id: impl std::hash::Hash + std::fmt::Debug, sense: Sense) -> Response {
        let (r, sense) = if self.active { (r, sense) } else { (Rect::NOTHING, Sense::hover()) };
        self.ui.interact(r, self.ui.scope_id().with(id), sense)
    }

    /// A text button. Returns true when clicked.
    fn button(&self, r: Rect, id: &str, label: &str, fg: Color32, bg: Color32) -> bool {
        let resp = self.hit(r, id, Sense::click());
        self.fill(r, 8.0, if resp.hovered() { HOVER } else { bg });
        self.text(r.center(), Align2::CENTER_CENTER, label, 13.0, fg);
        resp.clicked()
    }

    /// Two-way switch. Returns the side that was clicked.
    fn segmented(&self, r: Rect, id: &str, labels: [&str; 2], on: usize) -> Option<usize> {
        self.fill(r, 8.0, PANEL);
        let mut clicked = None;
        for (i, label) in labels.iter().enumerate() {
            let half = Rect::from_min_size(r.min + Vec2::new(i as f32 * r.width() / 2.0, 0.0), Vec2::new(r.width() / 2.0, r.height()));
            let resp = self.hit(half, (id, i), Sense::click());
            if i == on {
                self.fill(half.shrink(3.0 * self.s), 6.0, HOVER);
            }
            let c = if i == on || resp.hovered() { TEXT } else { DIM };
            self.text(half.center(), Align2::CENTER_CENTER, label, 13.0, c);
            if resp.clicked() && i != on {
                clicked = Some(i);
            }
        }
        clicked
    }

    fn slider(&self, r: Rect, id: &str, label: &str, v: &mut f32, lo: f32, hi: f32) -> bool {
        let s = self.s;
        self.text(r.left_center(), Align2::LEFT_CENTER, label, 13.0, TEXT);
        self.text(r.right_center(), Align2::RIGHT_CENTER, &format!("{v:.2}"), 13.0, DIM);
        let track = Rect::from_min_max(
            Pos2::new(r.left() + 120.0 * s, r.center().y - 2.0 * s),
            Pos2::new(r.right() - 48.0 * s, r.center().y + 2.0 * s),
        );
        let resp = self.hit(track.expand2(Vec2::new(8.0, 12.0) * s), id, Sense::click_and_drag());
        let mut changed = false;
        if let Some(p) = resp.interact_pointer_pos()
            && (resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on())
        {
            let f = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let nv = ((lo + f * (hi - lo)) * 100.0).round() / 100.0;
            changed = nv != *v;
            *v = nv;
        }
        let x = track.left() + (*v - lo) / (hi - lo) * track.width();
        self.fill(track, 2.0, EDGE);
        self.fill(Rect::from_min_max(track.min, Pos2::new(x, track.bottom())), 2.0, TEXT);
        self.ui.painter().circle_filled(Pos2::new(x, track.center().y), 7.0 * s, TEXT);
        changed
    }

    fn checkbox(&self, r: Rect, id: &str, label: &str, v: &mut bool) -> bool {
        let s = self.s;
        let resp = self.hit(r, id, Sense::click());
        let b = Rect::from_min_size(Pos2::new(r.left(), r.center().y - 8.0 * s), Vec2::splat(16.0 * s));
        self.fill(b, 4.0, if *v { TEXT } else { EDGE });
        if *v {
            self.text(b.center(), Align2::CENTER_CENTER, "✓", 12.0, BG);
        }
        self.text(Pos2::new(b.right() + 8.0 * s, r.center().y), Align2::LEFT_CENTER, label, 13.0, if resp.hovered() { TEXT } else { DIM });
        if resp.clicked() {
            *v = !*v;
        }
        resp.clicked()
    }
}

impl App {
    pub fn new(sh: Arc<Shared>, cfg: Config, path: PathBuf) -> App {
        App { sh, cfg, path, pick: None, adding: None, shot: None, frames: 0 }
    }

    /// Off-screen check: open the picker for `pick`, save frame 8 to `shot`, close.
    pub fn shot(mut self, shot: Option<PathBuf>, pick: Option<u16>) -> App {
        self.shot = shot;
        self.pick = pick;
        self
    }

    fn changed(&mut self) {
        self.cfg.save(&self.path);
        self.sh.core.lock().unwrap().set_config(&self.cfg);
        *self.sh.want.lock().unwrap() = (self.cfg.apps.clone(), self.cfg.console);
    }

    fn key(&mut self, cv: &Cv, r: Rect, code: u16, mode: Mode) {
        let s = cv.s;
        let bind = model::key_name(code).and_then(|n| self.cfg.layout(mode).get(n));
        let resp = cv.hit(r, ("key", code), Sense::click());
        cv.fill(r, 7.0, if resp.hovered() || self.pick == Some(code) { HOVER } else { KEY });
        let cap = model::key_cap(code);
        if let Some(b) = bind {
            let c = color(b.to.iter().copied().find(|t| *t != Target::Switch).unwrap_or(Target::Switch), self.cfg.console);
            cv.text(r.min + Vec2::new(6.0, 4.0) * s, Align2::LEFT_TOP, cap, 10.0, DIM);
            let label = model::describe(b, mode);
            cv.fit(Pos2::new(r.center().x, r.bottom() - 9.0 * s), Align2::CENTER_BOTTOM, label, 11.5, r.width() / s - 6.0, TEXT);
            cv.fill(Rect::from_min_max(Pos2::new(r.left() + 7.0 * s, r.bottom() - 5.0 * s), Pos2::new(r.right() - 7.0 * s, r.bottom() - 3.0 * s)), 1.0, c);
            if b.toggle {
                cv.ui.painter().circle_filled(r.right_top() + Vec2::new(-8.0, 8.0) * s, 3.0 * s, c);
            }
        } else {
            cv.text(r.min + Vec2::new(6.0, 4.0) * s, Align2::LEFT_TOP, cap, 10.0, MUTE);
        }
        if resp.clicked() {
            self.pick = Some(code);
        }
    }

    fn keyboard(&mut self, cv: &Cv, mode: Mode) {
        const U: f32 = 54.0;
        let (x0, y0) = (24.0, 124.0);
        for (ri, row) in ROWS.iter().enumerate() {
            let (mut x, mut alone) = (x0, 0);
            for &(code, w) in row.iter() {
                if w < 0.0 {
                    x += -w * U;
                    continue;
                }
                let r = cv.r(x, y0 + ri as f32 * U, w * U - 6.0, U - 6.0);
                x += w * U;
                if code == ARM_VK {
                    cv.outline(r, 7.0, EDGE);
                    cv.text(r.min + Vec2::new(6.0, 4.0) * cv.s, Align2::LEFT_TOP, "F10", 10.0, DIM);
                    cv.text(Pos2::new(r.center().x, r.bottom() - 9.0 * cv.s), Align2::CENTER_BOTTOM, "On/off", 11.5, DIM);
                } else if code == 0 {
                    cv.outline(r, 7.0, EDGE);
                    cv.text(r.min + Vec2::new(6.0, 4.0) * cv.s, Align2::LEFT_TOP, LEFT_ALONE[alone], 10.0, MUTE);
                    alone += 1;
                } else {
                    self.key(cv, r, code, mode);
                }
            }
        }
        // arrows
        let ax = x0 + 15.0 * U + 18.0;
        for (code, cx, cy) in [(0x26, 1.0, 4.0), (0x25, 0.0, 5.0), (0x28, 1.0, 5.0), (0x27, 2.0, 5.0)] {
            self.key(cv, cv.r(ax + cx * U, y0 + cy * U, U - 6.0, U - 6.0), code, mode);
        }
        // mouse
        let mx = ax + 3.0 * U + 18.0;
        let mw = W - 24.0 - mx;
        let half = (mw - 6.0) / 2.0;
        cv.text(cv.r(mx, y0, mw, U - 6.0).left_bottom() + Vec2::new(2.0, -6.0) * cv.s, Align2::LEFT_BOTTOM, "Mouse", 12.0, DIM);
        for (code, col, row, w) in [
            (0x01, 0.0, 1.0, half), (0x02, 1.0, 1.0, half), (0x04, 0.0, 2.0, mw),
            (WHEEL_UP, 0.0, 3.0, half), (WHEEL_DOWN, 1.0, 3.0, half), (0x05, 0.0, 4.0, half), (0x06, 1.0, 4.0, half),
        ] {
            self.key(cv, cv.r(mx + col * (half + 6.0), y0 + row * U, w, U - 6.0), code, mode);
        }
        cv.text(cv.r(mx, y0 + 5.0 * U, mw, U - 6.0).left_center() + Vec2::new(2.0, 0.0), Align2::LEFT_CENTER, "Move = look", 12.0, DIM);
    }

    fn aim_card(&mut self, cv: &Cv, x: f32, y: f32, w: f32) {
        cv.text(cv.r(x + 18.0, y + 14.0, 0.0, 0.0).min, Align2::LEFT_TOP, "Mouse aim", 12.0, DIM);
        let mut a = self.cfg.aim;
        let mut ch = cv.slider(cv.r(x + 18.0, y + 44.0, w - 36.0, 28.0), "speed", "Mouse speed", &mut a.speed, 0.2, 4.0);
        ch |= cv.slider(cv.r(x + 18.0, y + 82.0, w - 36.0, 28.0), "push", "Starting push", &mut a.push, 0.0, 0.6);
        ch |= cv.slider(cv.r(x + 18.0, y + 120.0, w - 36.0, 28.0), "curve", "Curve", &mut a.curve, 0.4, 2.0);
        ch |= cv.checkbox(cv.r(x + 18.0, y + 162.0, 110.0, 28.0), "invert", "Invert Y", &mut a.invert_y);
        if ch {
            self.cfg.aim = a;
            self.changed();
        }
    }

    fn pad_card(&self, cv: &Cv, x: f32, y: f32, st: &Status) {
        use Target::*;
        let (s, p, con) = (cv.s, st.pad, self.cfg.console);
        cv.text(cv.r(x + 18.0, y + 14.0, 0.0, 0.0).min, Align2::LEFT_TOP, "Controller", 12.0, DIM);
        for (i, (sx, sy, click)) in [(p.lx, p.ly, L3), (p.rx, p.ry, R3)].into_iter().enumerate() {
            let c = cv.r(x + 62.0 + i as f32 * 96.0, y + 92.0, 0.0, 0.0).min;
            cv.ui.painter().circle_stroke(c, 36.0 * s, Stroke::new(1.0, if p.has(click) { TEXT } else { EDGE }));
            cv.ui.painter().circle_filled(c + Vec2::new(sx, -sy) * 30.0 * s, 6.0 * s, if sx != 0.0 || sy != 0.0 { TEXT } else { MUTE });
            cv.text(c + Vec2::new(0.0, 50.0 * s), Align2::CENTER_CENTER, model::button_name(click, con), 11.0, if p.has(click) { TEXT } else { DIM });
        }
        let chips: [&[Target]; 4] = [&[L2, L1, R1, R2], &[Triangle, Circle, Cross, Square], &[DUp, DDown, DLeft, DRight], &[Share, Touchpad, Options, Ps]];
        for (ri, row) in chips.iter().enumerate() {
            for (ci, &t) in row.iter().enumerate() {
                if con == Console::Xbox && t == Touchpad {
                    continue;
                }
                let r = cv.r(x + 212.0 + ci as f32 * 40.0, y + 40.0 + ri as f32 * 38.0, 36.0, 32.0);
                let on = p.has(t) || (t == L2 && p.lt > 0.0) || (t == R2 && p.rt > 0.0);
                let c = color(t, con);
                cv.fill(r, 6.0, if on { c } else { KEY });
                let name = model::button_name(t, con).trim_start_matches("D-pad ");
                let size = if matches!(t, Cross | Circle | Square | Triangle) && con == Console::Ps { 17.0 } else { 12.0 };
                cv.fit(r.center(), Align2::CENTER_CENTER, name, size, 32.0, if on { BG } else { c });
            }
        }
    }

    fn apps_card(&mut self, cv: &Cv, x: f32, y: f32, w: f32, st: &Status, live: bool) {
        cv.text(cv.r(x + 18.0, y + 14.0, 0.0, 0.0).min, Align2::LEFT_TOP, if self.adding.is_some() { "Add an open app" } else { "Works in" }, 12.0, DIM);
        let row = |i: usize| cv.r(x + 12.0, y + 38.0 + i as f32 * 28.0, w - 24.0, 26.0);
        if let Some(list) = self.adding.clone() {
            if cv.button(cv.r(x + w - 86.0, y + 8.0, 72.0, 24.0), "add-cancel", "Cancel", DIM, PANEL) {
                self.adding = None;
            }
            let new: Vec<&String> = list.iter().filter(|e| !self.cfg.apps.contains(e)).take(6).collect();
            for (i, exe) in new.into_iter().enumerate() {
                let r = row(i);
                let resp = cv.hit(r, ("add", i), Sense::click());
                if resp.hovered() {
                    cv.fill(r, 6.0, HOVER);
                }
                cv.fit(r.left_center() + Vec2::new(8.0 * cv.s, 0.0), Align2::LEFT_CENTER, exe, 13.0, w - 50.0, TEXT);
                if resp.clicked() {
                    self.cfg.apps.push(exe.clone());
                    self.adding = None;
                    self.changed();
                    return;
                }
            }
            return;
        }
        if cv.button(cv.r(x + w - 86.0, y + 8.0, 72.0, 24.0), "add", "+ Add", TEXT, PANEL) {
            self.adding = Some(open_apps());
        }
        let mut remove = None;
        for (i, exe) in self.cfg.apps.iter().take(6).enumerate() {
            let r = row(i);
            let front = *exe == st.front;
            cv.ui.painter().circle_filled(r.left_center() + Vec2::new(10.0 * cv.s, 0.0), 3.0 * cv.s, if front && live { GREEN } else if front { TEXT } else { MUTE });
            cv.fit(r.left_center() + Vec2::new(22.0 * cv.s, 0.0), Align2::LEFT_CENTER, exe, 13.0, w - 80.0, if front { TEXT } else { DIM });
            let xr = Rect::from_min_size(Pos2::new(r.right() - 26.0 * cv.s, r.top()), Vec2::splat(26.0 * cv.s));
            let resp = cv.hit(xr, ("rm", i), Sense::click());
            cv.text(xr.center(), Align2::CENTER_CENTER, "✕", 11.0, if resp.hovered() { RED } else { MUTE });
            if resp.clicked() {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            self.cfg.apps.remove(i);
            self.changed();
        }
    }

    fn picker(&mut self, cv: &Cv, code: u16, mode: Mode, full: Rect) {
        let s = cv.s;
        let Some(name) = model::key_name(code) else { return };
        let back = cv.hit(full, "backdrop", Sense::click());
        cv.ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(170));
        let (pw, ph) = (576.0, 396.0);
        let (px, py) = ((W - pw) / 2.0, (H - ph) / 2.0);
        let panel = cv.r(px, py, pw, ph);
        cv.hit(panel, "panel", Sense::click());
        cv.fill(panel, 12.0, PANEL);
        cv.outline(panel, 12.0, EDGE);
        let layout = if mode == Mode::Foot { "On foot" } else { "Driving" };
        cv.text(cv.r(px + 22.0, py + 18.0, 0.0, 0.0).min, Align2::LEFT_TOP, &format!("{}  ·  {layout}", model::key_cap(code)), 16.0, TEXT);
        let mut bind = self.cfg.layout(mode).get(name).cloned().unwrap_or_default();
        let before = bind.clone();
        for (i, &t) in TARGETS.iter().enumerate() {
            let r = cv.r(px + 22.0 + (i % 4) as f32 * 134.0, py + 56.0 + (i / 4) as f32 * 46.0, 128.0, 40.0);
            let resp = cv.hit(r, ("target", i), Sense::click());
            let on = bind.to.contains(&t);
            let c = color(t, self.cfg.console);
            cv.fill(r, 8.0, if on || resp.hovered() { HOVER } else { KEY });
            if on {
                cv.outline(r, 8.0, c);
            }
            let face = self.cfg.console == Console::Ps && matches!(t, Target::Cross | Target::Circle | Target::Square | Target::Triangle);
            let room = if t == Target::Switch { 108.0 } else { 62.0 };
            cv.fit(r.left_center() + Vec2::new(10.0 * s, 0.0), Align2::LEFT_CENTER, model::button_name(t, self.cfg.console), if face { 18.0 } else { 13.0 }, room, c);
            if t != Target::Switch {
                cv.fit(r.right_center() - Vec2::new(8.0 * s, 0.0), Align2::RIGHT_CENTER, model::action(t, mode), 11.0, 52.0, DIM);
            }
            if resp.clicked() {
                if on { bind.to.retain(|x| *x != t) } else { bind.to.push(t) }
            }
        }
        let fy = py + ph - 52.0;
        cv.checkbox(cv.r(px + 22.0, fy, 190.0, 32.0), "toggle", "Press once to hold", &mut bind.toggle);
        if cv.button(cv.r(px + pw - 212.0, fy, 90.0, 32.0), "clear", "Clear", DIM, KEY) {
            bind = Bind::default();
        }
        let done = cv.button(cv.r(px + pw - 112.0, fy, 90.0, 32.0), "done", "Done", BG, TEXT);
        if bind != before {
            let l = self.cfg.layout_mut(mode);
            if bind.to.is_empty() {
                l.remove(name);
            } else {
                l.insert(name.to_string(), bind);
            }
            self.changed();
        }
        if done || back.clicked() || cv.ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.pick = None;
        }
    }
}

/// Segoe UI for text, Segoe UI Symbol for the button glyphs.
pub fn fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:/Windows".into());
    for (name, file, front) in [("segoe", "segoeui.ttf", true), ("symbols", "seguisym.ttf", false)] {
        if let Ok(bytes) = std::fs::read(format!("{dir}/Fonts/{file}")) {
            fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
            let fam = fonts.families.entry(egui::FontFamily::Proportional).or_default();
            if front { fam.insert(0, name.into()) } else { fam.push(name.into()) }
        }
    }
    ctx.set_fonts(fonts);
}

impl App {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, BG);
        let s = (full.width() / W).min(full.height() / H);
        let o = full.min + (full.size() - Vec2::new(W, H) * s) / 2.0;
        let modal = self.pick.is_some();
        let cv = Cv { ui, o, s, active: !modal };
        let st = self.sh.status.lock().unwrap().clone();
        let (armed, live) = (self.sh.armed.load(Relaxed), self.sh.live.load(Relaxed));
        let mode = self.sh.mode();

        // header
        cv.text(cv.r(24.0, 40.0, 0.0, 0.0).min, Align2::LEFT_CENTER, "padwire", 20.0, TEXT);
        let (dot, label) = match (&st.error, armed, live) {
            (Some(e), _, _) => (RED, e.as_str()),
            (_, _, true) => (GREEN, "Live"),
            (_, true, _) => (AMBER, "On, waiting for the game window"),
            _ => (MUTE, "Off"),
        };
        let p = cv.r(124.0, 40.0, 0.0, 0.0).min;
        cv.ui.painter().circle_filled(p, 4.0 * s, dot);
        cv.text(p + Vec2::new(12.0 * s, 0.0), Align2::LEFT_CENTER, label, 13.0, if dot == MUTE { DIM } else { dot });
        let arm = cv.r(W - 24.0 - 150.0, 22.0, 150.0, 36.0);
        if st.error.is_some() {
            if cv.button(arm, "driver", "Get the driver", BG, TEXT) {
                cv.ui.ctx().open_url(egui::OpenUrl::new_tab(DRIVER_URL));
            }
        } else if cv.button(arm, "arm", if armed { "Turn off  ·  F10" } else { "Turn on  ·  F10" }, if armed { TEXT } else { BG }, if armed { KEY } else { TEXT }) {
            self.sh.armed.store(!armed, Relaxed);
        }
        let on = (self.cfg.console == Console::Xbox) as usize;
        if let Some(i) = cv.segmented(cv.r(W - 24.0 - 150.0 - 16.0 - 220.0, 22.0, 220.0, 36.0), "console", ["PlayStation", "Xbox"], on) {
            self.cfg.console = [Console::Ps, Console::Xbox][i];
            self.changed();
        }
        if let Some(i) = cv.segmented(cv.r(24.0, 78.0, 220.0, 34.0), "mode", ["On foot", "Driving"], (mode == Mode::Drive) as usize) {
            let mut core = self.sh.core.lock().unwrap();
            if (core.mode == Mode::Drive) as usize != i {
                core.flip();
            }
        }
        if cv.button(cv.r(W - 24.0 - 130.0, 80.0, 130.0, 30.0), "reset", "Reset layouts", DIM, BG) {
            let d = Config::default();
            self.cfg.foot = d.foot;
            self.cfg.drive = d.drive;
            self.changed();
        }
        cv.text(cv.r(262.0, 95.0, 0.0, 0.0).min, Align2::LEFT_CENTER, "Click a key to change it", 12.0, MUTE);

        self.keyboard(&cv, mode);

        let (cy, ch) = (462.0, 214.0);
        let cards = [(24.0, 372.0), (412.0, 388.0), (816.0, 340.0)];
        for (x, w) in cards {
            cv.fill(cv.r(x, cy, w, ch), 12.0, PANEL);
        }
        self.aim_card(&cv, cards[0].0, cy, cards[0].1);
        self.apps_card(&cv, cards[2].0, cy, cards[2].1, &st, live);
        self.pad_card(&cv, cards[1].0, cy, &st);

        if let Some(code) = self.pick {
            self.picker(&Cv { ui, o, s, active: true }, code, mode, full);
        }

    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
        let ctx = ui.ctx().clone();
        let live = self.sh.live.load(Relaxed);
        ctx.request_repaint_after(std::time::Duration::from_millis(if live { 16 } else { 100 }));
        self.frames += 1;
        if self.frames == 8 && let Some(path) = self.shot.take() {
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                let _ = image::save_buffer(&path, img.as_raw(), img.width() as u32, img.height() as u32, image::ColorType::Rgba8);
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        }
    }

    fn on_exit(&mut self) {
        self.sh.quit.store(true, Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Core;
    use crate::model::Target::*;

    /// Drives the real UI code headless: no window, pointer events at design coordinates.
    struct Rig {
        ctx: egui::Context,
        app: App,
        t: f64,
    }

    impl Rig {
        fn new(name: &str) -> Rig {
            let path = std::env::temp_dir().join(format!("padwire-ui-{}-{name}.json", std::process::id()));
            let cfg = Config::default();
            let sh = Shared::new(Core::new(&cfg), cfg.apps.clone(), cfg.console);
            let mut rig = Rig { ctx: egui::Context::default(), app: App::new(sh, cfg, path), t: 0.0 };
            rig.frame(vec![]);
            rig
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            self.t += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H))),
                time: Some(self.t),
                events,
                ..Default::default()
            };
            let app = &mut self.app;
            self.ctx.run_ui(input, |ui| app.draw(ui)).textures_delta.clear();
        }

        fn button(&mut self, x: f32, y: f32, pressed: bool) {
            let pos = Pos2::new(x, y);
            self.frame(vec![egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }]);
        }

        fn click(&mut self, x: f32, y: f32) {
            self.frame(vec![egui::Event::PointerMoved(Pos2::new(x, y))]);
            self.button(x, y, true);
            self.button(x, y, false);
            self.frame(vec![]);
        }

        fn saved(&self) -> Config {
            Config::load(&self.app.path)
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.app.path);
        }
    }

    const KEY_W: (f32, f32) = (183.0, 256.0);
    const KEY_J: (f32, f32) = (444.0, 310.0);
    const CHIP_CROSS: (f32, f32) = (388.0, 228.0);
    const CHIP_L1: (f32, f32) = (388.0, 274.0);
    const HOLD: (f32, f32) = (419.0, 512.0);
    const CLEAR: (f32, f32) = (711.0, 512.0);
    const DONE: (f32, f32) = (811.0, 512.0);

    #[test]
    fn rebinding_a_key() {
        let mut r = Rig::new("rebind");
        r.click(KEY_J.0, KEY_J.1);
        assert_eq!(r.app.pick, Some(0x4A), "clicking J opens its picker");
        r.click(CHIP_CROSS.0, CHIP_CROSS.1);
        r.click(CHIP_L1.0, CHIP_L1.1);
        r.click(HOLD.0, HOLD.1);
        r.click(DONE.0, DONE.1);
        assert_eq!(r.app.pick, None);
        let want = Bind { to: vec![Cross, L1], toggle: true };
        assert_eq!(r.app.cfg.foot["J"], want);
        assert_eq!(r.saved().foot["J"], want, "written to disk");
        assert_eq!(r.saved().drive.get("J"), None, "only the layout on screen changes");
        // the running engine got it too: J now latches Cross + L1
        let mut core = r.app.sh.core.lock().unwrap();
        assert!(core.press(0x4A, 0.0));
        let pad = core.tick(0.0, 0.001);
        assert!(pad.has(Cross) && pad.has(L1));
    }

    #[test]
    fn unbinding_and_clicking_outside() {
        let mut r = Rig::new("unbind");
        r.click(KEY_W.0, KEY_W.1);
        r.click(CLEAR.0, CLEAR.1);
        assert_eq!(r.app.cfg.foot.get("W"), None);
        assert_eq!(r.saved().foot.get("W"), None);
        // a click outside the panel closes the picker and does not reach the key under it
        r.click(60.0, 256.0);
        assert_eq!(r.app.pick, None);
        r.click(KEY_W.0, KEY_W.1);
        r.click(CHIP_CROSS.0, CHIP_CROSS.1);
        r.click(CHIP_CROSS.0, CHIP_CROSS.1);
        assert_eq!(r.app.cfg.foot.get("W"), None, "a second click takes the button off again");
    }

    #[test]
    fn header_controls() {
        let mut r = Rig::new("header");
        r.click(935.0, 40.0);
        assert_eq!(r.app.cfg.console, Console::Xbox);
        assert_eq!(r.app.sh.want.lock().unwrap().1, Console::Xbox, "engine is told to become an Xbox pad");
        assert_eq!(r.saved().console, Console::Xbox);
        r.click(189.0, 95.0);
        assert_eq!(r.app.sh.mode(), Mode::Drive);
        r.click(1081.0, 40.0);
        assert!(r.app.sh.armed.load(Relaxed));
        r.click(1081.0, 40.0);
        assert!(!r.app.sh.armed.load(Relaxed));
        r.app.cfg.foot.clear();
        r.click(1091.0, 95.0);
        assert_eq!(r.app.cfg.foot, Config::default().foot, "reset layouts");
    }

    #[test]
    fn driver_missing_swaps_the_button() {
        let mut r = Rig::new("driver");
        r.app.sh.status.lock().unwrap().error = Some("ViGEmBus driver is not installed".into());
        r.click(1081.0, 40.0);
        assert!(!r.app.sh.armed.load(Relaxed), "cannot arm without the driver");
    }

    #[test]
    fn aim_sliders_and_invert() {
        let mut r = Rig::new("aim");
        r.click(330.0, 520.0);
        assert_eq!(r.app.cfg.aim.speed, 4.0);
        r.click(162.0, 558.0);
        assert_eq!(r.app.cfg.aim.push, 0.0);
        // drag the curve slider from the left end to the middle
        r.frame(vec![egui::Event::PointerMoved(Pos2::new(162.0, 596.0))]);
        r.button(162.0, 596.0, true);
        r.frame(vec![egui::Event::PointerMoved(Pos2::new(246.0, 596.0))]);
        r.button(246.0, 596.0, false);
        assert_eq!(r.app.cfg.aim.curve, 1.2);
        r.click(97.0, 638.0);
        assert!(r.app.cfg.aim.invert_y);
        assert_eq!(r.saved().aim, r.app.cfg.aim);
    }

    #[test]
    fn app_list() {
        let mut r = Rig::new("apps");
        r.click(1131.0, 513.0);
        assert!(!r.app.cfg.apps.contains(&"veyra_qml_ui.exe".to_string()), "first row removed");
        assert_eq!(r.app.cfg.apps.len(), 4);
        r.click(1106.0, 482.0);
        assert!(r.app.adding.is_some(), "+ Add lists the open apps");
        r.app.adding = Some(vec!["chiaki.exe".into(), "obs64.exe".into()]);
        r.frame(vec![]);
        r.click(900.0, 513.0);
        assert_eq!(r.app.cfg.apps.last().unwrap(), "obs64.exe", "apps already listed are not offered again");
        assert_eq!(r.app.sh.want.lock().unwrap().0, r.app.cfg.apps);
        assert_eq!(r.saved().apps, r.app.cfg.apps);
    }
}
