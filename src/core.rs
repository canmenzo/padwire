//! Input state -> controller state. No OS calls, so all of it is unit tested.
use crate::model::{Aim, Bind, CODES, Config, Mode, Target, WHEEL_DOWN, WHEEL_UP, key_code};
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pad {
    /// One bit per `Target` (stick directions and Switch are never set).
    pub buttons: u32,
    /// Sticks -1..1, y up. Triggers 0..1.
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub lt: f32,
    pub rt: f32,
}

impl Pad {
    pub fn has(&self, t: Target) -> bool {
        self.buttons & t.bit() != 0
    }
}

const WINDOW: f64 = 0.024; // mouse counts are averaged over this; shorter flickers between reports
const FULL: f64 = 6000.0; // counts/s that mean full tilt at speed 1
const GLIDE: f64 = 0.018;
const STOP: f64 = 0.012;
const PULSE: f64 = 0.07; // how long a wheel notch holds its button
const POINT: f32 = 140.0; // mouse counts from centre to full tilt while the mouse points a stick
const LINGER: f64 = 0.06; // a pointed stick stays put this long after its key, so a wheel pick lands
const RESWITCH: f64 = 0.4; // a second layout switch this soon is a double tap, not a change of mind

/// A stick or the triggers handed to the mouse while a key is held.
#[derive(Clone, Copy, Default)]
struct Pointed {
    on: bool,
    since: f64,
    until: f64,
    x: f32,
    y: f32,
}

/// Mouse speed -> right stick tilt. The camera stops when the mouse does.
#[derive(Default)]
struct Mouse {
    ev: VecDeque<(f64, f64, f64)>,
    moving: bool,
    sx: f64,
    sy: f64,
}

impl Mouse {
    fn reset(&mut self) {
        *self = Mouse::default();
    }

    fn target(&mut self, a: &Aim) -> (f64, f64) {
        let vx = self.ev.iter().map(|e| e.1).sum::<f64>() / WINDOW;
        let vy = self.ev.iter().map(|e| e.2).sum::<f64>() / WINDOW;
        let speed = vx.hypot(vy);
        let full = FULL / a.speed.max(0.05) as f64;
        // start at 3 % of full speed, keep going down to 1 %: a slow hand must not flicker on one line
        if speed < full * if self.moving { 0.01 } else { 0.03 } {
            self.moving = false;
            return (0.0, 0.0);
        }
        self.moving = true;
        let push = (a.push as f64).clamp(0.0, 0.9);
        let d = (speed / full).min(1.0).powf(a.curve.max(0.2) as f64);
        let out = (push + (1.0 - push) * d).min(1.0);
        (vx / speed * out, vy / speed * out)
    }

    /// Stick as (x right, y down), each -1..1.
    fn step(&mut self, now: f64, dt: f64, a: &Aim) -> (f32, f32) {
        while self.ev.front().is_some_and(|e| now - e.0 >= WINDOW) {
            self.ev.pop_front();
        }
        let (tx, ty) = self.target(a);
        let k = (-dt / if tx == 0.0 && ty == 0.0 { STOP } else { GLIDE }).exp();
        self.sx = self.sx * k + tx * (1.0 - k);
        self.sy = self.sy * k + ty * (1.0 - k);
        if self.sx.abs() < 1.0 / 127.0 && self.sy.abs() < 1.0 / 127.0 {
            self.sx = 0.0;
            self.sy = 0.0;
        }
        (self.sx as f32, self.sy as f32)
    }
}

pub struct Core {
    pub mode: Mode,
    maps: [Vec<Option<Bind>>; 2],
    aim: Aim,
    held: [bool; CODES],
    latched: [bool; CODES],
    pulse: [f64; 2],
    mouse: Mouse,
    steer: f32,
    combo_t: f64,
    /// When and by which key the layout last switched.
    switched: (f64, u16),
    /// Left stick, right stick, triggers.
    pointed: [Pointed; 3],
}

