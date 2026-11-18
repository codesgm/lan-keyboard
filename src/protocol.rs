use crate::config::Side;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_MSG: usize = 64 * 1024;

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

/// Frame: u32 big-endian com tamanho + payload bincode.
pub fn encode(msg: &Msg) -> anyhow::Result<Vec<u8>> {
    let payload = bincode::serialize(msg)?;
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub async fn read_msg<R: AsyncRead + Unpin>(r: &mut R) -> anyhow::Result<Msg> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    anyhow::ensure!(len <= MAX_MSG, "mensagem grande demais: {len} bytes");
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(bincode::deserialize(&buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip() {
        let msgs = vec![
            Msg::Hello { version: PROTOCOL_VERSION, host: "pc".into(), screen: (1920, 1080) },
            Msg::MouseMove { dx: -5.5, dy: 12.0 },
            Msg::Key { hid: 0x04, down: true },
            Msg::Enter { edge: Side::Bottom, pos: 0.5 },
            Msg::ReleaseAll,
        ];
        let mut buf = Vec::new();
        for m in &msgs {
            buf.extend(encode(m).unwrap());
        }
        let mut r = buf.as_slice();
        for m in &msgs {
            assert_eq!(&read_msg(&mut r).await.unwrap(), m);
        }
    }

    #[tokio::test]
    async fn rejects_oversized() {
        let mut buf = (MAX_MSG as u32 + 1).to_be_bytes().to_vec();
        buf.extend(vec![0; 8]);
        assert!(read_msg(&mut buf.as_slice()).await.is_err());
    }
}
