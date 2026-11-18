//! Injeção no Windows via SendInput. Desktop virtual = todos os monitores.

use crate::client::Injector;
use crate::config::Side;
use crate::keymap::{self, PAUSE_SC};
use crate::protocol::Button;
use crate::win_util::{cursor, has_monitor, monitor_rect, set_cursor, virtual_screen as virt};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub struct WinInjector;

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, size_of::<INPUT>() as i32);
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx, dy, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

impl WinInjector {
    pub fn new() -> Self {
        crate::win_util::dpi_aware();
        Self
    }
}

impl Injector for WinInjector {
    fn screen(&self) -> (u32, u32) {
        let v = virt();
        (v.w as u32, v.h as u32)
    }

    fn warp_to_edge(&mut self, edge: Side, pos: f32) {
        let v = virt();
        let pos = pos.clamp(0.0, 1.0);
        let along_x = v.x + (pos * (v.w - 1) as f32) as i32;
        let along_y = v.y + (pos * (v.h - 1) as f32) as i32;
        let probe = match edge {
            Side::Bottom => POINT { x: along_x, y: v.y + v.h - 1 },
            Side::Top => POINT { x: along_x, y: v.y },
            Side::Left => POINT { x: v.x, y: along_y },
            Side::Right => POINT { x: v.x + v.w - 1, y: along_y },
        };
        let r = monitor_rect(probe);
        let x = probe.x.clamp(r.left, r.right - 1);
        let y = probe.y.clamp(r.top, r.bottom - 1);
        let (x, y) = match edge {
            Side::Bottom => (x, r.bottom - 3),
            Side::Top => (x, r.top + 2),
            Side::Left => (r.left + 2, y),
            Side::Right => (r.right - 3, y),
        };
        set_cursor(x, y);
    }

    fn move_rel(&mut self, dx: i32, dy: i32, exit: Side) -> Option<f32> {
        let p = cursor();
        let r = monitor_rect(p);
        let v = virt();
        let fx = (p.x - v.x) as f32 / v.w as f32;
        let fy = (p.y - v.y) as f32 / v.h as f32;
        let leaving = match exit {
            Side::Bottom => dy > 0 && p.y + dy > r.bottom - 1 && !has_monitor(p.x, r.bottom),
            Side::Top => dy < 0 && p.y + dy < r.top && !has_monitor(p.x, r.top - 1),
            Side::Right => dx > 0 && p.x + dx > r.right - 1 && !has_monitor(r.right, p.y),
            Side::Left => dx < 0 && p.x + dx < r.left && !has_monitor(r.left - 1, p.y),
        };
        if leaving {
            return Some(match exit {
                Side::Top | Side::Bottom => fx,
                Side::Left | Side::Right => fy,
            });
        }
        if dx != 0 || dy != 0 {
            send(&[mouse(MOUSEEVENTF_MOVE, dx, dy, 0)]);
        }
        None
    }

    fn button(&mut self, b: Button, down: bool) {
        let (flags, data) = match (b, down) {
            (Button::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
            (Button::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
            (Button::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
            (Button::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
            (Button::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
            (Button::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
            (Button::Back, true) => (MOUSEEVENTF_XDOWN, 1),
            (Button::Back, false) => (MOUSEEVENTF_XUP, 1),
            (Button::Forward, true) => (MOUSEEVENTF_XDOWN, 2),
            (Button::Forward, false) => (MOUSEEVENTF_XUP, 2),
        };
        send(&[mouse(flags, 0, 0, data)]);
    }

    fn scroll(&mut self, dx: i32, dy: i32) {
        if dy != 0 {
            // Windows: positivo = rolar p/ cima.
            send(&[mouse(MOUSEEVENTF_WHEEL, 0, 0, (-dy) as u32)]);
        }
        if dx != 0 {
            send(&[mouse(MOUSEEVENTF_HWHEEL, 0, 0, dx as u32)]);
        }
    }

    fn key(&mut self, hid: u16, down: bool) {
        let Some(sc) = keymap::hid_to_win_scancode(hid) else { return };
        let up = if down { KEYBD_EVENT_FLAGS(0) } else { KEYEVENTF_KEYUP };
        let ki = if sc == PAUSE_SC {
            KEYBDINPUT { wVk: VK_PAUSE, wScan: 0, dwFlags: up, time: 0, dwExtraInfo: 0 }
        } else {
            let mut flags = KEYEVENTF_SCANCODE | up;
            if sc & 0xFF00 == 0xE000 {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: sc & 0xFF, dwFlags: flags, time: 0, dwExtraInfo: 0 }
        };
        send(&[INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } }]);
    }
}
