//! The virtual controller (ViGEmBus).
use crate::core::Pad;
use crate::model::{Console, Target, Target::*};
use vigem_client::{Client, DS4Report, DualShock4Wired, TargetId, XButtons, XGamepad, Xbox360Wired};

pub enum Out {
    Xbox(Xbox360Wired<Client>),
    Ds4(DualShock4Wired<Client>),
}

fn bits(p: &Pad, map: &[(Target, u16)]) -> u16 {
    map.iter().filter(|(t, _)| p.has(*t)).fold(0, |m, (_, b)| m | b)
}

pub fn xbox_report(p: &Pad) -> XGamepad {
    let ax = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    XGamepad {
        buttons: XButtons(bits(p, &[
            (Cross, XButtons::A), (Circle, XButtons::B), (Square, XButtons::X), (Triangle, XButtons::Y),
            (L1, XButtons::LB), (R1, XButtons::RB), (L3, XButtons::LTHUMB), (R3, XButtons::RTHUMB),
            (Options, XButtons::START), (Share, XButtons::BACK), (Touchpad, XButtons::BACK),
            (Ps, XButtons::GUIDE), (DUp, XButtons::UP), (DDown, XButtons::DOWN),
            (DLeft, XButtons::LEFT), (DRight, XButtons::RIGHT),
        ])),
        left_trigger: (p.lt * 255.0) as u8,
        right_trigger: (p.rt * 255.0) as u8,
        thumb_lx: ax(p.lx),
        thumb_ly: ax(p.ly),
        thumb_rx: ax(p.rx),
        thumb_ry: ax(p.ry),
    }
}

pub fn ds4_report(p: &Pad) -> DS4Report {
    // DS4 sticks: 0..255, 128 = centre, y down
    let ax = |v: f32| (128.0 + v.clamp(-1.0, 1.0) * 127.0).round() as u8;
    let mut buttons = bits(p, &[
        (Square, 1 << 4), (Cross, 1 << 5), (Circle, 1 << 6), (Triangle, 1 << 7), (L1, 1 << 8),
        (R1, 1 << 9), (L2, 1 << 10), (R2, 1 << 11), (Share, 1 << 12), (Options, 1 << 13),
        (L3, 1 << 14), (R3, 1 << 15),
    ]);
    let x = p.has(DRight) as i8 - p.has(DLeft) as i8;
    let y = p.has(DUp) as i8 - p.has(DDown) as i8;
    buttons |= match (x, y) {
        (0, 1) => 0,
        (1, 1) => 1,
        (1, 0) => 2,
        (1, -1) => 3,
        (0, -1) => 4,
        (-1, -1) => 5,
        (-1, 0) => 6,
        (-1, 1) => 7,
        _ => 8,
    };
    DS4Report {
        thumb_lx: ax(p.lx),
        thumb_ly: ax(-p.ly),
        thumb_rx: ax(p.rx),
        thumb_ry: ax(-p.ry),
        buttons,
        special: bits(p, &[(Ps, 1), (Touchpad, 2)]) as u8,
        trigger_l: (p.lt * 255.0) as u8,
        trigger_r: (p.rt * 255.0) as u8,
    }
}

impl Out {
    pub fn connect(console: Console) -> Result<Out, String> {
        let err = |e: vigem_client::Error| match e {
            vigem_client::Error::BusNotFound => "ViGEmBus driver is not installed".to_string(),
            e => format!("ViGEmBus: {e:?}"),
        };
        let client = Client::connect().map_err(err)?;
        let mut out = match console {
            Console::Xbox => {
                let mut t = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
                t.plugin().map_err(err)?;
                t.wait_ready().map_err(err)?;
                Out::Xbox(t)
            }
            Console::Ps => {
                let mut t = DualShock4Wired::new(client, TargetId::DUALSHOCK4_WIRED);
                t.plugin().map_err(err)?;
                t.wait_ready().map_err(err)?;
                Out::Ds4(t)
            }
        };
        // a freshly plugged pad can refuse reports for a moment, even after wait_ready
        let mut r = out.send(&Pad::default());
        for _ in 0..150 {
            if r.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            r = out.send(&Pad::default());
        }
        r.map(|_| out)
    }

    pub fn console(&self) -> Console {
        match self {
            Out::Xbox(_) => Console::Xbox,
            Out::Ds4(_) => Console::Ps,
        }
    }

    pub fn send(&mut self, p: &Pad) -> Result<(), String> {
        match self {
            Out::Xbox(t) => t.update(&xbox_report(p)),
            Out::Ds4(t) => t.update(&ds4_report(p)),
        }
        .map_err(|e| format!("ViGEmBus: {e:?}"))
    }

    pub fn xinput_index(&mut self) -> Option<u32> {
        match self {
            Out::Xbox(t) => t.get_user_index().ok(),
            Out::Ds4(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(ts: &[Target]) -> Pad {
        Pad { buttons: ts.iter().fold(0, |m, t| m | t.bit()), ..Pad::default() }
    }

    #[test]
    fn ds4_report_layout() {
        let rest = ds4_report(&Pad::default());
        assert_eq!(rest, DS4Report::default());
        let r = ds4_report(&Pad { lx: 1.0, ly: 1.0, rx: -1.0, ry: -1.0, lt: 1.0, rt: 0.5, ..pad(&[Cross, DUp, DRight, Touchpad, L2]) });
        assert_eq!((r.thumb_lx, r.thumb_ly, r.thumb_rx, r.thumb_ry), (255, 1, 1, 255), "y is flipped: up = 0 side");
        assert_eq!(r.buttons, (1 << 5) | (1 << 10) | 1, "cross + L2 + d-pad north-east");
        assert_eq!((r.special, r.trigger_l, r.trigger_r), (2, 255, 127));
        assert_eq!(ds4_report(&pad(&[DLeft, DDown])).buttons & 0xF, 5);
        assert_eq!(ds4_report(&pad(&[DLeft, DRight])).buttons & 0xF, 8, "opposites cancel");
    }

    #[test]
    fn xbox_report_layout() {
        let r = xbox_report(&Pad { lx: -1.0, ly: 1.0, rt: 1.0, ..pad(&[Cross, Triangle, Touchpad, Ps, DUp]) });
        assert_eq!(r.buttons.raw, XButtons::A | XButtons::Y | XButtons::BACK | XButtons::GUIDE | XButtons::UP);
        assert_eq!((r.thumb_lx, r.thumb_ly, r.right_trigger, r.left_trigger), (-32767, 32767, 255, 0));
    }
}
