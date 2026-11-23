//! Windows side: hooks, raw mouse input, the foreground check and the 1 kHz output loop.
//! Hooks exist only while armed. While off, the arm key is polled instead.
use crate::core::{Core, Pad};
use crate::model::{ARM_VK, Console, Mode};
use crate::pad::Out;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::w;

#[derive(Default, Clone)]
pub struct Status {
    pub pad: Pad,
    pub error: Option<String>,
    /// Exe name of the window in front.
    pub front: String,
}

pub struct Shared {
    pub core: Mutex<Core>,
    /// Apps to act in, and the controller to be.
    pub want: Mutex<(Vec<String>, Console)>,
    pub armed: AtomicBool,
    pub live: AtomicBool,
    pub quit: AtomicBool,
    /// Self-test: act in any window, handle injected input instead of the real keys and mouse,
    /// leave the cursor alone.
    pub test: AtomicBool,
    pub status: Mutex<Status>,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
static ARM_HELD: AtomicBool = AtomicBool::new(false);
static ARM_OURS: AtomicBool = AtomicBool::new(false);

impl Shared {
    pub fn new(core: Core, apps: Vec<String>, console: Console) -> Arc<Shared> {
        let sh = Arc::new(Shared {
            core: Mutex::new(core),
            want: Mutex::new((apps, console)),
            armed: AtomicBool::new(false),
            live: AtomicBool::new(false),
            quit: AtomicBool::new(false),
            test: AtomicBool::new(false),
            status: Mutex::new(Status::default()),
        });
        let _ = SHARED.set(sh.clone());
        sh
    }

    pub fn mode(&self) -> Mode {
        self.core.lock().unwrap().mode
    }
}

pub fn now() -> f64 {
    static T0: OnceLock<Instant> = OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn exe_of(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n);
        CloseHandle(h);
        if ok == 0 {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        path.rsplit('\\').next().unwrap_or("").to_lowercase()
    }
}

/// Exe names of the apps with a visible window, for the "add app" list.
pub fn open_apps() -> Vec<String> {
    unsafe extern "system" fn each(hwnd: HWND, out: LPARAM) -> i32 {
        unsafe {
            if IsWindowVisible(hwnd) != 0 && GetWindowTextLengthW(hwnd) > 0 {
                let exe = exe_of(hwnd);
                let v = &mut *(out as *mut Vec<String>);
                if !exe.is_empty() && !v.contains(&exe) {
                    v.push(exe);
                }
            }
            1
        }
    }
    let mut v: Vec<String> = Vec::new();
    unsafe { EnumWindows(Some(each), &mut v as *mut _ as LPARAM) };
    v.sort();
    v
}

/// Where to pin the cursor so it can't be seen: on a screen edge with no monitor beyond it the
/// arrow hangs off the screen. Falls back to the window's middle.
fn hide_point(hwnd: HWND) -> POINT {
    unsafe {
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi) != 0 {
            let m = mi.rcMonitor;
            let (mx, my) = ((m.left + m.right) / 2, (m.top + m.bottom) / 2);
            for (x, y, ox, oy) in [(mx, m.bottom - 1, 0, 8), (m.right - 1, my, 8, 0)] {
                if MonitorFromPoint(POINT { x: x + ox, y: y + oy }, MONITOR_DEFAULTTONULL).is_null() {
                    return POINT { x, y };
                }
            }
        }
        let mut r: RECT = zeroed();
        GetWindowRect(hwnd, &mut r);
        POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
    }
}

// ---------------------------------------------------------------- hook thread

fn on_key(sh: &Shared, vk: u16, down: bool, alt: bool) -> bool {
    if vk == ARM_VK {
        if !down {
            ARM_HELD.store(false, Relaxed);
            return ARM_OURS.swap(false, Relaxed);
        }
        if !ARM_HELD.swap(true, Relaxed) {
            sh.armed.store(false, Relaxed);
            ARM_OURS.store(true, Relaxed);
        }
        return ARM_OURS.load(Relaxed);
    }
    if alt && down {
        return false; // keep Alt+Tab and Alt+F4 working
    }
    let mut core = sh.core.lock().unwrap();
    if down { sh.live.load(Relaxed) && core.press(vk, now()) } else { core.release(vk) }
}

