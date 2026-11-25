use crate::input::{Shared, Status, open_apps};
use crate::model::{self, ARM_VK, Bind, Config, Console, Mode, TARGETS, Target, WHEEL_DOWN, WHEEL_UP};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Vec2};
use std::path::PathBuf;
use crate::update::{self, Update};
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering::Relaxed;

const W: f32 = 1180.0;
const H: f32 = 700.0;
const DRIVER_URL: &str = "https://github.com/nefarius/ViGEmBus/releases/latest";

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}
const BG: Color32 = rgb(0x0e0e0e);
const PANEL: Color32 = rgb(0x161616);
const KEY: Color32 = rgb(0x202020);
const HOVER: Color32 = rgb(0x2b2b2b);
const EDGE: Color32 = rgb(0x282828);
const TEXT: Color32 = rgb(0xececec);
const DIM: Color32 = rgb(0x8a8a8a);
const MUTE: Color32 = rgb(0x5a5a5a);
const GREEN: Color32 = rgb(0x3ecf8e);
const AMBER: Color32 = rgb(0xe9b949);
const RED: Color32 = rgb(0xf2706b);

fn cap(code: u16) -> &'static str {
    match code {
        0x01 => "LMB",
        0x02 => "RMB",
        0x04 => "MMB",
        0x05 => "M4",
        0x06 => "M5",
        WHEEL_UP => "Whl↑",
        WHEEL_DOWN => "Whl↓",
        _ => model::key_cap(code),
    }
}

/// The keys on a controller button, as a short label: "Q M5". Modes can have more: "Tab N +2".
fn keys_on(layout: &model::Layout, t: Target) -> String {
    let codes = model::keys_on(layout, t);
    let mut out: Vec<String> = codes.iter().take(2).map(|c| cap(*c).to_string()).collect();
    if codes.len() > 2 {
        out.push(format!("+{}", codes.len() - 2));
    }
    out.join(" ")
}

