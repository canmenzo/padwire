use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const WHEEL_UP: u16 = 0x100;
pub const WHEEL_DOWN: u16 = 0x101;
pub const CODES: usize = 0x102;
pub const ARM_VK: u16 = 0x79; // F10
/// Most keys one controller button can have.
pub const MAX_KEYS: usize = 2;

/// (code, config name, key cap). Keyboard codes are Windows virtual keys.
pub const KEYS: &[(u16, &str, &str)] = &[
    (0x1B, "Esc", "Esc"), (0x70, "F1", "F1"), (0x71, "F2", "F2"), (0x72, "F3", "F3"), (0x73, "F4", "F4"),
    (0x74, "F5", "F5"), (0x75, "F6", "F6"), (0x76, "F7", "F7"), (0x77, "F8", "F8"), (0x78, "F9", "F9"),
    (0x7A, "F11", "F11"), (0x7B, "F12", "F12"), (0x87, "F24", "F24"),
    (0xC0, "Grave", "`"), (0x31, "1", "1"), (0x32, "2", "2"), (0x33, "3", "3"), (0x34, "4", "4"),
    (0x35, "5", "5"), (0x36, "6", "6"), (0x37, "7", "7"), (0x38, "8", "8"), (0x39, "9", "9"),
    (0x30, "0", "0"), (0xBD, "Minus", "-"), (0xBB, "Equals", "="), (0x08, "Backspace", "Bksp"),
    (0x09, "Tab", "Tab"), (0x51, "Q", "Q"), (0x57, "W", "W"), (0x45, "E", "E"), (0x52, "R", "R"),
    (0x54, "T", "T"), (0x59, "Y", "Y"), (0x55, "U", "U"), (0x49, "I", "I"), (0x4F, "O", "O"),
    (0x50, "P", "P"), (0xDB, "LBracket", "["), (0xDD, "RBracket", "]"), (0xDC, "Backslash", "\\"),
    (0x14, "CapsLock", "Caps"), (0x41, "A", "A"), (0x53, "S", "S"), (0x44, "D", "D"), (0x46, "F", "F"),
    (0x47, "G", "G"), (0x48, "H", "H"), (0x4A, "J", "J"), (0x4B, "K", "K"), (0x4C, "L", "L"),
    (0xBA, "Semicolon", ";"), (0xDE, "Quote", "'"), (0x0D, "Enter", "Enter"),
    (0xA0, "LShift", "Shift"), (0x5A, "Z", "Z"), (0x58, "X", "X"), (0x43, "C", "C"), (0x56, "V", "V"),
    (0x42, "B", "B"), (0x4E, "N", "N"), (0x4D, "M", "M"), (0xBC, "Comma", ","), (0xBE, "Period", "."),
    (0xBF, "Slash", "/"), (0xA1, "RShift", "Shift"), (0xA2, "LCtrl", "Ctrl"), (0x20, "Space", "Space"),
    (0xA3, "RCtrl", "Ctrl"), (0x26, "Up", "↑"), (0x25, "Left", "←"), (0x28, "Down", "↓"),
    (0x27, "Right", "→"),
    (0x01, "LMB", "Left"), (0x02, "RMB", "Right"), (0x04, "MMB", "Middle"), (0x05, "Mouse4", "Side 1"),
    (0x06, "Mouse5", "Side 2"), (WHEEL_UP, "WheelUp", "Wheel ↑"), (WHEEL_DOWN, "WheelDown", "Wheel ↓"),
];

pub fn key_code(name: &str) -> Option<u16> {
    KEYS.iter().find(|k| k.1 == name).map(|k| k.0)
}

pub fn key_name(code: u16) -> Option<&'static str> {
    KEYS.iter().find(|k| k.0 == code).map(|k| k.1)
}

/// Codes of the keys bound to a controller button, in keyboard order.
pub fn keys_on(l: &Layout, t: Target) -> Vec<u16> {
    KEYS.iter().filter(|k| l.get(k.1).is_some_and(|b| b.to.contains(&t))).map(|k| k.0).collect()
}