unsafe extern "system" fn kb_proc(n: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if n == HC_ACTION as i32
            && let Some(sh) = SHARED.get()
        {
            let ev = &*(l as *const KBDLLHOOKSTRUCT);
            let test = sh.test.load(Relaxed);
            if (ev.flags & LLKHF_INJECTED != 0) == test {
                let down = w as u32 == WM_KEYDOWN || w as u32 == WM_SYSKEYDOWN;
                if on_key(sh, ev.vkCode as u16, down, ev.flags & LLKHF_ALTDOWN != 0) {
                    return 1;
                }
            }
        }
        CallNextHookEx(null_mut(), n, w, l)
    }
}

unsafe extern "system" fn ms_proc(n: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if n == HC_ACTION as i32
            && w as u32 != WM_MOUSEMOVE
            && let Some(sh) = SHARED.get()
        {
            let ev = &*(l as *const MSLLHOOKSTRUCT);
            let test = sh.test.load(Relaxed);
            if (ev.flags & LLMHF_INJECTED != 0) == test {
                let hi = (ev.mouseData >> 16) as u16;
                let x = if hi == 1 { 5 } else { 6 };
                let button = match w as u32 {
                    WM_LBUTTONDOWN => Some((1, true)),
                    WM_LBUTTONUP => Some((1, false)),
                    WM_RBUTTONDOWN => Some((2, true)),
                    WM_RBUTTONUP => Some((2, false)),
                    WM_MBUTTONDOWN => Some((4, true)),
                    WM_MBUTTONUP => Some((4, false)),
                    WM_XBUTTONDOWN => Some((x, true)),
                    WM_XBUTTONUP => Some((x, false)),
                    _ => None,
                };
                let live = sh.live.load(Relaxed);
                let mut core = sh.core.lock().unwrap();
                let swallow = match button {
                    Some((code, true)) => live && core.press(code, now()),
                    Some((code, false)) => core.release(code),
                    None if w as u32 == WM_MOUSEWHEEL => live && core.wheel((hi as i16) > 0, now()),
                    None => false,
                };
                drop(core);
                if swallow {
                    return 1;
                }
            }
        }
        CallNextHookEx(null_mut(), n, w, l)
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_INPUT
            && let Some(sh) = SHARED.get()
        {
            let mut ri: RAWINPUT = zeroed();
            let mut size = size_of::<RAWINPUT>() as u32;
            let got = GetRawInputData(
                l as HRAWINPUT,
                RID_INPUT,
                &mut ri as *mut _ as *mut c_void,
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            );
            let ours = !sh.test.load(Relaxed) || ri.header.hDevice.is_null();
            if got != u32::MAX && ri.header.dwType == RIM_TYPEMOUSE && ours {
                let m = ri.data.mouse;
                let relative = m.usFlags & MOUSE_MOVE_ABSOLUTE == 0;
                if relative && (m.lLastX != 0 || m.lLastY != 0) && sh.live.load(Relaxed) {
                    sh.core.lock().unwrap().mouse_move(now(), m.lLastX, m.lLastY);
                }
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}

fn hook_thread(ready: mpsc::Sender<u32>) {
    unsafe {
        let hinst = GetModuleHandleW(null());
        let class = w!("padwire-raw");
        let mut wc: WNDCLASSEXW = zeroed();
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinst;
        wc.lpszClassName = class;
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(0, class, class, 0, 0, 0, 0, 0, HWND_MESSAGE, null_mut(), hinst, null());
        let mut rid = RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
        RegisterRawInputDevices(&rid, 1, size_of::<RAWINPUTDEVICE>() as u32);
        // the arm key may still be down from switching on: its repeats must not switch off again
        ARM_HELD.store(GetAsyncKeyState(ARM_VK as i32) < 0, Relaxed);
        ARM_OURS.store(false, Relaxed);
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), hinst, 0);
        let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(ms_proc), hinst, 0);
        let _ = ready.send(GetCurrentThreadId());
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
        UnhookWindowsHookEx(kb);
        UnhookWindowsHookEx(ms);
        rid.dwFlags = RIDEV_REMOVE;
        rid.hwndTarget = null_mut();
        RegisterRawInputDevices(&rid, 1, size_of::<RAWINPUTDEVICE>() as u32);
        DestroyWindow(hwnd);
        UnregisterClassW(class, hinst);
    }
}

