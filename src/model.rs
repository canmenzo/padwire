use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const WHEEL_UP: u16 = 0x100;
pub const WHEEL_DOWN: u16 = 0x101;
pub const CODES: usize = 0x102;
pub const ARM_VK: u16 = 0x79; // F10

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

pub fn key_cap(code: u16) -> &'static str {
    KEYS.iter().find(|k| k.0 == code).map(|k| k.2).unwrap_or("")
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Cross, Circle, Square, Triangle, L1, R1, L2, R2, L3, R3, Options, Share, Touchpad, Ps,
    DUp, DDown, DLeft, DRight, LsUp, LsDown, LsLeft, LsRight,
    /// Switches between the on-foot and driving layouts.
    Switch,
}
use Target::*;

pub const TARGETS: [Target; 23] = [
    Cross, Circle, Square, Triangle, L1, R1, L2, R2, L3, R3, DUp, DDown, DLeft, DRight,
    LsUp, LsDown, LsLeft, LsRight, Options, Touchpad, Share, Ps, Switch,
];

impl Target {
    pub const fn bit(self) -> u32 {
        1 << self as u32
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
        // GTA 6's default PS5 scheme on GTA V's PC keys (from Rewire).
        let foot = layout(&[
            ("W", &[LsUp]), ("A", &[LsLeft]), ("S", &[LsDown]), ("D", &[LsRight]),
            ("Space", &[Cross]), ("LShift", &[L3]), ("LCtrl", &[Circle]),
            ("F", &[Triangle, Switch]), ("E", &[Triangle]), ("R", &[Square]),
            ("Q", &[R1]), ("G", &[R1]), ("Tab", &[L1]), ("C", &[R3]), ("V", &[L3, R3]),
            ("M", &[Touchpad]), ("P", &[Options]), ("Enter", &[Options]),
            ("Up", &[DUp]), ("Down", &[DDown]), ("Left", &[DLeft]), ("Right", &[DRight]),
            ("Z", &[DDown]), ("CapsLock", &[Switch]), ("F1", &[Ps]), ("F2", &[Share]),
            ("LMB", &[R2]), ("RMB", &[L2]), ("MMB", &[R3]), ("Mouse4", &[L1]), ("Mouse5", &[R1]),
        ]);
        let drive = layout(&[
            ("W", &[R2]), ("S", &[L2]), ("A", &[LsLeft]), ("D", &[LsRight]),
            ("LCtrl", &[LsDown]), ("X", &[LsUp]), ("Space", &[Cross]),
            ("F", &[Triangle, Switch]), ("E", &[L3]), ("LShift", &[L3]), ("H", &[Circle]),
            ("C", &[R3]), ("V", &[L3, R3]), ("R", &[Square]), ("Q", &[DLeft]), ("Tab", &[L1]),
            ("M", &[Touchpad]), ("P", &[Options]), ("Enter", &[Options]),
            ("Up", &[DUp]), ("Down", &[DDown]), ("Left", &[DLeft]), ("Right", &[DRight]),
            ("Z", &[DDown]), ("CapsLock", &[Switch]), ("F1", &[Ps]), ("F2", &[Share]),
            ("LMB", &[R1]), ("RMB", &[L1]),
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
            l.retain(|k, b| key_code(k).is_some() && !b.to.is_empty());
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
        LsUp => "Stick ↑",
        LsDown => "Stick ↓",
        LsLeft => "Stick ←",
        LsRight => "Stick →",
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
        Switch => "Switch",
    }
}

/// Short label for a key's binding in a layout.
pub fn describe(b: &Bind, m: Mode) -> &'static str {
    let main: Vec<Target> = b.to.iter().copied().filter(|t| *t != Switch).collect();
    match main.as_slice() {
        [] => action(Switch, m),
        m2 if m2.contains(&L3) && m2.contains(&R3) => "Ability",
        [Triangle] if b.to.contains(&Switch) => if m == Mode::Foot { "Get in" } else { "Get out" },
        [t, ..] => action(*t, m),
    }
}
