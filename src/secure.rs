//! Canal seguro com PIN: SPAKE2 (PIN nunca trafega; cada conexão = 1 tentativa,
//! sem ataque offline) + ChaCha20-Poly1305 com nonce contador por direção.

use crate::protocol::Msg;
use anyhow::Context as _;
use chacha20poly1305::aead::{Aead, Key, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME: usize = 64 * 1024;
pub const MIN_PIN: usize = 4;

/// PIN não confere (falha ao decifrar a 1ª mensagem).
#[derive(Debug)]
pub struct AuthError;

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PIN incorreto")
    }
}

impl std::error::Error for AuthError {}

pub fn check_pin(pin: &str) -> anyhow::Result<()> {
    anyhow::ensure!(pin.trim().chars().count() >= MIN_PIN, "PIN precisa de pelo menos {MIN_PIN} caracteres");
    Ok(())
}

pub fn random_pin() -> String {
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).expect("RNG do SO");
    format!("{:06}", u32::from_le_bytes(b) % 1_000_000)
}

fn nonce(n: u64) -> Nonce {
    let mut b = [0u8; 12];
    b[4..].copy_from_slice(&n.to_be_bytes());
    Nonce::from(b)
}

fn derive(k: &[u8], label: &[u8]) -> ChaCha20Poly1305 {
    let mut h = Sha256::new();
    h.update(b"lan-keyboard v1 ");
    h.update(label);
    h.update(k);
    let key: [u8; 32] = h.finalize().into();
    ChaCha20Poly1305::new(&Key::<ChaCha20Poly1305>::from(key))
}

pub struct Sealer {
    cipher: ChaCha20Poly1305,
    n: u64,
}

impl Sealer {
    /// Frame: u32 BE tamanho + ciphertext.
    pub fn seal(&mut self, msg: &Msg) -> anyhow::Result<Vec<u8>> {
        let pt = bincode::serialize(msg)?;
        let ct = self.cipher.encrypt(&nonce(self.n), pt.as_slice()).map_err(|_| anyhow::anyhow!("cifrar"))?;
        self.n += 1;
        let mut out = Vec::with_capacity(4 + ct.len());
        out.extend_from_slice(&(ct.len() as u32).to_be_bytes());
        out.extend_from_slice(&ct);
        Ok(out)
    }
}

pub struct Opener {
    cipher: ChaCha20Poly1305,
    n: u64,
}

impl Opener {
    pub async fn read<R: AsyncRead + Unpin>(&mut self, r: &mut R) -> anyhow::Result<Msg> {
        let ct = read_frame(r).await?;
        let pt = self.cipher.decrypt(&nonce(self.n), ct.as_slice()).map_err(|_| AuthError)?;
        self.n += 1;
        Ok(bincode::deserialize(&pt)?)
    }
}

async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> anyhow::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    anyhow::ensure!(len <= MAX_FRAME, "frame grande demais: {len}");
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Troca SPAKE2 e deriva chaves por direção. Não autentica sozinho:
/// PIN errado aparece como `AuthError` na 1ª leitura.
pub async fn handshake<R, W>(r: &mut R, w: &mut W, pin: &str, is_server: bool) -> anyhow::Result<(Sealer, Opener)>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (state, out) = Spake2::<Ed25519Group>::start_symmetric(
        &Password::new(pin.trim().as_bytes()),
        &Identity::new(b"lan-keyboard"),
    );
    let mut frame = (out.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&out);
    w.write_all(&frame).await?;
    let inbound = read_frame(r).await?;
    let k = state.finish(&inbound).map_err(|e| anyhow::anyhow!("SPAKE2: {e:?}")).context("handshake")?;
    let s2c = derive(&k, b"s2c");
    let c2s = derive(&k, b"c2s");
    Ok(if is_server {
        (Sealer { cipher: s2c, n: 0 }, Opener { cipher: c2s, n: 0 })
    } else {
        (Sealer { cipher: c2s, n: 0 }, Opener { cipher: s2c, n: 0 })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pair(pin_s: &'static str, pin_c: &'static str) -> anyhow::Result<Msg> {
        let (a, b) = tokio::io::duplex(4096);
        let (mut ar, mut aw) = tokio::io::split(a);
        let (mut br, mut bw) = tokio::io::split(b);
        let server = tokio::spawn(async move {
            let (mut s, _o) = handshake(&mut ar, &mut aw, pin_s, true).await.unwrap();
            let f = s.seal(&Msg::Heartbeat).unwrap();
            aw.write_all(&f).await.unwrap();
            let f = s.seal(&Msg::Key { hid: 4, down: true }).unwrap();
            aw.write_all(&f).await.unwrap();
        });
        let (_s, mut o) = handshake(&mut br, &mut bw, pin_c, false).await?;
        o.read(&mut br).await?;
        let m = o.read(&mut br).await;
        server.await.unwrap();
        m
    }

    #[tokio::test]
    async fn same_pin_ok() {
        assert_eq!(pair("123456", "123456").await.unwrap(), Msg::Key { hid: 4, down: true });
    }

    #[tokio::test]
    async fn wrong_pin_fails() {
        let e = pair("123456", "654321").await.unwrap_err();
        assert!(e.downcast_ref::<AuthError>().is_some());
    }

    #[test]
    fn pin_rules() {
        assert!(check_pin("123").is_err());
        assert!(check_pin("1234").is_ok());
        assert_eq!(random_pin().len(), 6);
    }
}