fn color(t: Target, c: Console) -> Color32 {
    use Target::*;
    let ps = c == Console::Ps;
    rgb(match t {
        Cross => if ps { 0x7fa7ff } else { 0x5fd068 },
        Circle => if ps { 0xff6b6b } else { 0xff5f5f },
        Square => if ps { 0xf08fd6 } else { 0x4fa3ff },
        Triangle => if ps { 0x3fdcb0 } else { 0xffd23f },
        L1 | R1 => 0xc9b8ff,
        L2 | R2 | Triggers => 0xffb86b,
        L3 | R3 | LsUp | LsDown | LsLeft | LsRight | RsUp | RsDown | RsLeft | RsRight | LsHold | RsHold => 0xe8e4d8,
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
    /// Controller view instead of the keyboard.
    pad: bool,
    /// Controller button whose keys are being edited.
    btn: Option<Target>,
    /// Waiting for a key press: (slot, which keys were down last frame).
    listen: Option<(usize, Vec<bool>)>,
    /// A captured mouse press is still held: ignore it as a click.
    hold: bool,
    /// "Reset layouts" was clicked once: a second click before this time does it.
    reset_until: f64,
    adding: Option<Vec<String>>,
    update: Arc<Mutex<Update>>,
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

    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.o + Vec2::new(x, y) * self.s
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

    /// A key name or a number: monospace, shrinks to fit `max` design pixels.
    fn kbd(&self, p: Pos2, a: Align2, t: &str, size: f32, max: f32, c: Color32) {
        let w = self.kbd_width(t, size);
        let k = (max / w).min(1.0);
        self.ui.painter().text(p, a, t, FontId::monospace(size * k * self.s), c);
    }

    /// Width of `kbd` text in design pixels.
    fn kbd_width(&self, t: &str, size: f32) -> f32 {
        self.ui.painter().layout_no_wrap(t.to_string(), FontId::monospace(size * self.s), TEXT).size().x / self.s
    }

    /// A card: one step up from the page, with a hairline.
    fn card(&self, r: Rect) {
        self.fill(r, 12.0, PANEL);
        self.outline(r, 12.0, EDGE);
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
        self.outline(r, 8.0, EDGE);
        let mut clicked = None;
        for (i, label) in labels.iter().enumerate() {
            let half = Rect::from_min_size(r.min + Vec2::new(i as f32 * r.width() / 2.0, 0.0), Vec2::new(r.width() / 2.0, r.height()));
            let resp = self.hit(half, (id, i), Sense::click());
            if i == on {
                self.fill(half.shrink(3.0 * self.s), 6.0, KEY);
                self.outline(half.shrink(3.0 * self.s), 6.0, rgb(0x343434));
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
        self.kbd(r.right_center(), Align2::RIGHT_CENTER, &format!("{v:.2}"), 12.0, 44.0, DIM);
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
        App { sh, cfg, path, pick: None, pad: false, btn: None, listen: None, hold: false, reset_until: 0.0, adding: None, update: Arc::default(), shot: None, frames: 0 }
    }

    /// Off-screen check: open a view or picker, save frame 8 to `shot`, close.
    pub fn shot(mut self, shot: Option<PathBuf>, pick: Option<u16>, pad: bool, popup: bool) -> App {
        self.shot = shot;
        self.pick = pick;
        self.pad = pad || popup;
        self.btn = popup.then_some(Target::Cross);
        self
    }

    /// Looks for a newer release once at start (not in off-screen checks).
    pub fn check_update(self, yes: bool) -> App {
        if yes {
            update::check(&self.update);
        }
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
        let hot = resp.hovered() || self.pick == Some(code);
        if hot || bind.is_some() {
            cv.fill(r, 7.0, if hot { HOVER } else { KEY });
        }
        cv.outline(r, 7.0, EDGE);
        let cap = model::key_cap(code);
        if let Some(b) = bind {
            let c = color(b.to.iter().copied().find(|t| t.is_button()).unwrap_or(b.to[0]), self.cfg.console);
            cv.kbd(r.min + Vec2::new(6.0, 5.0) * s, Align2::LEFT_TOP, cap, 9.5, 40.0, DIM);
            let label = model::describe(b, mode);
            cv.fit(Pos2::new(r.center().x, r.bottom() - 9.0 * s), Align2::CENTER_BOTTOM, label, 11.5, r.width() / s - 6.0, TEXT);
            cv.fill(Rect::from_min_max(Pos2::new(r.left() + 7.0 * s, r.bottom() - 5.0 * s), Pos2::new(r.right() - 7.0 * s, r.bottom() - 3.0 * s)), 1.0, c);
            if b.toggle {
                cv.ui.painter().circle_filled(r.right_top() + Vec2::new(-8.0, 8.0) * s, 3.0 * s, c);
            }
        } else {
            cv.kbd(r.min + Vec2::new(6.0, 5.0) * s, Align2::LEFT_TOP, cap, 9.5, 40.0, MUTE);
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
                    cv.kbd(r.min + Vec2::new(6.0, 5.0) * cv.s, Align2::LEFT_TOP, "F10", 9.5, 40.0, DIM);
                    cv.text(Pos2::new(r.center().x, r.bottom() - 9.0 * cv.s), Align2::CENTER_BOTTOM, "On/off", 11.5, DIM);
                } else if code == 0 {
                    cv.outline(r, 7.0, EDGE);
                    cv.kbd(r.min + Vec2::new(6.0, 5.0) * cv.s, Align2::LEFT_TOP, LEFT_ALONE[alone], 9.5, 40.0, MUTE);
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

    fn pad_card(&self, cv: &Cv, x: f32, y: f32, st: &Status, mode: Mode) {
        use Target::*;
        let (s, p, con) = (cv.s, st.pad, self.cfg.console);
        let keys = |t: Target| keys_on(self.cfg.layout(mode), t);
        cv.text(cv.r(x + 18.0, y + 14.0, 0.0, 0.0).min, Align2::LEFT_TOP, "Controller", 12.0, DIM);
        let sticks = [(p.lx, p.ly, L3, Some([LsUp, LsDown, LsLeft, LsRight])), (p.rx, p.ry, R3, None)];
        for (i, (sx, sy, click, dirs)) in sticks.into_iter().enumerate() {
            let c = cv.r(x + 56.0 + i as f32 * 84.0, y + 96.0, 0.0, 0.0).min;
            cv.ui.painter().circle_stroke(c, 34.0 * s, Stroke::new(1.0, if p.has(click) { TEXT } else { EDGE }));
            match dirs {
                Some(dirs) => {
                    for (t, d) in dirs.into_iter().zip([Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)]) {
                        cv.kbd(c + d * 22.0 * s, Align2::CENTER_CENTER, &keys(t), 10.5, 24.0, if p.has(t) { TEXT } else { DIM });
                    }
                }
                None => {
                    cv.text(c, Align2::CENTER_CENTER, "Mouse", 11.0, DIM);
                }
            }
            if sx != 0.0 || sy != 0.0 {
                cv.ui.painter().circle_filled(c + Vec2::new(sx, -sy) * 28.0 * s, 5.0 * s, TEXT);
            }
            let label = format!("{}  {}", model::button_name(click, con), keys(click));
            cv.fit(c + Vec2::new(0.0, 52.0 * s), Align2::CENTER_CENTER, label.trim_end(), 11.0, 78.0, if p.has(click) { TEXT } else { DIM });
        }
        let chips: [&[Target]; 4] = [&[L2, L1, R1, R2], &[Triangle, Circle, Cross, Square], &[DUp, DDown, DLeft, DRight], &[Share, Touchpad, Options, Ps]];
        for (ri, row) in chips.iter().enumerate() {
            for (ci, &t) in row.iter().enumerate() {
                if con == Console::Xbox && t == Touchpad {
                    continue;
                }
                let r = cv.r(x + 186.0 + ci as f32 * 67.0, y + 38.0 + ri as f32 * 41.0, 63.0, 37.0);
                let on = p.has(t) || (t == L2 && p.lt > 0.0) || (t == R2 && p.rt > 0.0);
                let c = color(t, con);
                cv.fill(r, 6.0, if on { c } else { KEY });
                let name = model::button_name(t, con).trim_start_matches("D-pad ");
                let size = if matches!(t, Cross | Circle | Square | Triangle) && con == Console::Ps { 13.0 } else { 10.0 };
                cv.fit(Pos2::new(r.center().x, r.top() + 11.0 * s), Align2::CENTER_CENTER, name, size, 57.0, if on { BG } else { c });
                cv.kbd(Pos2::new(r.center().x, r.bottom() - 11.0 * s), Align2::CENTER_CENTER, &keys(t), 11.0, 57.0, if on { BG } else { TEXT });
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
        let (pw, ph) = (576.0, 488.0);
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
            let full = !on && t.is_button() && model::keys_on(self.cfg.layout(mode), t).len() >= model::MAX_KEYS;
            let c = if full { MUTE } else { color(t, self.cfg.console) };
            cv.fill(r, 8.0, if on || resp.hovered() && !full { HOVER } else { KEY });
            if on {
                cv.outline(r, 8.0, c);
            }
            let face = self.cfg.console == Console::Ps && matches!(t, Target::Cross | Target::Circle | Target::Square | Target::Triangle);
            let room = if t.is_button() { 62.0 } else { 108.0 };
            cv.fit(r.left_center() + Vec2::new(10.0 * s, 0.0), Align2::LEFT_CENTER, model::button_name(t, self.cfg.console), if face { 18.0 } else { 13.0 }, room, c);
            if t.is_button() {
                cv.fit(r.right_center() - Vec2::new(8.0 * s, 0.0), Align2::RIGHT_CENTER, model::action(t, mode), 11.0, 52.0, DIM);
            }
            if resp.clicked() && !full {
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

const SHELL: Color32 = rgb(0x262626);
const CAP: Color32 = rgb(0x101010);
const LEAD: Color32 = rgb(0x3c3c3c);

/// The controller's outline, seen from the front or the back.
fn shell(v: &Cv) {
    let p = v.ui.painter();
    for (a, b) in [((178.0, 150.0), (152.0, 262.0)), ((402.0, 150.0), (428.0, 262.0))] {
        let (a, b) = (v.p(a.0, a.1), v.p(b.0, b.1));
        p.line_segment([a, b], Stroke::new(76.0 * v.s, SHELL));
        p.circle_filled(a, 38.0 * v.s, SHELL);
        p.circle_filled(b, 38.0 * v.s, SHELL);
    }
    v.fill(v.r(150.0, 58.0, 280.0, 128.0), 50.0, SHELL);
}

impl App {
    /// Front, top and back of the controller, with the keys on each button. Click one to change its keys.
    fn pad_view(&mut self, cv: &Cv, mode: Mode, st: &Status) {
        use Target::*;
        let (con, p) = (self.cfg.console, st.pad);
        let ps = con == Console::Ps;
        let layout = self.cfg.layout(mode);
        let open = std::cell::Cell::new(None);
        let on = |t: Target| p.has(t) || (t == L2 && p.lt > 0.0) || (t == R2 && p.rt > 0.0);
        let hit = |v: &Cv, t: Target, r: Rect, n: u8| {
            let resp = v.hit(r, ("pad", t as u8, n), Sense::click());
            if resp.clicked() {
                open.set(Some(t));
            }
            resp.hovered()
        };
        let face = |t: Target, hov: bool| if on(t) { color(t, con) } else if hov { HOVER } else { CAP };
        let ink = |t: Target| if on(t) { BG } else { color(t, con) };
        // the keys on a button, as a clickable label
        let chip = |v: &Cv, t: Target, x: f32, y: f32, a: Align2, w: Option<f32>| {
            let k = keys_on(layout, t);
            let text = if k.is_empty() { "+" } else { k.as_str() };
            let w = w.unwrap_or((v.kbd_width(text, 11.0) + 16.0).max(30.0));
            let r = a.anchor_size(v.p(x, y), Vec2::new(w, 22.0) * v.s);
            let hov = hit(v, t, r, 1);
            v.fill(r, 6.0, if hov { HOVER } else { KEY });
            v.outline(r, 6.0, rgb(0x303030));
            let c = if on(t) { color(t, con) } else if k.is_empty() { MUTE } else { TEXT };
            v.kbd(r.center(), Align2::CENTER_CENTER, text, 11.0, w - 6.0, c);
        };
        let lead = |v: &Cv, pts: &[(f32, f32)]| {
            for w in pts.windows(2) {
                v.ui.painter().line_segment([v.p(w[0].0, w[0].1), v.p(w[1].0, w[1].1)], Stroke::new(1.0, LEAD));
            }
        };
        let y0 = 124.0;
        for (x, w, name) in [(24.0, 620.0, "Front"), (660.0, 240.0, "Top"), (916.0, 240.0, "Back")] {
            cv.card(cv.r(x, y0, w, 322.0));
            cv.text(cv.p(x + 18.0, y0 + 14.0), Align2::LEFT_TOP, name, 12.0, DIM);
        }
        let at = |x: f32, y: f32, k: f32| Cv { ui: cv.ui, o: cv.p(x, y), s: cv.s * k, active: cv.active };

        // front
        let v = at(24.0 + 310.0 - 290.5 * 1.08, y0 + 167.0 - 164.5 * 1.08, 1.08);
        shell(&v);
        lead(&v, &[(224.0, 64.0), (224.0, 40.0), (124.0, 40.0)]);
        lead(&v, &[(356.0, 64.0), (356.0, 40.0), (456.0, 40.0)]);
        lead(&v, &[(186.0, 89.0), (124.0, 89.0)]);
        lead(&v, &[(163.0, 112.0), (124.0, 112.0)]);
        lead(&v, &[(186.0, 135.0), (124.0, 135.0)]);
        lead(&v, &[(219.0, 122.0), (219.0, 158.0), (124.0, 158.0)]);
        lead(&v, &[(396.0, 87.0), (456.0, 87.0)]);
        lead(&v, &[(421.0, 112.0), (456.0, 112.0)]);
        lead(&v, &[(396.0, 137.0), (456.0, 137.0)]);
        lead(&v, &[(361.0, 122.0), (361.0, 158.0), (456.0, 158.0)]);
        lead(&v, &[(221.0, 180.0), (136.0, 180.0), (136.0, 240.0), (121.0, 240.0)]);
        lead(&v, &[(359.0, 180.0), (444.0, 180.0), (444.0, 240.0), (459.0, 240.0)]);
        lead(&v, &[(244.0, 195.0), (244.0, 204.0)]);
        lead(&v, &[(336.0, 195.0), (336.0, 204.0)]);
        lead(&v, &[(290.0, 180.0), (290.0, 240.0)]);
        for (t, dx, dy) in [(DUp, 0.0, -23.0), (DDown, 0.0, 23.0), (DLeft, -23.0, 0.0), (DRight, 23.0, 0.0)] {
            let r = v.r(186.0 + dx, 102.0 + dy, 20.0, 20.0);
            v.fill(r, 5.0, face(t, hit(&v, t, r, 0)));
            v.text(r.center(), Align2::CENTER_CENTER, model::button_name(t, con).trim_start_matches("D-pad "), 10.0, ink(t));
        }
        for (t, dx, dy) in [(Triangle, 0.0, -25.0), (Circle, 25.0, 0.0), (Cross, 0.0, 25.0), (Square, -25.0, 0.0)] {
            let c = v.p(384.0 + dx, 112.0 + dy);
            let hov = hit(&v, t, Rect::from_center_size(c, Vec2::splat(24.0 * v.s)), 0);
            v.ui.painter().circle_filled(c, 12.0 * v.s, face(t, hov));
            v.text(c, Align2::CENTER_CENTER, model::button_name(t, con), if ps { 12.0 } else { 11.0 }, ink(t));
        }
        for (t, x) in [(Share, 220.0), (Options, 352.0)] {
            let r = v.r(x, 64.0, 8.0, 16.0);
            v.fill(r, 4.0, face(t, hit(&v, t, r.expand(4.0 * v.s), 0)));
        }
        if ps {
            let r = v.r(236.0, 58.0, 108.0, 58.0);
            let hov = hit(&v, Touchpad, r, 0);
            v.fill(r, 12.0, if hov { HOVER } else { KEY });
            if on(Touchpad) {
                v.outline(r, 12.0, color(Touchpad, con));
            }
            let k = keys_on(layout, Touchpad);
            v.kbd(r.center(), Align2::CENTER_CENTER, if k.is_empty() { "+" } else { &k }, 11.5, 96.0, if k.is_empty() { MUTE } else { TEXT });
        }
        for (t, x, sx, sy) in [(L3, 244.0, p.lx, p.ly), (R3, 336.0, p.rx, p.ry)] {
            let c = v.p(x, 172.0);
            let hov = hit(&v, t, Rect::from_center_size(c, Vec2::splat(46.0 * v.s)), 0);
            v.ui.painter().circle_filled(c, 23.0 * v.s, face(t, hov));
            v.ui.painter().circle_filled(c + Vec2::new(sx, -sy) * 7.0 * v.s, 14.0 * v.s, rgb(0x343434));
        }
        let c = v.p(290.0, 172.0);
        let hov = hit(&v, Ps, Rect::from_center_size(c, Vec2::splat(20.0 * v.s)), 0);
        v.ui.painter().circle_filled(c, 8.0 * v.s, face(Ps, hov));
        for (t, y) in [(Share, 40.0), (DUp, 89.0), (DLeft, 112.0), (DDown, 135.0), (DRight, 158.0)] {
            chip(&v, t, 120.0, y, Align2::RIGHT_CENTER, None);
        }
        for (t, y) in [(Options, 40.0), (Triangle, 87.0), (Circle, 112.0), (Cross, 137.0), (Square, 158.0)] {
            chip(&v, t, 460.0, y, Align2::LEFT_CENTER, None);
        }
        for (ts, x) in [([LsUp, LsLeft, LsDown, LsRight], 66.0), ([RsUp, RsLeft, RsDown, RsRight], 514.0)] {
            for (t, dx, y) in [(ts[0], 0.0, 214.0), (ts[1], -36.0, 240.0), (ts[2], 0.0, 240.0), (ts[3], 36.0, 240.0)] {
                chip(&v, t, x + dx, y, Align2::CENTER_CENTER, Some(34.0));
            }
        }
        // hold one of these and the mouse points that stick
        for (t, x) in [(LsHold, 66.0), (RsHold, 514.0)] {
            v.text(v.p(x - 53.0, 270.0), Align2::LEFT_CENTER, "Mouse", 10.5, DIM);
            chip(&v, t, x + 53.0, 270.0, Align2::RIGHT_CENTER, None);
        }
        chip(&v, L3, 244.0, 215.0, Align2::CENTER_CENTER, None);
        chip(&v, R3, 336.0, 215.0, Align2::CENTER_CENTER, None);
        chip(&v, Ps, 290.0, 251.0, Align2::CENTER_CENTER, None);

        // top: the shoulder buttons
        let v = at(660.0, y0, 1.0);
        v.fill(v.r(10.0, 138.0, 220.0, 76.0), 26.0, SHELL);
        for (trigger, bumper, x) in [(L2, L1, 16.0), (R2, R1, 128.0)] {
            for (t, y, h) in [(trigger, 96.0, 50.0), (bumper, 160.0, 34.0)] {
                let r = v.r(x, y, 96.0, h);
                let hov = hit(&v, t, r, 0);
                v.fill(r, 10.0, if on(t) { color(t, con) } else if hov { HOVER } else { rgb(0x303030) });
                let k = keys_on(layout, t);
                let (text, c) = if k.is_empty() { ("+", MUTE) } else { (k.as_str(), if on(t) { BG } else { TEXT }) };
                let name = model::button_name(t, con);
                if t == trigger {
                    v.text(Pos2::new(r.center().x, r.top() + 14.0 * v.s), Align2::CENTER_CENTER, name, 11.0, ink(t));
                    v.kbd(Pos2::new(r.center().x, r.bottom() - 16.0 * v.s), Align2::CENTER_CENTER, text, 12.0, 86.0, c);
                } else {
                    v.text(r.left_center() + Vec2::new(12.0 * v.s, 0.0), Align2::LEFT_CENTER, name, 11.0, ink(t));
                    v.kbd(r.right_center() - Vec2::new(12.0 * v.s, 0.0), Align2::RIGHT_CENTER, text, 12.0, 52.0, c);
                }
            }
        }

        v.text(v.p(16.0, 250.0), Align2::LEFT_CENTER, "Pressure by mouse", 10.5, DIM);
        chip(&v, Triggers, 224.0, 250.0, Align2::RIGHT_CENTER, None);

        // back: the triggers, mirrored
        shell(&at(916.0 + 120.0 - 290.0 * 0.62, y0 + 60.0, 0.62));
        let v = at(916.0, y0, 1.0);
        for (t, x) in [(R2, 34.0), (L2, 142.0)] {
            let r = v.r(x, 82.0, 64.0, 30.0);
            let hov = hit(&v, t, r, 2);
            v.fill(r, 10.0, if on(t) { color(t, con) } else if hov { HOVER } else { rgb(0x303030) });
            v.text(r.center(), Align2::CENTER_CENTER, model::button_name(t, con), 11.0, ink(t));
            lead(&v, &[(x + 32.0, 82.0), (x + 32.0, 70.0)]);
            chip(&v, t, x + 32.0, 58.0, Align2::CENTER_CENTER, None);
        }

        if let Some(t) = open.get() {
            self.btn = Some(t);
        }
    }

    /// The key that was just pressed, while a slot is waiting for one.
    fn heard(&mut self, ui: &egui::Ui) -> Option<u16> {
        let (_, was) = self.listen.as_mut()?;
        if !ui.input(|i| i.focused) {
            return None;
        }
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            return Some(if wheel > 0.0 { WHEEL_UP } else { WHEEL_DOWN });
        }
        let mut hit = None;
        for &(code, ..) in model::KEYS.iter().filter(|k| k.0 < 0x100 && k.0 != 0x1B) {
            let down = key_down(code);
            if down && !was[code as usize] && hit.is_none() {
                hit = Some(code);
            }
            was[code as usize] = down;
        }
        hit
    }

    fn capture(&mut self, t: Target, mode: Mode, code: u16) {
        if let Some((slot, _)) = self.listen.take() {
            self.cfg.set_key(mode, t, slot, Some(code));
            self.changed();
            self.hold = true;
        }
    }

    /// The two key slots of one controller button.
    fn popup(&mut self, cv: &Cv, t: Target, mode: Mode, full: Rect) {
        let s = cv.s;
        let back = cv.hit(full, "backdrop2", Sense::click());
        cv.ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(170));
        let codes = model::keys_on(self.cfg.layout(mode), t);
        let slots = if t.is_button() { model::MAX_KEYS } else { (codes.len() + 1).min(6) };
        let (pw, ph) = (420.0, 140.0 + slots as f32 * 48.0);
        let (px, py) = ((W - pw) / 2.0, (H - ph) / 2.0);
        let panel = cv.r(px, py, pw, ph);
        cv.hit(panel, "panel2", Sense::click());
        cv.fill(panel, 12.0, PANEL);
        cv.outline(panel, 12.0, EDGE);
        let con = self.cfg.console;
        let name = model::button_name(t, con);
        let layout = if mode == Mode::Foot { "On foot" } else { "Driving" };
        let title = if t.is_button() { format!("{name}  ·  {}  ·  {layout}", model::action(t, mode)) } else { format!("{name}  ·  {layout}") };
        cv.text(cv.p(px + 22.0, py + 18.0), Align2::LEFT_TOP, &title, 16.0, TEXT);
        let (mut start, mut clear) = (None, None);
        for i in 0..slots {
            let r = cv.r(px + 22.0, py + 58.0 + i as f32 * 48.0, pw - 44.0, 40.0);
            let code = codes.get(i).copied();
            let hear = self.listen.as_ref().is_some_and(|l| l.0 == i);
            let resp = cv.hit(r, ("slot", i), Sense::click());
            cv.fill(r, 8.0, if resp.hovered() { HOVER } else { KEY });
            let left = r.left_center() + Vec2::new(14.0 * s, 0.0);
            if hear {
                cv.outline(r, 8.0, AMBER);
                cv.text(left, Align2::LEFT_CENTER, "Press a key or mouse button", 13.0, AMBER);
            } else if let Some(code) = code {
                cv.kbd(left, Align2::LEFT_CENTER, cap(code), 13.5, 300.0, TEXT);
                let xr = Rect::from_min_max(Pos2::new(r.right() - 40.0 * s, r.top()), r.max);
                let x = cv.hit(xr, ("slot-x", i), Sense::click());
                cv.text(xr.center(), Align2::CENTER_CENTER, "✕", 11.0, if x.hovered() { RED } else { MUTE });
                if x.clicked() {
                    clear = Some(i);
                }
            } else {
                cv.text(left, Align2::LEFT_CENTER, "Click, then press a key", 13.0, MUTE);
            }
            if resp.clicked() && clear.is_none() {
                start = Some(i);
            }
        }
        let fy = py + ph - 50.0;
        if self.listen.is_some() {
            cv.text(cv.p(px + 22.0, fy + 16.0), Align2::LEFT_CENTER, "Esc cancels", 12.0, DIM);
        }
        let done = cv.button(cv.r(px + pw - 112.0, fy, 90.0, 32.0), "done2", "Done", BG, TEXT);
        let esc = cv.ui.input(|i| i.key_pressed(egui::Key::Escape));
        if let Some(i) = clear {
            self.cfg.set_key(mode, t, i, None);
            self.changed();
        } else if let Some(i) = start {
            self.listen = Some((i, vec![true; 0x100]));
        } else if self.listen.is_some() {
            if esc {
                self.listen = None;
            }
        } else if done || back.clicked() || esc {
            self.btn = None;
        }
    }
}

#[cfg(not(test))]
fn key_down(code: u16) -> bool {
    // high bit: down now. low bit: went down since the last call, so a quick tap between frames still counts.
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(code as i32) as u16 & 0x8001 != 0 }
}

#[cfg(test)]
thread_local!(static DOWN: std::cell::Cell<u16> = const { std::cell::Cell::new(0) });

#[cfg(test)]
fn key_down(code: u16) -> bool {
    DOWN.get() == code
}

/// Segoe UI for text, Consolas for key names and numbers, Segoe UI Symbol for the button glyphs.
pub fn fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:/Windows".into());
    use egui::FontFamily::{Monospace, Proportional};
    // (name, file, family it leads). Every font is also a fallback in both families, for arrows and glyphs.
    for (name, file, lead) in [("segoe", "segoeui.ttf", Some(Proportional)), ("mono", "consola.ttf", Some(Monospace)), ("symbols", "seguisym.ttf", None)] {
        if let Ok(bytes) = std::fs::read(format!("{dir}/Fonts/{file}")) {
            fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
            for fam in [Proportional, Monospace] {
                let list = fonts.families.entry(fam.clone()).or_default();
                if lead.as_ref() == Some(&fam) { list.insert(0, name.into()) } else { list.push(name.into()) }
            }
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
        let modal = self.pick.is_some() || self.btn.is_some();
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
        if let Some(i) = cv.segmented(cv.r(24.0, 78.0, 220.0, 34.0), "view", ["Keyboard", "Controller"], self.pad as usize) {
            self.pad = i == 1;
        }
        if let Some(i) = cv.segmented(cv.r(260.0, 78.0, 220.0, 34.0), "mode", ["On foot", "Driving"], (mode == Mode::Drive) as usize) {
            let mut core = self.sh.core.lock().unwrap();
            if (core.mode == Mode::Drive) as usize != i {
                core.flip();
            }
        }
        let state = self.update.lock().unwrap().clone();
        let (label, fg) = match &state {
            Update::Unknown => (format!("v{}  ·  Check for updates", update::VERSION), DIM),
            Update::Checking => ("Checking for updates".to_string(), MUTE),
            Update::Current => (format!("v{}  ·  Up to date", update::VERSION), DIM),
            Update::Newer(tag) => (format!("Get {tag}"), GREEN),
            Update::Failed => ("Update check failed  ·  Retry".to_string(), DIM),
        };
        if cv.button(cv.r(W - 24.0 - 130.0 - 8.0 - 200.0, 80.0, 200.0, 30.0), "update", &label, fg, BG) {
            match state {
                Update::Newer(_) => cv.ui.ctx().open_url(egui::OpenUrl::new_tab(update::PAGE)),
                Update::Checking => {}
                _ => update::check(&self.update),
            }
        }
        let now = cv.ui.input(|i| i.time);
        let sure = now < self.reset_until;
        if cv.button(cv.r(W - 24.0 - 130.0, 80.0, 130.0, 30.0), "reset", if sure { "Click again" } else { "Reset layouts" }, if sure { RED } else { DIM }, BG) {
            if sure {
                let d = Config::default();
                self.cfg.foot = d.foot;
                self.cfg.drive = d.drive;
                self.changed();
                self.reset_until = 0.0;
            } else {
                self.reset_until = now + 3.0;
            }
        }
        let hint = if self.pad { "Click a button to change its keys" } else { "Click a key to change it" };
        cv.text(cv.p(498.0, 95.0), Align2::LEFT_CENTER, hint, 12.0, MUTE);

        if self.pad {
            self.pad_view(&cv, mode, &st);
        } else {
            self.keyboard(&cv, mode);
        }

        let (cy, ch) = (462.0, 214.0);
        let cards = [(24.0, 372.0), (412.0, 468.0), (896.0, 260.0)];
        for (x, w) in cards {
            cv.card(cv.r(x, cy, w, ch));
        }
        self.aim_card(&cv, cards[0].0, cy, cards[0].1);
        self.apps_card(&cv, cards[2].0, cy, cards[2].1, &st, live);
        self.pad_card(&cv, cards[1].0, cy, &st, mode);

        if let Some(code) = self.pick {
            self.picker(&Cv { ui, o, s, active: true }, code, mode, full);
        }
        if let Some(t) = self.btn {
            if let Some(code) = self.heard(ui) {
                self.capture(t, mode, code);
            }
            self.hold &= ui.input(|i| i.pointer.any_down());
            self.popup(&Cv { ui, o, s, active: self.listen.is_none() && !self.hold }, t, mode, full);
        }

    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
        let ctx = ui.ctx().clone();
        let live = self.sh.live.load(Relaxed);
        ctx.request_repaint_after(std::time::Duration::from_millis(if self.listen.is_some() { 10 } else if live { 16 } else { 100 }));
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
        /// The last web page the UI asked the system to open.
        opened: Option<String>,
    }

    impl Rig {
        fn new(name: &str) -> Rig {
            let path = std::env::temp_dir().join(format!("padwire-ui-{}-{name}.json", std::process::id()));
            let cfg = Config::default();
            let sh = Shared::new(Core::new(&cfg), cfg.apps.clone(), cfg.console);
            let mut rig = Rig { ctx: egui::Context::default(), app: App::new(sh, cfg, path), t: 0.0, opened: None };
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
            let mut out = self.ctx.run_ui(input, |ui| app.draw(ui));
            out.textures_delta.clear();
            for c in out.platform_output.commands {
                if let egui::OutputCommand::OpenUrl(o) = c {
                    self.opened = Some(o.url);
                }
            }
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
    const KEY_U: (f32, f32) = (453.0, 256.0);
    const CHIP_CROSS: (f32, f32) = (388.0, 182.0);
    const CHIP_SQUARE: (f32, f32) = (656.0, 182.0);
    const HOLD: (f32, f32) = (419.0, 558.0);
    const CLEAR: (f32, f32) = (711.0, 558.0);
    const DONE: (f32, f32) = (811.0, 558.0);

    #[test]
    fn rebinding_a_key() {
        let mut r = Rig::new("rebind");
        r.click(KEY_U.0, KEY_U.1);
        assert_eq!(r.app.pick, Some(0x55), "clicking U opens its picker");
        r.click(CHIP_CROSS.0, CHIP_CROSS.1);
        r.click(CHIP_SQUARE.0, CHIP_SQUARE.1);
        r.click(HOLD.0, HOLD.1);
        r.click(DONE.0, DONE.1);
        assert_eq!(r.app.pick, None);
        let want = Bind { to: vec![Cross, Square], toggle: true };
        assert_eq!(r.app.cfg.foot["U"], want);
        assert_eq!(r.saved().foot["U"], want, "written to disk");
        assert_eq!(r.saved().drive.get("U"), None, "only the layout on screen changes");
        // the running engine got it too: U now latches Cross + Square
        let mut core = r.app.sh.core.lock().unwrap();
        assert!(core.press(0x55, 0.0));
        let pad = core.tick(0.0, 0.001);
        assert!(pad.has(Cross) && pad.has(Square));
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
        r.click(425.0, 95.0);
        assert_eq!(r.app.sh.mode(), Mode::Drive);
        r.click(1081.0, 40.0);
        assert!(r.app.sh.armed.load(Relaxed));
        r.click(1081.0, 40.0);
        assert!(!r.app.sh.armed.load(Relaxed));
        r.app.cfg.foot.clear();
        r.click(1091.0, 95.0);
        assert!(r.app.cfg.foot.is_empty(), "the first click only asks");
        r.click(1091.0, 95.0);
        assert_eq!(r.app.cfg.foot, Config::default().foot, "reset layouts");
    }

    #[test]
    fn a_newer_release_becomes_a_download_button() {
        let mut r = Rig::new("update");
        *r.app.update.lock().unwrap() = Update::Newer("v9.9.9".into());
        r.click(918.0, 95.0);
        assert_eq!(r.opened.as_deref(), Some(update::PAGE), "it opens the release page and downloads nothing itself");
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
        r.click(1000.0, 513.0);
        assert_eq!(r.app.cfg.apps.last().unwrap(), "obs64.exe", "apps already listed are not offered again");
        assert_eq!(r.app.sh.want.lock().unwrap().0, r.app.cfg.apps);
        assert_eq!(r.saved().apps, r.app.cfg.apps);
    }

    #[test]
    fn controller_view_rebinds_a_button() {
        let mut r = Rig::new("padview");
        r.click(189.0, 95.0);
        assert!(r.app.pad, "the Controller tab opens the controller view");
        r.click(435.0, 261.0);
        assert_eq!(r.app.btn, Some(Cross), "clicking the drawn button opens its keys");
        r.click(560.0, 358.0);
        assert_eq!(r.app.listen.as_ref().map(|l| l.0), Some(1), "the empty slot waits for a key");
        DOWN.set(0x55);
        r.frame(vec![]);
        DOWN.set(0);
        assert_eq!(r.app.listen, None, "pressing U fills the slot");
        assert_eq!(model::keys_on(&r.app.cfg.foot, Cross), [0x55, 0x20], "U joins Space");
        assert_eq!(r.saved().foot["U"], Bind { to: vec![Cross], toggle: false });
        // a key that was on another button moves over
        r.click(560.0, 310.0);
        r.app.capture(Cross, Mode::Foot, 0x51);
        r.frame(vec![]);
        assert_eq!(model::keys_on(&r.app.cfg.foot, Cross), [0x51, 0x20], "Q replaces U in slot one");
        assert_eq!(model::keys_on(&r.app.cfg.foot, R1), [0x06], "Q left R1");
        // the cross takes a key off
        r.click(760.0, 310.0);
        assert_eq!(model::keys_on(&r.app.cfg.foot, Cross), [0x20]);
        assert_eq!(r.saved().foot.get("Q"), None);
        // a mouse button can be the key: the click that picks it does nothing else
        DOWN.set(0x01);
        r.click(560.0, 358.0);
        assert_eq!(r.app.listen.as_ref().map(|l| l.0), Some(1), "the click that starts listening is not taken as the key");
        DOWN.set(0);
        r.frame(vec![]);
        DOWN.set(0x01);
        r.button(733.0, 434.0, true);
        assert_eq!(model::keys_on(&r.app.cfg.foot, Cross), [0x20, 0x01], "left mouse button bound");
        DOWN.set(0);
        r.button(733.0, 434.0, false);
        r.frame(vec![]);
        assert_eq!(r.app.btn, Some(Cross), "that press did not hit Done");
        assert!(model::keys_on(&r.app.cfg.foot, R2).is_empty(), "it left R2");
        r.click(733.0, 434.0);
        assert_eq!(r.app.btn, None, "Done closes it");
        // the label next to a button opens the same thing
        r.click(134.0, 209.0);
        assert_eq!(r.app.btn, Some(DUp));
    }

    #[test]
    fn a_mode_is_added_on_top_of_a_key() {
        let mut cfg = Config::default();
        let slot = model::keys_on(&cfg.foot, RsHold).len();
        cfg.set_key(Mode::Foot, RsHold, slot, Some(0x51)); // Q, which is cover
        assert_eq!(cfg.foot["Q"].to, [R1, RsHold], "Q keeps cover and also hands the stick to the mouse");
        cfg.set_key(Mode::Foot, Cross, 1, Some(0x51));
        assert_eq!(cfg.foot["Q"].to, [Cross], "given to a button, a key moves there");
    }

    #[test]
    fn a_button_takes_two_keys_at_most() {
        let d = Config::default();
        for (name, l) in [("foot", &d.foot), ("drive", &d.drive)] {
            for t in TARGETS.into_iter().filter(|t| t.is_button()) {
                assert!(model::keys_on(l, t).len() <= model::MAX_KEYS, "{name} {t:?}");
            }
        }
        // the key picker refuses a third key: Triangle already has E and F
        let mut r = Rig::new("max");
        r.click(KEY_U.0, KEY_U.1);
        r.click(790.0, 182.0);
        assert_eq!(r.app.cfg.foot.get("U"), None);
        // a config file with three is trimmed on load
        r.app.cfg.foot.insert("U".into(), Bind { to: vec![Triangle], toggle: false });
        r.app.cfg.save(&r.app.path);
        assert_eq!(model::keys_on(&r.saved().foot, Triangle).len(), 2);
    }
}