pub fn key_cap(code: u16) -> &'static str {
    KEYS.iter().find(|k| k.0 == code).map(|k| k.2).unwrap_or("")
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Cross, Circle, Square, Triangle, L1, R1, L2, R2, L3, R3, Options, Share, Touchpad, Ps,
    DUp, DDown, DLeft, DRight, LsUp, LsDown, LsLeft, LsRight, RsUp, RsDown, RsLeft, RsRight,
    /// While held, the mouse points the left stick and it stays where it is put.
    LsHold,
    /// The same for the right stick: weapon and radio wheels, lock tools, hotwiring.
    RsHold,
    /// While held, the mouse sets how far L2 (sideways) and R2 (up) are pressed.
    Triggers,
    /// Switches between the on-foot and driving layouts.
    Switch,
}
use Target::*;

pub const TARGETS: [Target; 30] = [
    Cross, Circle, Square, Triangle, L1, R1, L2, R2, L3, R3, DUp, DDown, DLeft, DRight,
    LsUp, LsDown, LsLeft, LsRight, RsUp, RsDown, RsLeft, RsRight, Options, Touchpad, Share, Ps,
    LsHold, RsHold, Triggers, Switch,
];

impl Target {
    pub const fn bit(self) -> u32 {
        1 << self as u32
    }

