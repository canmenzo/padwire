//! `padwire --selftest`: plugs in the real virtual pad, feeds it through the real hooks with
//! injected input, and reads the result back from Windows (XInput / the joystick API).
//! Your own keys and mouse are left alone; only the injected events are handled.
use crate::core::{Core, Pad};
use crate::input::{self, Shared};
use crate::model::{ARM_VK, Bind, Config, Console, Mode, TARGETS, Target::*};
use crate::pad::{Out, xbox_report};
use std::mem::{size_of, zeroed};
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};
use windows_sys::Win32::Media::Multimedia::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Input::XboxController::*;

struct T {
    failed: i32,
}

impl T {
    fn check(&mut self, ok: bool, what: &str) {
        println!("{} {what}", if ok { "ok  " } else { "FAIL" });
        self.failed += !ok as i32;
    }
}

fn wait(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Polls `f` for up to `ms`.
fn until(ms: u64, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        if f() {
            return true;
        }
        wait(2);
    }
    f()
}

fn xinput(slot: u32) -> Option<XINPUT_GAMEPAD> {
    unsafe {
        let mut s: XINPUT_STATE = zeroed();
        (XInputGetState(slot, &mut s) == 0).then_some(s.Gamepad)
    }
}

fn xinput_slots() -> Vec<u32> {
    (0..4).filter(|i| xinput(*i).is_some()).collect()
}

/// The virtual DualShock 4 as the Windows joystick API sees it.
fn ds4() -> Option<JOYINFOEX> {
    unsafe {
        for id in 0..joyGetNumDevs() {
            let mut caps: JOYCAPSW = zeroed();
            if joyGetDevCapsW(id as usize, &mut caps, size_of::<JOYCAPSW>() as u32) != 0 {
                continue;
            }
            if (caps.wMid, caps.wPid) != (0x054C, 0x05C4) {
                continue;
            }
            let mut j: JOYINFOEX = zeroed();
            j.dwSize = size_of::<JOYINFOEX>() as u32;
            j.dwFlags = 0xFF; // JOY_RETURNALL
            if joyGetPosEx(id, &mut j) == 0 {
                return Some(j);
            }
        }
        None
    }
}

fn key(vk: u16, down: bool) {
    unsafe {
        let mut i: INPUT = zeroed();
        i.r#type = INPUT_KEYBOARD;
        i.Anonymous.ki = KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if down { 0 } else { KEYEVENTF_KEYUP }, time: 0, dwExtraInfo: 0 };
        SendInput(1, &i, size_of::<INPUT>() as i32);
    }
}

fn mouse(dx: i32, flags: u32, data: i32) {
    unsafe {
        let mut i: INPUT = zeroed();
        i.r#type = INPUT_MOUSE;
        i.Anonymous.mi = MOUSEINPUT { dx, dy: 0, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 };
        SendInput(1, &i, size_of::<INPUT>() as i32);
    }
}

