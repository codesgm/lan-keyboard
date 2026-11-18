//! Injeção no Linux via uinput. Ponteiro absoluto: posição rastreada aqui,
//! então a borda de saída é exata mesmo no Wayland.

use crate::client::Injector;
use crate::config::Side;
use crate::keymap;
use crate::protocol::Button;
use anyhow::Context as _;
use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode,
    UinputAbsSetup,
};

pub struct LinuxInjector {
    kbd: VirtualDevice,
    ptr: VirtualDevice,
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    wheel: (i32, i32),
}

impl LinuxInjector {
    pub fn new(fallback: (u32, u32)) -> anyhow::Result<Self> {
        let (w, h) = screen_size().unwrap_or((fallback.0 as i32, fallback.1 as i32));
        let hint = "sem acesso a /dev/uinput (usuário no grupo input? faça logout/login)";

        let mut keys = AttributeSet::<KeyCode>::new();
        for code in keymap::all_evdev() {
            keys.insert(KeyCode(code));
        }
        let kbd = VirtualDevice::builder()
            .context(hint)?
            .name("lan-keyboard teclado")
            .with_keys(&keys)?
            .build()
            .context(hint)?;

        let mut btns = AttributeSet::<KeyCode>::new();
        for b in [KeyCode::BTN_LEFT, KeyCode::BTN_RIGHT, KeyCode::BTN_MIDDLE, KeyCode::BTN_SIDE, KeyCode::BTN_EXTRA] {
            btns.insert(b);
        }
        let mut rel = AttributeSet::<RelativeAxisCode>::new();
        for r in [
            RelativeAxisCode::REL_WHEEL,
            RelativeAxisCode::REL_HWHEEL,
            RelativeAxisCode::REL_WHEEL_HI_RES,
            RelativeAxisCode::REL_HWHEEL_HI_RES,
        ] {
            rel.insert(r);
        }
        let ptr = VirtualDevice::builder()?
            .name("lan-keyboard mouse")
            .with_keys(&btns)?
            .with_relative_axes(&rel)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisCode::ABS_X, AbsInfo::new(0, 0, w - 1, 0, 0, 0)))?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisCode::ABS_Y, AbsInfo::new(0, 0, h - 1, 0, 0, 0)))?
            .build()
            .context(hint)?;
        log::info!("uinput pronto, tela {w}x{h}");
        Ok(Self { kbd, ptr, w, h, x: w / 2, y: h / 2, wheel: (0, 0) })
    }

    fn emit_pos(&mut self) {
        let ev = [
            InputEvent::new(EventType::ABSOLUTE.0, AbsoluteAxisCode::ABS_X.0, self.x),
            InputEvent::new(EventType::ABSOLUTE.0, AbsoluteAxisCode::ABS_Y.0, self.y),
        ];
        let _ = self.ptr.emit(&ev);
    }
}

/// Tamanho do desktop lógico via XWayland (`xrandr`).
fn screen_size() -> Option<(i32, i32)> {
    let out = std::process::Command::new("xrandr").arg("--current").output().ok()?;
    parse_xrandr(&String::from_utf8_lossy(&out.stdout))
}

fn parse_xrandr(s: &str) -> Option<(i32, i32)> {
    let line = s.lines().find(|l| l.starts_with("Screen "))?;
    let cur = line.split("current ").nth(1)?;
    let mut it = cur.split(',').next()?.split(" x ");
    Some((it.next()?.trim().parse().ok()?, it.next()?.trim().parse().ok()?))
}

impl Injector for LinuxInjector {
    fn screen(&self) -> (u32, u32) {
        (self.w as u32, self.h as u32)
    }

    fn warp_to_edge(&mut self, edge: Side, pos: f32) {
        let pos = pos.clamp(0.0, 1.0);
        let ax = (pos * (self.w - 1) as f32) as i32;
        let ay = (pos * (self.h - 1) as f32) as i32;
        (self.x, self.y) = match edge {
            Side::Bottom => (ax, self.h - 3),
            Side::Top => (ax, 2),
            Side::Left => (2, ay),
            Side::Right => (self.w - 3, ay),
        };
        self.emit_pos();
    }

    fn move_rel(&mut self, dx: i32, dy: i32, exit: Side) -> Option<f32> {
        let (nx, ny) = (self.x + dx, self.y + dy);
        let fx = self.x as f32 / self.w as f32;
        let fy = self.y as f32 / self.h as f32;
        let out = match exit {
            Side::Bottom => (ny > self.h - 1).then_some(fx),
            Side::Top => (ny < 0).then_some(fx),
            Side::Right => (nx > self.w - 1).then_some(fy),
            Side::Left => (nx < 0).then_some(fy),
        };
        if out.is_some() {
            return out;
        }
        self.x = nx.clamp(0, self.w - 1);
        self.y = ny.clamp(0, self.h - 1);
        self.emit_pos();
        None
    }

    fn button(&mut self, b: Button, down: bool) {
        let code = match b {
            Button::Left => KeyCode::BTN_LEFT,
            Button::Right => KeyCode::BTN_RIGHT,
            Button::Middle => KeyCode::BTN_MIDDLE,
            Button::Back => KeyCode::BTN_SIDE,
            Button::Forward => KeyCode::BTN_EXTRA,
        };
        let _ = self.ptr.emit(&[InputEvent::new(EventType::KEY.0, code.0, down as i32)]);
    }

    fn scroll(&mut self, dx: i32, dy: i32) {
        // Protocolo: dy>0 = p/ baixo. evdev: REL_WHEEL>0 = p/ cima.
        let mut ev = Vec::new();
        if dy != 0 {
            ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_WHEEL_HI_RES.0, -dy));
            self.wheel.1 -= dy;
            let clicks = self.wheel.1 / 120;
            if clicks != 0 {
                self.wheel.1 -= clicks * 120;
                ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_WHEEL.0, clicks));
            }
        }
        if dx != 0 {
            ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_HWHEEL_HI_RES.0, dx));
            self.wheel.0 += dx;
            let clicks = self.wheel.0 / 120;
            if clicks != 0 {
                self.wheel.0 -= clicks * 120;
                ev.push(InputEvent::new(EventType::RELATIVE.0, RelativeAxisCode::REL_HWHEEL.0, clicks));
            }
        }
        if !ev.is_empty() {
            let _ = self.ptr.emit(&ev);
        }
    }

    fn key(&mut self, hid: u16, down: bool) {
        match keymap::hid_to_evdev(hid) {
            Some(code) => {
                if let Err(e) = self.kbd.emit(&[InputEvent::new(EventType::KEY.0, code, down as i32)]) {
                    log::warn!("uinput tecla {code}: {e}");
                }
            }
            None => log::warn!("hid {hid:#04x} sem evdev"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xrandr_parse() {
        let s = "Screen 0: minimum 16 x 16, current 3840 x 1080, maximum 32767 x 32767\nXWAYLAND0 connected";
        assert_eq!(parse_xrandr(s), Some((3840, 1080)));
    }
}