fn compile(cfg: &Config, m: Mode) -> Vec<Option<Bind>> {
    let mut v = vec![None; CODES];
    for (k, b) in cfg.layout(m) {
        if let Some(c) = key_code(k) {
            v[c as usize] = Some(b.clone());
        }
    }
    v
}

impl Core {
    pub fn new(cfg: &Config) -> Core {
        Core {
            mode: Mode::Foot,
            maps: [compile(cfg, Mode::Foot), compile(cfg, Mode::Drive)],
            aim: cfg.aim,
            held: [false; CODES],
            latched: [false; CODES],
            pulse: [0.0; 2],
            mouse: Mouse::default(),
            steer: 0.0,
            combo_t: -1.0,
            switched: (-1.0, 0),
            pointed: [Pointed::default(); 3],
        }
    }

    pub fn set_config(&mut self, cfg: &Config) {
        self.maps = [compile(cfg, Mode::Foot), compile(cfg, Mode::Drive)];
        self.aim = cfg.aim;
        self.latched = [false; CODES];
    }

    /// Lets go of everything. The layout stays where it is.
    pub fn reset(&mut self) {
        self.held = [false; CODES];
        self.latched = [false; CODES];
        self.pulse = [0.0; 2];
        self.mouse.reset();
        self.steer = 0.0;
        self.pointed = [Pointed::default(); 3];
    }

    fn bind(&self, code: u16) -> Option<&Bind> {
        self.maps[self.mode as usize][code as usize].as_ref()
    }

    fn bound(&self, code: u16) -> bool {
        self.maps.iter().any(|m| m[code as usize].is_some())
    }

    pub fn flip(&mut self) {
        self.mode = self.mode.other();
        self.latched = [false; CODES];
    }

    /// A key asked for the other layout. Double-tapping it (bailing out of a car) switches once.
    fn switch(&mut self, now: f64, code: u16) {
        if (now - self.switched.0 > RESWITCH) || code != self.switched.1 {
            self.flip();
            self.switched = (now, code);
        }
    }

    /// Key or mouse button went down. True = swallow it.
    pub fn press(&mut self, code: u16, now: f64) -> bool {
        let c = code as usize;
        let mouse_button = code <= 6;
        if c >= CODES || !(mouse_button || self.bound(code)) {
            return false;
        }
        if self.held[c] {
            return true; // key repeat
        }
        self.held[c] = true;
        if let Some(b) = self.bind(code) {
            let (mask, toggle) = (b.mask(), b.toggle);
            if mask & Target::L3.bit() != 0 && mask & Target::R3.bit() != 0 {
                self.combo_t = now;
            }
            if toggle {
                self.latched[c] = !self.latched[c];
            }
            if mask & Target::Switch.bit() != 0 {
                self.switch(now, code);
            }
        }
        true
    }

    /// True = swallow it. Only an "up" whose "down" was swallowed, so nothing is left half pressed.
    pub fn release(&mut self, code: u16) -> bool {
        let c = code as usize;
        c < CODES && std::mem::replace(&mut self.held[c], false)
    }

    pub fn wheel(&mut self, up: bool, now: f64) -> bool {
        let code = if up { WHEEL_UP } else { WHEEL_DOWN };
        if !self.bound(code) {
            return false;
        }
        self.pulse[!up as usize] = now + PULSE;
        if self.bind(code).is_some_and(|b| b.to.contains(&Target::Switch)) {
            self.switch(now, code);
        }
        true
    }

