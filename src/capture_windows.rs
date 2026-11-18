//! Captura no Windows via hooks de baixo nível (WH_MOUSE_LL / WH_KEYBOARD_LL).
//! Capturado: cursor preso no centro do monitor; delta = pt - centro; eventos bloqueados.

use crate::config::Side;
use crate::keymap::{self, PAUSE_SC};
use crate::protocol::{Button, Msg};
use crate::server::{CapCmd, CapEvent, CaptureHandle, Desktop, Hotkey, KeyKind};
use crate::win_util::{self, has_monitor, monitor_rect, set_cursor};
use std::sync::Mutex;
use tokio::sync::mpsc;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Eventos seguidos na borda p/ ativar (evita troca acidental).
const EDGE_HITS: u32 = 2;

struct State {
    tx: mpsc::UnboundedSender<CapEvent>,
    side: Side,
    desktop: Desktop,
    captured: bool,
    center: POINT,
    entry_along: f32,
    edge_hits: u32,
    hotkey: Hotkey,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn start(side: Side) -> anyhow::Result<CaptureHandle> {
    win_util::dpi_aware();
    let desktop = win_util::virtual_screen();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<CapCmd>();
    {
        let mut st = STATE.lock().unwrap();
        anyhow::ensure!(st.is_none(), "captura já ativa");
        *st = Some(State {
            tx: ev_tx,
            side,
            desktop,
            captured: false,
            center: POINT::default(),
            entry_along: 0.0,
            edge_hits: 0,
            hotkey: Hotkey::default(),
        });
    }

    let (tid_tx, tid_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || hook_thread(tid_tx));
    let thread_id = match tid_rx.recv() {
        Ok(Ok(id)) => id,
        Ok(Err(e)) => {
            *STATE.lock().unwrap() = None;
            anyhow::bail!("falha ao instalar hooks: {e}");
        }
        Err(_) => anyhow::bail!("thread de hooks morreu"),
    };

    tokio::spawn(async move {
        let _guard = Guard { thread_id };
        while let Some(CapCmd::Release { along }) = cmd_rx.recv().await {
            if let Some(st) = STATE.lock().unwrap().as_mut() {
                if st.captured {
                    st.captured = false;
                    let (x, y) = st.desktop.return_cursor(st.side, along);
                    set_cursor(x as i32, y as i32);
                }
            }
        }
    });

    Ok(CaptureHandle { desktop, events: ev_rx, cmds: cmd_tx })
}

/// Ao parar o servidor: solta captura, remove hooks.
struct Guard {
    thread_id: u32,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(st) = STATE.lock().unwrap().take() {
            if st.captured {
                let (x, y) = st.desktop.return_cursor(st.side, st.entry_along);
                set_cursor(x as i32, y as i32);
            }
        }
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

fn hook_thread(tid_tx: std::sync::mpsc::Sender<Result<u32, String>>) {
    unsafe {
        let hmod = GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0));
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0);
        let kbd = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd_proc), hmod, 0);
        let (mouse, kbd) = match (mouse, kbd) {
            (Ok(m), Ok(k)) => (m, k),
            (m, k) => {
                let _ = tid_tx.send(Err(format!("{:?} {:?}", m.err(), k.err())));
                return;
            }
        };
        let _ = tid_tx.send(Ok(GetCurrentThreadId()));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(mouse);
        let _ = UnhookWindowsHookEx(kbd);
    }
}