/// Every button, straight to the driver and back.
fn mapping(t: &mut T) {
    let before = xinput_slots();
    let mut out = match Out::connect(Console::Xbox) {
        Ok(o) => o,
        Err(e) => return t.check(false, &format!("plug in a virtual Xbox pad: {e}")),
    };
    let slot = out.xinput_index();
    t.check(slot.is_some_and(|s| !before.contains(&s)), "virtual Xbox pad shows up in XInput");
    let Some(slot) = slot else { return };
    let mut bad = Vec::new();
    for target in TARGETS.iter().filter(|x| !matches!(x, LsUp | LsDown | LsLeft | LsRight | RsUp | RsDown | RsLeft | RsRight | LsHold | RsHold | Triggers | Switch)) {
        let p = Pad { buttons: target.bit(), lt: (*target == L2) as u8 as f32, rt: (*target == R2) as u8 as f32, ..Pad::default() };
        let want = xbox_report(&p);
        // XInputGetState never reports the Guide button
        let buttons = want.buttons.raw & !0x0400;
        if !until(1000, || out.send(&p).is_ok() && xinput(slot).is_some_and(|g| (g.wButtons, g.bLeftTrigger, g.bRightTrigger) == (buttons, want.left_trigger, want.right_trigger))) {
            bad.push(*target);
        }
    }
    t.check(bad.is_empty(), &format!("Xbox: buttons and triggers read back {bad:?}"));
    let p = Pad { lx: -1.0, ly: 1.0, rx: 0.5, ry: -0.5, ..Pad::default() };
    let _ = out.send(&p);
    let w = xbox_report(&p);
    t.check(
        until(300, || xinput(slot).is_some_and(|g| (g.sThumbLX, g.sThumbLY, g.sThumbRX, g.sThumbRY) == (w.thumb_lx, w.thumb_ly, w.thumb_rx, w.thumb_ry))),
        "Xbox: both sticks read back",
    );
    drop(out);
    t.check(until(2000, || xinput(slot).is_none()), "Xbox pad unplugs");

    let mut out = match Out::connect(Console::Ps) {
        Ok(o) => o,
        Err(e) => return t.check(false, &format!("plug in a virtual DualShock 4: {e}")),
    };
    t.check(until(3000, || ds4().is_some()), "virtual DualShock 4 shows up as a controller");
    // joystick API numbering: square, cross, circle, triangle, L1, R1, L2, R2, share, options, L3, R3, PS, touchpad
    let order = [Square, Cross, Circle, Triangle, L1, R1, L2, R2, Share, Options, L3, R3, Ps, Touchpad];
    let mut bad = Vec::new();
    for (i, target) in order.iter().enumerate() {
        let _ = out.send(&Pad { buttons: target.bit(), ..Pad::default() });
        if !until(300, || ds4().is_some_and(|j| j.dwButtons == 1 << i)) {
            bad.push(*target);
        }
    }
    t.check(bad.is_empty(), &format!("DS4: all 14 buttons read back {bad:?}"));
    let mut bad = Vec::new();
    for (target, pov) in [(DUp, 0), (DRight, 9000), (DDown, 18000), (DLeft, 27000)] {
        let _ = out.send(&Pad { buttons: target.bit(), ..Pad::default() });
        if !until(300, || ds4().is_some_and(|j| j.dwPOV == pov)) {
            bad.push(target);
        }
    }
    t.check(bad.is_empty(), &format!("DS4: d-pad reads back {bad:?}"));
    let _ = out.send(&Pad { lx: -1.0, ly: 1.0, rx: 1.0, ry: -1.0, ..Pad::default() });
    // left stick = X/Y, right stick = Z/R; 0 = left / up
    t.check(
        until(300, || ds4().is_some_and(|j| j.dwXpos < 2000 && j.dwYpos < 2000 && j.dwZpos > 63000 && j.dwRpos > 63000)),
        "DS4: both sticks read back, up is up",
    );
    drop(out);
    t.check(until(3000, || ds4().is_none()), "DualShock 4 unplugs");
}

fn press_reads(read: &dyn Fn() -> bool, vk: u16) -> bool {
    key(vk, true);
    let down = until(500, read);
    key(vk, false);
    down && until(500, || !read())
}

/// How long a key press and a mouse move take to reach XInput, through the hooks, the engine and the driver.
fn latency(t: &mut T, slot: u32, vk: u16) {
    let pad = || xinput(slot).unwrap_or(unsafe { zeroed() });
    let a = || pad().wButtons & XINPUT_GAMEPAD_A != 0;
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    // busy-waits (no sleep) for `f` until `limit` ms after `from`, returning the time since `from`
    let spin = |from: Instant, limit: u64, f: &dyn Fn() -> bool| {
        while from.elapsed() < Duration::from_millis(limit) {
            if f() {
                return Some(from.elapsed());
            }
        }
        None
    };

    let mut lat = Vec::new();
    for i in 0..200u64 {
        // 0 to 2 ms lead-in so presses do not line up with the output loop's phase
        let lead = Duration::from_micros(i * 7919 % 2000);
        let s = Instant::now();
        while s.elapsed() < lead {}
        let t0 = Instant::now();
        key(vk, true);
        let got = spin(t0, 500, &a);
        key(vk, false);
        spin(Instant::now(), 500, &|| !a());
        lat.extend(got);
    }
    lat.sort();
    let worst = lat.last().copied().unwrap_or(Duration::MAX);
    if !lat.is_empty() {
        println!("     key latency: median {:.2} ms, worst {:.2} ms ({} of 200 presses read)", ms(lat[lat.len() / 2]), ms(worst), lat.len());
    }
    t.check(lat.len() == 200 && worst < Duration::from_millis(20), "key latency: every press reads within 20 ms");

    let t0 = Instant::now();
    let (mut next, mut last, mut rise) = (t0, t0, None);
    while t0.elapsed() < Duration::from_millis(100) {
        if Instant::now() >= next {
            last = Instant::now();
            mouse(6, MOUSEEVENTF_MOVE, 0);
            next += Duration::from_millis(1);
        }
        if rise.is_none() && pad().sThumbRX > 16384 {
            rise = Some(t0.elapsed());
        }
    }
    let rise = rise.or_else(|| spin(t0, 400, &|| pad().sThumbRX > 16384));
    let rest = spin(last, 500, &|| pad().sThumbRX == 0);
    let show = |d: Option<Duration>| d.map_or("never".to_string(), |d| format!("{:.2} ms", ms(d)));
    // Windows hands injected moves over in 8 ms lumps, so this is the 125 Hz path, not a fast mouse
    println!("     camera latency at 125 Hz: first move to RX > 16384 {}, last move to rest {}", show(rise), show(rest));
    t.check(rise.is_some() && rest.is_some(), "camera: steady mouse motion tilts the right stick and it settles at rest");
}

