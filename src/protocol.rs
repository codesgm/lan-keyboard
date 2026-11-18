use crate::config::Side;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Button {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    Hello { version: u32, host: String, screen: (u32, u32) },
    /// Movimento relativo em pixels lógicos do servidor.
    MouseMove { dx: f32, dy: f32 },
    MouseButton { button: Button, down: bool },
    /// Unidades de 1/120 de "clique" de roda (mesmo padrão do Windows). dy > 0 = rolar p/ baixo.
    Scroll { dx: i32, dy: i32 },
    /// `hid` = USB HID usage code (página 0x07).
    Key { hid: u16, down: bool },
    /// Cursor entrou na tela do cliente pela borda `edge` (borda do cliente), em `pos` (0..1).
    Enter { edge: Side, pos: f32 },
    /// Cursor saiu da tela do cliente pela borda `edge`, em `pos` (0..1).
    Leave { edge: Side, pos: f32 },
    /// Solta tudo e para de injetar (atalho de emergência / perda de foco).
    ReleaseAll,
    Heartbeat,
}