    /// A controller button or stick direction, which takes `MAX_KEYS` keys. The rest are padwire's own modes.
    pub fn is_button(self) -> bool {
        !matches!(self, LsHold | RsHold | Triggers | Switch)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Console {
    Ps,
    Xbox,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Foot,
    Drive,
}

impl Mode {
    pub fn other(self) -> Mode {
        if self == Mode::Foot { Mode::Drive } else { Mode::Foot }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Bind {
    pub to: Vec<Target>,
    /// Press once to hold, press again to let go.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub toggle: bool,
}

impl Bind {
    pub fn mask(&self) -> u32 {
        self.to.iter().fold(0, |m, t| m | t.bit())
    }
}

pub type Layout = BTreeMap<String, Bind>;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct Aim {
    /// Mouse speed multiplier.
    pub speed: f32,
    /// Stick tilt the smallest move starts at, to clear the game's dead zone.
    pub push: f32,
    /// 1 = linear. Lower makes small moves turn more.
    pub curve: f32,
    pub invert_y: bool,
}

impl Default for Aim {
    fn default() -> Self {
        Aim { speed: 1.0, push: 0.34, curve: 1.0, invert_y: false }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Config {
    pub console: Console,
    /// Exe names of the apps padwire acts in.
    pub apps: Vec<String>,
    pub foot: Layout,
    pub drive: Layout,
    pub aim: Aim,
}

fn layout(binds: &[(&str, &[Target])]) -> Layout {
    binds.iter().map(|(k, t)| (k.to_string(), Bind { to: t.to_vec(), toggle: false })).collect()
}

impl Default for Config {
    fn default() -> Self {
        // GTA 6's default PS5 scheme on GTA V's PC keys. B / N / T hand a stick or the triggers to the mouse.
        let foot = layout(&[
            ("W", &[LsUp]), ("A", &[LsLeft]), ("S", &[LsDown]), ("D", &[LsRight]),
            ("I", &[RsUp]), ("J", &[RsLeft]), ("K", &[RsDown]), ("L", &[RsRight]),
            ("Space", &[Cross]), ("LShift", &[L3]), ("LCtrl", &[Circle]),
            ("F", &[Triangle, Switch]), ("E", &[Triangle]), ("R", &[Square]),
            ("Q", &[R1]), ("Tab", &[L1, RsHold]), ("C", &[R3]), ("V", &[L3, R3]), ("G", &[DRight]),
            ("M", &[Touchpad]), ("P", &[Options]), ("Enter", &[Options]),
            ("Up", &[DUp]), ("Down", &[DDown]), ("Left", &[DLeft, RsHold]), ("Right", &[DRight]),
            ("Z", &[DDown]), ("CapsLock", &[Switch]), ("F1", &[Ps]), ("F2", &[Share]),
            ("B", &[LsHold]), ("N", &[RsHold]), ("T", &[Triggers]),
            ("LMB", &[R2]), ("RMB", &[L2]), ("MMB", &[DUp]), ("Mouse4", &[L1, RsHold]), ("Mouse5", &[R1]),
        ]);
        let drive = layout(&[
            ("W", &[R2]), ("S", &[L2]), ("A", &[LsLeft]), ("D", &[LsRight]),
            ("LShift", &[LsUp]), ("LCtrl", &[LsDown]),
            ("I", &[RsUp]), ("J", &[RsLeft]), ("K", &[RsDown]), ("L", &[RsRight]),
            ("Space", &[Cross]), ("F", &[Triangle, Switch]), ("E", &[L3]), ("H", &[Circle]), ("X", &[Circle]),
            ("R", &[Square]), ("C", &[R3]), ("Q", &[DLeft, RsHold]), ("Tab", &[L1, RsHold]),
            ("M", &[Touchpad]), ("P", &[Options]), ("Enter", &[Options]),
            ("Up", &[DUp]), ("Down", &[DDown]), ("Left", &[DLeft, RsHold]), ("Right", &[DRight]),
            ("Z", &[DDown]), ("CapsLock", &[Switch]), ("F1", &[Ps]), ("F2", &[Share]),
            ("B", &[LsHold]), ("N", &[RsHold]), ("T", &[Triggers]),
            ("LMB", &[R1]), ("RMB", &[L1]), ("MMB", &[DUp]), ("Mouse5", &[R1]),
        ]);
        Config {
            console: Console::Ps,
            apps: ["veyra_qml_ui.exe", "chiaki.exe", "chiaki-ng.exe", "remoteplay.exe", "xboxpcapp.exe"]
                .map(String::from)
                .to_vec(),
            foot,
            drive,
            aim: Aim::default(),
        }
    }
}

impl Config {
    pub fn layout(&self, m: Mode) -> &Layout {
        if m == Mode::Foot { &self.foot } else { &self.drive }
    }

    pub fn layout_mut(&mut self, m: Mode) -> &mut Layout {
        if m == Mode::Foot { &mut self.foot } else { &mut self.drive }
    }

    /// Puts a key (or nothing) in one of a button's key slots. The key's old binding is replaced.
    pub fn set_key(&mut self, m: Mode, t: Target, slot: usize, code: Option<u16>) {
        let old = keys_on(self.layout(m), t).get(slot).copied();
        let l = self.layout_mut(m);
        if let Some(name) = old.and_then(key_name) {
            let empty = l.get_mut(name).is_some_and(|b| {
                b.to.retain(|x| *x != t);
                b.to.is_empty()
            });
            if empty {
                l.remove(name);
            }
        }
        if let Some(name) = code.and_then(key_name) {
            l.insert(name.to_string(), Bind { to: vec![t], toggle: false });
        }
    }

    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
        base.join("padwire").join("config.json")
    }

    pub fn load(path: &std::path::Path) -> Config {
        let mut c: Config = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        c.clean();
        c
    }

    pub fn save(&self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    fn clean(&mut self) {
        for l in [&mut self.foot, &mut self.drive] {
            l.retain(|k, _| key_code(k).is_some());
            for t in TARGETS.into_iter().filter(|t| t.is_button()) {
                for code in keys_on(l, t).into_iter().skip(MAX_KEYS) {
                    if let Some(b) = key_name(code).and_then(|n| l.get_mut(n)) {
                        b.to.retain(|x| *x != t);
                    }
                }
            }
            l.retain(|_, b| !b.to.is_empty());
        }
        self.apps = self.apps.iter().map(|a| a.trim().to_lowercase()).filter(|a| !a.is_empty()).collect();
        self.aim.speed = self.aim.speed.clamp(0.2, 4.0);
        self.aim.push = self.aim.push.clamp(0.0, 0.6);
        self.aim.curve = self.aim.curve.clamp(0.4, 2.0);
    }
}

/// Button name on the chosen console.
pub fn button_name(t: Target, c: Console) -> &'static str {
    let ps = c == Console::Ps;
    match t {
        Cross => if ps { "✕" } else { "A" },
        Circle => if ps { "○" } else { "B" },
        Square => if ps { "⬜" } else { "X" },
        Triangle => if ps { "△" } else { "Y" },
        L1 => if ps { "L1" } else { "LB" },
        R1 => if ps { "R1" } else { "RB" },
        L2 => if ps { "L2" } else { "LT" },
        R2 => if ps { "R2" } else { "RT" },
        L3 => if ps { "L3" } else { "LS" },
        R3 => if ps { "R3" } else { "RS" },
        Options => if ps { "Options" } else { "Menu" },
        Share => if ps { "Create" } else { "View" },
        Touchpad => if ps { "Touchpad" } else { "View" },
        Ps => if ps { "PS" } else { "Xbox" },
        DUp => "D-pad ↑",
        DDown => "D-pad ↓",
        DLeft => "D-pad ←",
        DRight => "D-pad →",
        LsUp => "L stick ↑",
        LsDown => "L stick ↓",
        LsLeft => "L stick ←",
        LsRight => "L stick →",
        RsUp => "R stick ↑",
        RsDown => "R stick ↓",
        RsLeft => "R stick ←",
        RsRight => "R stick →",
        LsHold => "L stick by mouse",
        RsHold => "R stick by mouse",
        Triggers => "L2 R2 by mouse",
        Switch => "Switch layout",
    }
}

/// What the button does in GTA 6 (default scheme) in each layout.
pub fn action(t: Target, m: Mode) -> &'static str {
    let foot = m == Mode::Foot;
    match t {
        LsUp => if foot { "Forward" } else { "Lean fwd" },
        LsDown => if foot { "Back" } else { "Lean back" },
        LsLeft => if foot { "Left" } else { "Steer L" },
        LsRight => if foot { "Right" } else { "Steer R" },
        RsUp => "Look up",
        RsDown => "Look down",
        RsLeft => "Look left",
        RsRight => "Look right",
        Cross => if foot { "Jump" } else { "Handbrake" },
        Circle => if foot { "Crouch" } else { "Lights" },
        Square => "Melee",
        Triangle => if foot { "Interact" } else { "Get out" },
        L1 => "Weapons",
        R1 => if foot { "Cover" } else { "Shoot" },
        L2 => if foot { "Aim" } else { "Brake" },
        R2 => if foot { "Shoot" } else { "Gas" },
        L3 => if foot { "Run" } else { "Horn" },
        R3 => "Look back",
        Touchpad => "Map",
        Options => "Pause",
        DUp => "Phone",
        DDown => "Minimap",
        DLeft => if foot { "Radio" } else { "Blinker L" },
        DRight => if foot { "Detonate" } else { "Blinker R" },
        Ps => "Home",
        Share => "Capture",
        LsHold => "Point L",
        RsHold => "Point R",
        Triggers => "Triggers",
        Switch => "Switch",
    }
}

/// Short label for a key's binding in a layout.
pub fn describe(b: &Bind, m: Mode) -> &'static str {
    let main: Vec<Target> = b.to.iter().copied().filter(|t| t.is_button()).collect();
    match main.as_slice() {
        [] => b.to.first().map_or("", |t| action(*t, m)),
        m2 if m2.contains(&L3) && m2.contains(&R3) => "Ability",
        [Triangle] if b.to.contains(&Switch) => if m == Mode::Foot { "Get in" } else { "Get out" },
        [t, ..] => action(*t, m),
    }
}