/// Keyboard and mouse through the hooks, the engine and the driver.
fn chain(t: &mut T) {
    const F24: u16 = 0x87;
    let mut cfg = Config { console: Console::Xbox, ..Config::default() };
    cfg.foot.insert("F24".into(), Bind { to: vec![Cross], toggle: false });
    cfg.foot.insert("WheelUp".into(), Bind { to: vec![Triangle], toggle: false });
    let before = xinput_slots();
    let sh = Shared::new(Core::new(&cfg), cfg.apps.clone(), cfg.console);
    sh.test.store(true, Relaxed);
    let engine = {
        let sh = sh.clone();
        std::thread::spawn(move || input::run(sh))
    };
    let mut slot = None;
    until(3000, || {
        slot = xinput_slots().into_iter().find(|s| !before.contains(s));
        slot.is_some()
    });
    t.check(slot.is_some(), "engine plugs in its pad");
    if let Some(slot) = slot {
        let pad = || xinput(slot).unwrap_or(unsafe { zeroed() });
        let a = || pad().wButtons & XINPUT_GAMEPAD_A != 0;

        key(F24, true);
        wait(150);
        let off = !a();
        key(F24, false);
        t.check(off, "off: keys do nothing");

        sh.armed.store(true, Relaxed);
        let live = until(1000, || sh.live.load(Relaxed));
        t.check(live, "turning on goes live");
        if live {
            t.check(press_reads(&a, F24), "keyboard hook: key down presses the button, key up lets go");
            latency(t, slot, F24);

            let rt = || pad().bRightTrigger == 255;
            mouse(0, MOUSEEVENTF_LEFTDOWN, 0);
            let down = until(500, rt);
            mouse(0, MOUSEEVENTF_LEFTUP, 0);
            t.check(down && until(500, || !rt()), "mouse hook: left click is R2");

            mouse(0, MOUSEEVENTF_WHEEL, 120);
            let y = || pad().wButtons & XINPUT_GAMEPAD_Y != 0;
            t.check(until(500, y) && until(500, || !y()), "mouse hook: a wheel notch taps its button");

            let (mut right, mut left) = (0i16, 0i16);
            for (dx, peak) in [(40, &mut right), (-40, &mut left)] {
                mouse(dx, MOUSEEVENTF_MOVE, 0);
                until(120, || {
                    let x = pad().sThumbRX;
                    if x.unsigned_abs() > peak.unsigned_abs() {
                        *peak = x;
                    }
                    false
                });
            }
            t.check(right > 8000 && left < -8000, &format!("raw mouse input: right then left tilts the right stick ({right}, {left})"));
            t.check(until(300, || pad().sThumbRX == 0), "the stick is back at rest when the mouse stops");

            key(0x46, true); // F: get in
            wait(60);
            key(0x46, false);
            t.check(sh.mode() == Mode::Drive, "F switches to the driving layout");
            sh.core.lock().unwrap().flip();

            key(F24, true);
            until(500, a);
            key(ARM_VK, true);
            key(ARM_VK, false);
            t.check(until(1000, || !sh.live.load(Relaxed) && !a()), "F10 turns it off and lets go of held buttons");
            key(F24, false);

            wait(500);
            key(ARM_VK, true);
            wait(120);
            key(ARM_VK, false);
            t.check(until(1000, || sh.live.load(Relaxed)), "F10 turns it on with no hooks installed");
            key(ARM_VK, true);
            key(ARM_VK, false);
            t.check(until(1000, || !sh.live.load(Relaxed)), "F10 turns it off again");

            *sh.want.lock().unwrap() = (cfg.apps.clone(), Console::Ps);
            t.check(until(5000, || xinput(slot).is_none() && ds4().is_some()), "switching console swaps the Xbox pad for a DualShock 4");
            sh.armed.store(true, Relaxed);
            if until(1000, || sh.live.load(Relaxed)) {
                let cross = || ds4().is_some_and(|j| j.dwButtons & 2 != 0);
                t.check(press_reads(&cross, F24), "DS4: a key press arrives as Cross");
            }
        }
    }
    sh.quit.store(true, Relaxed);
    let _ = engine.join();
    t.check(until(3000, || ds4().is_none() && xinput_slots() == before), "closing unplugs the pad");
}

pub fn run() -> i32 {
    if let Err(e) = Out::connect(Console::Xbox) {
        println!("FAIL {e}");
        return 2;
    }
    let mut t = T { failed: 0 };
    wait(500);
    mapping(&mut t);
    wait(500);
    chain(&mut t);
    println!("{}", if t.failed == 0 { "all passed".to_string() } else { format!("{} failed", t.failed) });
    t.failed
}