fn at_edge(side: Side, p: POINT) -> bool {
    let r = monitor_rect(p);
    match side {
        Side::Top => p.y <= r.top && !has_monitor(p.x, r.top - 1),
        Side::Bottom => p.y >= r.bottom - 1 && !has_monitor(p.x, r.bottom),
        Side::Left => p.x <= r.left && !has_monitor(r.left - 1, p.y),
        Side::Right => p.x >= r.right - 1 && !has_monitor(r.right, p.y),
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if ms.flags & LLMHF_INJECTED == 0 && handle_mouse(wparam.0 as u32, ms) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Retorna true p/ bloquear o evento.
fn handle_mouse(msg: u32, ms: &MSLLHOOKSTRUCT) -> bool {
    let mut guard = STATE.lock().unwrap();
    let Some(st) = guard.as_mut() else { return false };
    if !st.captured {
        if msg != WM_MOUSEMOVE {
            return false;
        }
        if !at_edge(st.side, ms.pt) {
            st.edge_hits = 0;
            return false;
        }
        st.edge_hits += 1;
        if st.edge_hits < EDGE_HITS {
            return false;
        }
        st.edge_hits = 0;
        st.captured = true;
        st.hotkey.reset();
        st.entry_along = st.desktop.along_edge(st.side, ms.pt.x as f32, ms.pt.y as f32);
        let r = monitor_rect(ms.pt);
        st.center = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
        set_cursor(st.center.x, st.center.y);
        let _ = st.tx.send(CapEvent::Activated { along: st.entry_along });
        return true;
    }
    let wheel = (ms.mouseData >> 16) as u16 as i16 as i32;
    let xbtn = || if (ms.mouseData >> 16) == 1 { Button::Back } else { Button::Forward };
    let out = match msg {
        WM_MOUSEMOVE => {
            let (dx, dy) = (ms.pt.x - st.center.x, ms.pt.y - st.center.y);
            (dx != 0 || dy != 0).then_some(Msg::MouseMove { dx: dx as f32, dy: dy as f32 })
        }
        WM_LBUTTONDOWN => Some(Msg::MouseButton { button: Button::Left, down: true }),
        WM_LBUTTONUP => Some(Msg::MouseButton { button: Button::Left, down: false }),
        WM_RBUTTONDOWN => Some(Msg::MouseButton { button: Button::Right, down: true }),
        WM_RBUTTONUP => Some(Msg::MouseButton { button: Button::Right, down: false }),
        WM_MBUTTONDOWN => Some(Msg::MouseButton { button: Button::Middle, down: true }),
        WM_MBUTTONUP => Some(Msg::MouseButton { button: Button::Middle, down: false }),
        WM_XBUTTONDOWN => Some(Msg::MouseButton { button: xbtn(), down: true }),
        WM_XBUTTONUP => Some(Msg::MouseButton { button: xbtn(), down: false }),
        // Windows: positivo = p/ cima; protocolo: dy>0 = p/ baixo.
        WM_MOUSEWHEEL => Some(Msg::Scroll { dx: 0, dy: -wheel }),
        WM_MOUSEHWHEEL => Some(Msg::Scroll { dx: wheel, dy: 0 }),
        _ => None,
    };
    if let Some(m) = out {
        let _ = st.tx.send(CapEvent::Input(m));
    }
    true
}

unsafe extern "system" fn kbd_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if !kb.flags.contains(LLKHF_INJECTED) && handle_key(wparam.0 as u32, kb) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn handle_key(msg: u32, kb: &KBDLLHOOKSTRUCT) -> bool {
    let mut guard = STATE.lock().unwrap();
    let Some(st) = guard.as_mut() else { return false };
    if !st.captured {
        return false;
    }
    let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
    let vk = VIRTUAL_KEY(kb.vkCode as u16);
    // AltGr gera um LCtrl falso (scancode 0x21D): bloqueia sem repassar.
    if kb.scanCode & 0x200 != 0 {
        return true;
    }
    let kind = match vk {
        VK_LCONTROL | VK_RCONTROL | VK_CONTROL => KeyKind::Ctrl,
        VK_LMENU | VK_RMENU | VK_MENU => KeyKind::Alt,
        VK_PAUSE | VK_CANCEL => KeyKind::Pause,
        _ => KeyKind::Other,
    };
    if st.hotkey.on_key(kind, down) {
        st.captured = false;
        let (x, y) = st.desktop.return_cursor(st.side, st.entry_along);
        set_cursor(x as i32, y as i32);
        let _ = st.tx.send(CapEvent::Released);
        return true;
    }
    let sc = if vk == VK_PAUSE || vk == VK_CANCEL {
        PAUSE_SC
    } else {
        let base = (kb.scanCode & 0xFF) as u16;
        if kb.flags.contains(LLKHF_EXTENDED) && base != 0x36 { 0xE000 | base } else { base }
    };
    match keymap::win_scancode_to_hid(sc) {
        Some(hid) => {
            log::debug!("tecla vk={:#x} sc={sc:#x} -> hid={hid:#x} down={down}", kb.vkCode);
            let _ = st.tx.send(CapEvent::Input(Msg::Key { hid, down }));
        }
        None => log::warn!("tecla sem mapa: vk={:#x} sc={:#x} flags={:#x}", kb.vkCode, kb.scanCode, kb.flags.0),
    }
    true
}