struct Hooks {
    thread_id: u32,
    handle: std::thread::JoinHandle<()>,
}

impl Hooks {
    fn start() -> Hooks {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || hook_thread(tx));
        Hooks { thread_id: rx.recv().unwrap_or(0), handle }
    }

    fn stop(self) {
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        let _ = self.handle.join();
    }
}

// ---------------------------------------------------------------- output loop

/// Whatever ends the loop, even a panic: stop acting and give the cursor back.
struct Release(Arc<Shared>);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.armed.store(false, Relaxed);
        self.0.live.store(false, Relaxed);
        unsafe { ClipCursor(null()) };
    }
}

pub fn run(sh: Arc<Shared>) {
    let _release = Release(sh.clone());
    unsafe { timeBeginPeriod(1) };
    let mut out: Option<Out> = None;
    let mut sent: Option<Pad> = None;
    let mut hooks: Option<Hooks> = None;
    let (mut next_check, mut next_connect, mut hooks_off_at) = (0.0, 0.0, -1.0);
    let (mut clipped, mut arm_was_down, mut live) = (false, false, false);
    let mut last = now();
    while !sh.quit.load(Relaxed) {
        let t = now();
        if t >= next_check {
            next_check = t + 0.05;
            let (apps, console) = sh.want.lock().unwrap().clone();
            let test = sh.test.load(Relaxed);
            if out.as_ref().map(Out::console) != Some(console) && t >= next_connect {
                drop(out.take()); // unplug the old pad before plugging the new one
                sent = None;
                let r = Out::connect(console);
                if r.is_err() {
                    next_connect = t + 2.0;
                }
                sh.status.lock().unwrap().error = r.as_ref().err().cloned();
                out = r.ok();
            }
            let hwnd = unsafe { GetForegroundWindow() };
            let exe = exe_of(hwnd);
            let front = test || apps.contains(&exe);
            let down = unsafe { GetAsyncKeyState(ARM_VK as i32) } < 0;
            if out.is_none() {
                sh.armed.store(false, Relaxed); // no pad, nothing to drive
            } else if down && !arm_was_down && front && hooks.is_none() && t - hooks_off_at > 0.4 {
                sh.armed.store(true, Relaxed);
            }
            arm_was_down = down;
            let armed = sh.armed.load(Relaxed);
            if armed && hooks.is_none() {
                hooks = Some(Hooks::start());
            } else if !armed && let Some(h) = hooks.take() {
                h.stop();
                hooks_off_at = t;
            }
            live = armed && front && out.is_some();
            if live != sh.live.swap(live, Relaxed) && !live {
                sh.core.lock().unwrap().reset();
            }
            if live && !test {
                let p = hide_point(hwnd);
                unsafe { ClipCursor(&RECT { left: p.x, top: p.y, right: p.x + 1, bottom: p.y + 1 }) };
                clipped = true;
            } else if clipped {
                unsafe { ClipCursor(null()) };
                clipped = false;
            }
            sh.status.lock().unwrap().front = exe;
        }
        let dt = (t - last).max(1e-4);
        last = t;
        let pad = if live { sh.core.lock().unwrap().tick(t, dt) } else { Pad::default() };
        if sent != Some(pad) {
            sent = Some(pad);
            if let Some(o) = &mut out
                && let Err(e) = o.send(&pad)
            {
                out = None;
                next_connect = t + 2.0;
                sh.status.lock().unwrap().error = Some(e);
            }
            sh.status.lock().unwrap().pad = pad;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if let Some(h) = hooks {
        h.stop();
    }
    drop(out);
    unsafe { timeEndPeriod(1) };
}
