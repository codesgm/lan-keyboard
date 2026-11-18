use crate::config::Side;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_MSG: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    MouseMove { dx: i32, dy: i32 },
    MouseButton { button: Button, down: bool },
    Scroll { dx: i32, dy: i32 },
    /// `hid` = USB HID usage code (página 0x07).
    Key { hid: u16, down: bool },
    /// Cursor entrou na tela do receptor pela borda `edge`, em `pos` (0..1 ao longo da borda).
    Enter { edge: Side, pos: f32 },
    /// Cursor saiu da tela do cliente pela borda `edge`, em `pos`.
    Leave { edge: Side, pos: f32 },
    ReleaseAll,
    Heartbeat,
}

/// Frame: u32 big-endian com tamanho + payload bincode.
pub fn write_msg<W: Write>(w: &mut W, msg: &Msg) -> anyhow::Result<()> {
    let payload = bincode::serialize(msg)?;
    w.write_all(&(payload.len() as u32).to_be_bytes())?;
    w.write_all(&payload)?;
    Ok(())
}

pub fn read_msg<R: Read>(r: &mut R) -> anyhow::Result<Msg> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    anyhow::ensure!(len <= MAX_MSG, "mensagem grande demais: {len} bytes");
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(bincode::deserialize(&buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let msgs = vec![
            Msg::Hello { version: PROTOCOL_VERSION, host: "pc".into(), screen: (1920, 1080) },
            Msg::MouseMove { dx: -5, dy: 12 },
            Msg::Key { hid: 0x04, down: true },
            Msg::Enter { edge: Side::Left, pos: 0.5 },
            Msg::ReleaseAll,
        ];
        let mut buf = Vec::new();
        for m in &msgs {
            write_msg(&mut buf, m).unwrap();
        }
        let mut cur = std::io::Cursor::new(buf);
        for m in &msgs {
            assert_eq!(&read_msg(&mut cur).unwrap(), m);
        }
    }

    #[test]
    fn rejects_oversized() {
        let mut buf = (MAX_MSG as u32 + 1).to_be_bytes().to_vec();
        buf.extend(vec![0; 8]);
        assert!(read_msg(&mut std::io::Cursor::new(buf)).is_err());
    }
}