    pub fn mouse_move(&mut self, now: f64, dx: i32, dy: i32) {
        // the stick (or triggers) whose key went down last gets the mouse; the others stay where they were put
        let latest = self.pointed.iter().filter(|p| p.on).map(|p| p.since).fold(f64::MIN, f64::max);
        if latest == f64::MIN {
            self.mouse.ev.push_back((now, dx as f64, dy as f64));
            return;
        }
        for (i, p) in self.pointed.iter_mut().enumerate().filter(|(_, p)| p.on && p.since == latest) {
            let (x, y) = (p.x + dx as f32 / POINT, p.y - dy as f32 / POINT);
            let len = x.hypot(y).max(1.0);
            (p.x, p.y) = if i == 2 { (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)) } else { (x / len, y / len) };
        }
    }

    pub fn tick(&mut self, now: f64, dt: f64) -> Pad {
        let map = &self.maps[self.mode as usize];
        let mut t = 0u32;
        for (c, b) in map.iter().enumerate() {
            let Some(b) = b else { continue };
            let on = match c as u16 {
                WHEEL_UP => now < self.pulse[0],
                WHEEL_DOWN => now < self.pulse[1],
                _ if b.toggle => self.latched[c],
                _ => self.held[c],
            };
            if on {
                t |= b.mask();
            }
        }
        if now - self.combo_t < 0.05 {
            // both stick clicks: let go first (Shift may hold L3), then press them together
            t &= !(Target::L3.bit() | Target::R3.bit());
        }
        let on = |x: Target| (t & x.bit() != 0) as i32 as f32;
        for (p, target) in self.pointed.iter_mut().zip([Target::LsHold, Target::RsHold, Target::Triggers]) {
            let held = t & target.bit() != 0;
            if held && !p.on {
                *p = Pointed { on: true, since: now, ..Pointed::default() };
            } else if !held && p.on {
                p.on = false;
                p.until = now + LINGER;
            }
        }
        let [left, right, triggers] = self.pointed.map(|p| (p.on || now < p.until).then_some((p.x, p.y)));
        let keys_left = t & (Target::LsUp.bit() | Target::LsDown.bit() | Target::LsLeft.bit() | Target::LsRight.bit()) != 0;
        let (mut lx, mut ly) = (on(Target::LsRight) - on(Target::LsLeft), on(Target::LsUp) - on(Target::LsDown));
        let m = lx.hypot(ly);
        if m > 0.0 {
            lx /= m;
            ly /= m;
        }
        if self.mode == Mode::Drive {
            // a key is all or nothing: ease the wheel over so A/D steer like a thumb
            let turning_in = lx != 0.0 && (lx.abs() > self.steer.abs() || lx * self.steer < 0.0);
            let rate = if turning_in { 5.0 } else { 9.0 } * dt as f32;
            self.steer += (lx - self.steer).clamp(-rate, rate);
            if lx != 0.0 && self.steer.abs() < 0.3 {
                self.steer = 0.3f32.copysign(lx); // skip the game's dead middle
            }
            lx = self.steer;
        } else {
            self.steer = 0.0;
        }
        if let (Some(p), false) = (left, keys_left) {
            (lx, ly) = p;
        }
        let (mut rx, mut ry) = self.mouse.step(now, dt, &self.aim);
        if !self.aim.invert_y {
            ry = -ry;
        }
        if let Some(p) = right {
            (rx, ry) = p;
        }
        // keys on the right stick win over the mouse
        let (kx, ky) = (on(Target::RsRight) - on(Target::RsLeft), on(Target::RsUp) - on(Target::RsDown));
        let m = kx.hypot(ky);
        if m > 0.0 {
            (rx, ry) = (kx / m, ky / m);
        }
        let (tl, tr) = triggers.filter(|_| self.pointed[2].on).unwrap_or_default();
        use Target::*;
        let sticks = [LsUp, LsDown, LsLeft, LsRight, RsUp, RsDown, RsLeft, RsRight, LsHold, RsHold, Triggers, Switch];
        Pad {
            buttons: t & !sticks.iter().fold(0, |m, s| m | s.bit()),
            lx,
            ly,
            rx,
            ry,
            lt: on(L2).max(tl),
            rt: on(R2).max(tr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Target::*;

    const W: u16 = 0x57;
    const A: u16 = 0x41;
    const D: u16 = 0x44;
    const F: u16 = 0x46;
    const V: u16 = 0x56;
    const U: u16 = 0x55;
    const SHIFT: u16 = 0xA0;
    const LMB: u16 = 1;
    const RMB: u16 = 2;
    const MMB: u16 = 4;

    fn core() -> Core {
        Core::new(&Config::default())
    }

    #[test]
    fn keys_drive_buttons_and_left_stick() {
        let mut c = core();
        assert!(c.press(W, 0.0));
        assert!(c.press(LMB, 0.0));
        let p = c.tick(0.0, 0.001);
        assert_eq!((p.lx, p.ly), (0.0, 1.0));
        assert!(p.has(R2) && p.rt == 1.0 && p.lt == 0.0);
        assert!(!p.has(LsUp), "stick directions are not buttons");
        c.press(A, 0.0);
        let p = c.tick(0.0, 0.001);
        assert!((p.lx.hypot(p.ly) - 1.0).abs() < 1e-5 && p.lx < 0.0, "diagonal stays on the unit circle");
        assert!(c.release(W) && c.release(A) && c.release(LMB));
        assert_eq!(c.tick(0.0, 0.001), Pad::default());
    }

    #[test]
    fn only_swallows_what_it_owns() {
        let mut c = core();
        assert!(!c.press(U, 0.0), "unbound key goes to the app");
        assert!(!c.release(W), "an up without its down is not swallowed");
        assert!(c.press(W, 0.0) && c.press(W, 0.0), "key repeat is swallowed");
        c.reset();
        assert!(!c.release(W), "after a reset the up passes through");
        assert_eq!(c.tick(0.0, 0.001), Pad::default());
    }

    #[test]
    fn unbound_mouse_buttons_are_swallowed() {
        let mut cfg = Config::default();
        cfg.foot.remove("MMB");
        let mut c = Core::new(&cfg);
        assert!(c.press(MMB, 0.0) && c.release(MMB));
        assert_eq!(c.tick(0.0, 0.001).buttons, 0);
    }

    #[test]
    fn f_switches_layout_and_keeps_triangle_held() {
        let mut c = core();
        c.press(F, 0.0);
        assert_eq!(c.mode, Mode::Drive);
        assert!(c.tick(0.0, 0.001).has(Triangle));
        c.press(F, 0.0); // repeat must not flip back
        assert_eq!(c.mode, Mode::Drive);
        c.release(F);
        c.press(W, 0.0);
        let p = c.tick(0.0, 0.001);
        assert!(p.has(R2) && p.ly == 0.0, "W is gas while driving");
        c.release(W);
        c.press(F, 1.0);
        assert_eq!(c.mode, Mode::Foot);
    }

    #[test]
    fn toggle_latches_until_pressed_again() {
        let mut cfg = Config::default();
        cfg.foot.get_mut("RMB").unwrap().toggle = true;
        let mut c = Core::new(&cfg);
        c.press(RMB, 0.0);
        c.release(RMB);
        assert!(c.tick(0.0, 0.001).has(L2), "stays on after release");
        c.press(RMB, 0.5);
        c.release(RMB);
        assert!(!c.tick(0.5, 0.001).has(L2));
        c.press(RMB, 1.0);
        c.release(RMB);
        c.press(F, 1.0);
        assert!(!c.tick(1.0, 0.001).has(L2), "switching layout drops latched keys");
    }

    #[test]
    fn both_stick_clicks_go_down_together() {
        let mut c = core();
        c.press(SHIFT, 0.0);
        assert!(c.tick(0.0, 0.001).has(L3));
        c.press(V, 1.0);
        let p = c.tick(1.01, 0.001);
        assert!(!p.has(L3) && !p.has(R3), "both up for a moment");
        let p = c.tick(1.06, 0.001);
        assert!(p.has(L3) && p.has(R3));
    }

    #[test]
    fn keys_on_the_right_stick_beat_the_mouse() {
        let mut cfg = Config::default();
        cfg.foot.insert("U".into(), Bind { to: vec![LsDown, RsDown], toggle: false });
        let mut c = Core::new(&cfg);
        c.mouse_move(0.0, 200, 0);
        c.press(U, 0.0);
        let p = c.tick(0.001, 0.001);
        assert_eq!((p.lx, p.ly, p.rx, p.ry), (0.0, -1.0, 0.0, -1.0), "one key pulls both sticks down");
        assert_eq!(p.buttons, 0);
    }

    const TAB: u16 = 0x09;
    const B: u16 = 0x42;
    const N: u16 = 0x4E;
    const T: u16 = 0x54;

    #[test]
    fn the_mouse_points_a_stick_and_it_stays() {
        let mut c = core();
        c.press(TAB, 0.0);
        let p = c.tick(0.0, 0.001);
        assert!(p.has(L1) && (p.rx, p.ry) == (0.0, 0.0), "loadout opens, stick at rest");
        c.mouse_move(0.001, 300, 0);
        let p = c.tick(0.002, 0.001);
        assert_eq!((p.rx, p.ry), (1.0, 0.0), "a flick right points the stick right");
        for i in 0..500 {
            assert_eq!(c.tick(0.003 + i as f64 * 0.001, 0.001).rx, 1.0, "and it stays there with the mouse still");
        }
        c.mouse_move(0.6, 0, -1000);
        let p = c.tick(0.601, 0.001);
        assert!(p.ry > 0.98 && p.rx.abs() < 0.2, "moving on slides it round the rim: ({}, {})", p.rx, p.ry);
        c.release(TAB);
        let p = c.tick(0.602, 0.001);
        assert!(!p.has(L1) && p.ry > 0.98, "the stick outlasts the button for a moment, so the pick lands");
        let p = c.tick(0.7, 0.001);
        assert_eq!((p.rx, p.ry), (0.0, 0.0), "the camera did not collect the mouse meanwhile");
    }

    #[test]
    fn both_sticks_can_be_set_one_after_the_other() {
        let mut c = core();
        c.press(B, 0.0);
        c.tick(0.0, 0.001);
        c.mouse_move(0.001, 300, 0);
        c.press(N, 0.01);
        c.tick(0.01, 0.001);
        c.mouse_move(0.011, -300, 0);
        let p = c.tick(0.012, 0.001);
        assert_eq!((p.lx, p.rx), (1.0, -1.0), "hotwire: left stick right, right stick left, both held");
        c.press(W, 0.02);
        assert_eq!(c.tick(0.02, 0.001).ly, 1.0, "a movement key still wins on its stick");
    }

    #[test]
    fn the_mouse_sets_trigger_pressure() {
        let mut c = core();
        c.press(T, 0.0);
        c.tick(0.0, 0.001);
        c.mouse_move(0.001, 70, -35);
        let p = c.tick(0.002, 0.001);
        assert!((p.lt - 0.5).abs() < 0.01 && (p.rt - 0.25).abs() < 0.01, "sideways is L2, up is R2: {} {}", p.lt, p.rt);
        c.mouse_move(0.003, -1000, 1000);
        assert_eq!(c.tick(0.004, 0.001).lt, 0.0);
        c.release(T);
        c.press(LMB, 0.01);
        assert_eq!(c.tick(0.01, 0.001).rt, 1.0, "a plain click is still a full pull");
    }

    #[test]
    fn a_double_tap_switches_layout_once() {
        let mut c = core();
        c.flip();
        c.press(F, 5.0);
        c.release(F);
        c.press(F, 5.2);
        assert_eq!(c.mode, Mode::Foot, "bailing out of a car leaves you on foot");
        assert!(c.tick(5.2, 0.001).has(Triangle));
        c.release(F);
        c.press(0x14, 5.3); // Caps Lock
        assert_eq!(c.mode, Mode::Drive, "another key can still correct it at once");
        c.press(F, 6.0);
        assert_eq!(c.mode, Mode::Foot, "and F works again after the double-tap window");
    }

    #[test]
    fn wheel_pulses_its_button() {
        let mut cfg = Config::default();
        cfg.foot.insert("WheelUp".into(), Bind { to: vec![L1], toggle: false });
        let mut c = Core::new(&cfg);
        assert!(c.wheel(true, 1.0));
        assert!(!c.wheel(false, 1.0), "unbound wheel direction goes to the app");
        assert!(c.tick(1.03, 0.001).has(L1));
        assert!(!c.tick(1.08, 0.001).has(L1));
    }

    #[test]
    fn steering_eases_in_and_skips_the_dead_middle() {
        let mut c = core();
        c.flip();
        c.press(D, 0.0);
        let first = c.tick(0.0, 0.001).lx;
        assert!((first - 0.3).abs() < 1e-6);
        let mut lx = first;
        for i in 1..400 {
            lx = c.tick(i as f64 * 0.001, 0.001).lx;
        }
        assert_eq!(lx, 1.0);
        c.release(D);
        for i in 400..700 {
            lx = c.tick(i as f64 * 0.001, 0.001).lx;
        }
        assert_eq!(lx, 0.0);
    }

    /// Feeds a 1000 Hz mouse moving `cps` counts/s for `secs`, returns the stick at the end.
    fn swipe(c: &mut Core, cps: f64, secs: f64, t0: f64) -> Pad {
        let mut p = Pad::default();
        let mut acc = 0.0;
        for i in 0..(secs * 1000.0) as usize {
            let now = t0 + i as f64 * 0.001;
            acc += cps * 0.001;
            let n = acc.trunc();
            acc -= n;
            if n != 0.0 {
                c.mouse_move(now, n as i32, 0);
            }
            p = c.tick(now, 0.001);
        }
        p
    }

    #[test]
    fn mouse_speed_maps_to_stick_tilt() {
        let mut c = core();
        let full = swipe(&mut c, 6000.0, 0.3, 0.0);
        assert!(full.rx > 0.98 && full.ry == 0.0, "full speed = full tilt, got {}", full.rx);
        let mut c = core();
        let half = swipe(&mut c, 3000.0, 0.3, 0.0);
        let want = 0.34 + 0.66 * 0.5;
        assert!((half.rx - want).abs() < 0.03, "half speed: want {want}, got {}", half.rx);
        let mut c = core();
        let slow = swipe(&mut c, 100.0, 0.3, 0.0);
        assert_eq!(slow.rx, 0.0, "under the start gate nothing moves");
    }

    #[test]
    fn camera_stops_when_the_mouse_does() {
        let mut c = core();
        swipe(&mut c, -6000.0, 0.2, 0.0);
        let mut p = Pad::default();
        for i in 0..100 {
            p = c.tick(0.2 + i as f64 * 0.001, 0.001);
        }
        assert_eq!((p.rx, p.ry), (0.0, 0.0), "stick is back at rest 100 ms after the last count");
    }

    #[test]
    fn speed_slider_and_invert() {
        let mut cfg = Config::default();
        cfg.aim.speed = 2.0;
        let mut c = Core::new(&cfg);
        assert!(swipe(&mut c, 3000.0, 0.3, 0.0).rx > 0.98, "speed 2 reaches full tilt at half the hand speed");

        let mut c = core();
        c.mouse_move(0.0, 0, 200);
        assert!(c.tick(0.001, 0.02).ry < 0.0, "mouse down = stick down");
        cfg.aim.invert_y = true;
        let mut c = Core::new(&cfg);
        c.mouse_move(0.0, 0, 200);
        assert!(c.tick(0.001, 0.02).ry > 0.0);
    }

    #[test]
    fn config_round_trips_and_drops_junk() {
        let dir = std::env::temp_dir().join(format!("padwire-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let mut cfg = Config::default();
        cfg.foot.get_mut("RMB").unwrap().toggle = true;
        cfg.aim.speed = 1.7;
        cfg.save(&path);
        assert_eq!(Config::load(&path), cfg);
        std::fs::write(&path, r#"{"foot":{"NoSuchKey":{"to":["cross"]},"W":{"to":["ls_up"]}},"aim":{"speed":99}}"#).unwrap();
        let c = Config::load(&path);
        assert_eq!(c.foot.len(), 1);
        assert_eq!(c.aim.speed, 4.0);
        assert_eq!(c.drive, Config::default().drive, "missing parts fall back to defaults");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Config::load(&path), Config::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
