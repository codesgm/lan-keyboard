//! Servidor genérico: rede + troca de tela. Captura vem de um backend por SO
//! (`capture_linux` / `capture_windows`) via canais.

use crate::config::{Config, Side};
use crate::protocol::{Msg, PROTOCOL_VERSION};
use crate::secure::{self, Sealer};
use crate::switch::Layout;
use crate::worker::Shared;
use anyhow::Context as _;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// Área total do desktop local (bounding box dos monitores).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Desktop {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Desktop {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn bounding_box(regions: impl Iterator<Item = (i32, i32, i32, i32)>) -> Option<Desktop> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for (x, y, w, h) in regions {
            b = Some(match b {
                None => (x, y, x + w, y + h),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + w), y1.max(y + h)),
            });
        }
        b.map(|(x0, y0, x1, y1)| Desktop { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }

    /// Posição do cursor ao longo da borda, relativa ao desktop.
    pub fn along_edge(&self, side: Side, x: f32, y: f32) -> f32 {
        match side {
            Side::Top | Side::Bottom => x - self.x as f32,
            Side::Left | Side::Right => y - self.y as f32,
        }
    }

    /// Onde pôr o cursor local ao voltar: alguns px p/ dentro da borda.
    pub fn return_cursor(&self, side: Side, along: f32) -> (f64, f64) {
        let along = along as f64;
        let (x, y, w, h) = (self.x as f64, self.y as f64, self.w as f64, self.h as f64);
        match side {
            Side::Top => (x + along, y + 4.0),
            Side::Bottom => (x + along, y + h - 5.0),
            Side::Left => (x + 4.0, y + along),
            Side::Right => (x + w - 5.0, y + along),
        }
    }
}

pub enum CapEvent {
    /// Cursor bateu na borda; captura ativa. `along` = posição na borda (px).
    Activated { along: f32 },
    Input(Msg),
    /// Captura terminou sem pedido (atalho de emergência ou SO).
    Released,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Failed(String),
}

pub enum CapCmd {
    /// Termina a captura e põe cursor local em `along` na borda.
    Release { along: f32 },
}

pub struct CaptureHandle {
    pub desktop: Desktop,
    pub events: mpsc::UnboundedReceiver<CapEvent>,
    pub cmds: mpsc::UnboundedSender<CapCmd>,
}

#[cfg(target_os = "linux")]
async fn start_capture(side: Side) -> anyhow::Result<CaptureHandle> {
    crate::capture_linux::start(side).await
}

#[cfg(windows)]
async fn start_capture(side: Side) -> anyhow::Result<CaptureHandle> {
    crate::capture_windows::start(side)
}

enum ClientEvent {
    Connected { id: u64, writer: OwnedWriteHalf, sealer: Sealer, screen: (u32, u32), host: String },
    Msg { id: u64, msg: Msg },
    Gone { id: u64 },
}

struct Client {
    id: u64,
    writer: OwnedWriteHalf,
    sealer: Sealer,
    screen: (u32, u32),
}

impl Client {
    async fn send(&mut self, msg: &Msg) -> anyhow::Result<()> {
        let frame = self.sealer.seal(msg)?;
        self.writer.write_all(&frame).await?;
        Ok(())
    }
}

/// Anti força bruta: máx. 5 handshakes falhos por minuto.
#[derive(Default)]
struct Limiter {
    fails: VecDeque<Instant>,
}

const MAX_FAILS: usize = 5;
const FAIL_WINDOW: Duration = Duration::from_secs(60);

impl Limiter {
    fn prune(&mut self) {
        while self.fails.front().is_some_and(|t| t.elapsed() > FAIL_WINDOW) {
            self.fails.pop_front();
        }
    }
    fn blocked(&mut self) -> bool {
        self.prune();
        self.fails.len() >= MAX_FAILS
    }
    fn fail(&mut self) {
        self.fails.push_back(Instant::now());
    }
}

pub async fn run(cfg: Config, shared: Shared) -> anyhow::Result<()> {
    secure::check_pin(&cfg.pin)?;
    let listener = TcpListener::bind(("0.0.0.0", cfg.port))
        .await
        .with_context(|| format!("porta {} ocupada?", cfg.port))?;

    shared.status("Iniciando captura…");
    let CaptureHandle { desktop, events: mut cap, cmds } = start_capture(cfg.remote_side).await?;
    shared.update(|s| s.local_size = Some((desktop.w as u32, desktop.h as u32)));
    log::info!("desktop local {desktop:?}");

    let (ctx_tx, mut ctx_rx) = mpsc::channel::<ClientEvent>(256);
    let mut next_id = 0u64;
    let mut client: Option<Client> = None;
    // Some(along na entrada) = captura ativa e cliente controlando.
    let mut captured: Option<f32> = None;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
    let awake = crate::awake::Awake::start();
    let mut awake_tick = tokio::time::interval(crate::awake::REPORT_EVERY);
    let limiter = Arc::new(Mutex::new(Limiter::default()));
    let hello = Msg::Hello { version: PROTOCOL_VERSION, host: hostname(), screen: (desktop.w as u32, desktop.h as u32) };
    let release = |along: f32| {
        let _ = cmds.send(CapCmd::Release { along });
    };
    let layout = |c: &Client| Layout {
        side: cfg.remote_side,
        offset: cfg.remote_offset,
        local: (desktop.w as f32, desktop.h as f32),
        remote: (c.screen.0 as f32, c.screen.1 as f32),
    };

    shared.status(format!("Aguardando cliente na porta {}…", cfg.port));
    loop {
        tokio::select! {
            acc = listener.accept() => {
                let (sock, addr) = acc?;
                if limiter.lock().unwrap().blocked() {
                    log::warn!("recusando {addr}: muitas tentativas de PIN erradas, aguarde 1 min");
                    shared.status("Muitas tentativas de PIN erradas. Bloqueado por 1 min.");
                    continue;
                }
                next_id += 1;
                log::info!("conexão de {addr}");
                tokio::spawn(handle_client(next_id, sock, ctx_tx.clone(), cfg.pin.clone(), hello.clone(), limiter.clone()));
            }
            Some(ev) = ctx_rx.recv() => match ev {
                ClientEvent::Connected { id, writer, sealer, screen, host } => {
                    if let Some(along) = captured.take() {
                        release(along);
                    }
                    shared.update(|s| s.remote_size = Some(screen));
                    shared.status(format!("Conectado: {host} ({}x{})", screen.0, screen.1));
                    client = Some(Client { id, writer, sealer, screen });
                }
                ClientEvent::Msg { id, msg: Msg::Leave { pos, .. } } if client.as_ref().is_some_and(|c| c.id == id) => {
                    log::info!("cliente saiu pela borda em {pos}");
                    if captured.take().is_some() {
                        release(layout(client.as_ref().unwrap()).remote_to_local(pos));
                    }
                }
                ClientEvent::Msg { id, msg: Msg::Awake } if client.as_ref().is_some_and(|c| c.id == id) => {
                    if cfg.keep_awake {
                        awake.poke();
                    }
                }
                ClientEvent::Gone { id } if client.as_ref().is_some_and(|c| c.id == id) => {
                    client = None;
                    if let Some(along) = captured.take() {
                        release(along);
                    }
                    shared.status(format!("Cliente desconectou. Aguardando na porta {}…", cfg.port));
                }
                _ => {}
            },
            ev = cap.recv() => match ev {
                None => anyhow::bail!("captura encerrou"),
                Some(CapEvent::Failed(e)) => anyhow::bail!("captura: {e}"),
                Some(CapEvent::Activated { along }) => {
                    log::info!("captura ativada em {along}");
                    match client.as_mut() {
                        Some(c) => {
                            let pos = layout(c).local_to_remote(along);
                            if c.send(&Msg::Enter { edge: cfg.remote_side.opposite(), pos }).await.is_ok() {
                                captured = Some(along);
                            } else {
                                release(along);
                            }
                        }
                        None => release(along),
                    }
                }
                Some(CapEvent::Released) => {
                    log::info!("captura liberada localmente");
                    if captured.take().is_some() {
                        if let Some(c) = client.as_mut() {
                            let _ = c.send(&Msg::ReleaseAll).await;
                        }
                    }
                }
                Some(CapEvent::Input(msg)) => {
                    if let (Some(along), Some(c)) = (captured, client.as_mut()) {
                        if c.send(&msg).await.is_err() {
                            captured = None;
                            release(along);
                        }
                    }
                }
            },
            _ = heartbeat.tick() => {
                if let Some(c) = client.as_mut() {
                    let _ = c.send(&Msg::Heartbeat).await;
                }
            }
            _ = awake_tick.tick() => {
                if let Some(c) = client.as_mut() {
                    if cfg.keep_awake && awake.user_active() {
                        let _ = c.send(&Msg::Awake).await;
                    }
                }
            }
        }
    }
}

async fn handle_client(
    id: u64,
    sock: TcpStream,
    tx: mpsc::Sender<ClientEvent>,
    pin: String,
    hello: Msg,
    limiter: Arc<Mutex<Limiter>>,
) {
    let _ = sock.set_nodelay(true);
    let (mut reader, mut writer) = sock.into_split();
    // Handshake + Hellos em até 5s; qualquer falha conta como tentativa de PIN.
    let auth = async {
        let (mut sealer, mut opener) = secure::handshake(&mut reader, &mut writer, &pin, true).await?;
        let frame = sealer.seal(&hello)?;
        writer.write_all(&frame).await?;
        match opener.read(&mut reader).await? {
            Msg::Hello { version, host, screen } if version == PROTOCOL_VERSION => Ok((sealer, opener, host, screen)),
            Msg::Hello { version, .. } => anyhow::bail!("versão do cliente {version} ≠ {PROTOCOL_VERSION}"),
            other => anyhow::bail!("esperava Hello, veio {other:?}"),
        }
    };
    let (sealer, mut opener, host, screen) = match tokio::time::timeout(Duration::from_secs(5), auth).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            log::warn!("cliente {id} recusado: {e:#}");
            limiter.lock().unwrap().fail();
            return;
        }
        Err(_) => {
            log::warn!("cliente {id} recusado: timeout no handshake");
            limiter.lock().unwrap().fail();
            return;
        }
    };
    if tx.send(ClientEvent::Connected { id, writer, sealer, screen, host }).await.is_err() {
        return;
    }
    loop {
        match tokio::time::timeout(Duration::from_secs(4), opener.read(&mut reader)).await {
            Ok(Ok(msg)) => {
                if tx.send(ClientEvent::Msg { id, msg }).await.is_err() {
                    return;
                }
            }
            Ok(Err(e)) => {
                log::info!("cliente {id} saiu: {e}");
                break;
            }
            Err(_) => {
                log::warn!("cliente {id} sem heartbeat");
                break;
            }
        }
    }
    let _ = tx.send(ClientEvent::Gone { id }).await;
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string()))
        .unwrap_or_else(|| "pc".into())
}

/// Detecta Ctrl+Alt+Pause. Recebe códigos evdev (Linux) ou VK (Windows) já classificados.
#[derive(Default)]
pub struct Hotkey {
    ctrl: u8,
    alt: u8,
}

pub enum KeyKind {
    Ctrl,
    Alt,
    Pause,
    Other,
}

impl Hotkey {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Retorna true quando Ctrl+Alt+Pause é pressionado.
    pub fn on_key(&mut self, kind: KeyKind, down: bool) -> bool {
        let d = |v: &mut u8| if down { *v = v.saturating_add(1) } else { *v = v.saturating_sub(1) };
        match kind {
            KeyKind::Ctrl => d(&mut self.ctrl),
            KeyKind::Alt => d(&mut self.alt),
            KeyKind::Pause => return down && self.ctrl > 0 && self.alt > 0,
            KeyKind::Other => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_two_monitors() {
        let d = Desktop::bounding_box([(0, 0, 1920, 1080), (1920, -200, 2560, 1440)].into_iter()).unwrap();
        assert_eq!(d, Desktop { x: 0, y: -200, w: 4480, h: 1440 });
    }

    #[test]
    fn limiter_blocks_after_fails() {
        let mut l = Limiter::default();
        for _ in 0..MAX_FAILS - 1 {
            l.fail();
        }
        assert!(!l.blocked());
        l.fail();
        assert!(l.blocked());
    }

    #[test]
    fn emergency_hotkey() {
        let mut h = Hotkey::default();
        assert!(!h.on_key(KeyKind::Pause, true));
        h.on_key(KeyKind::Ctrl, true);
        h.on_key(KeyKind::Alt, true);
        assert!(h.on_key(KeyKind::Pause, true));
        h.on_key(KeyKind::Ctrl, false);
        assert!(!h.on_key(KeyKind::Pause, true));
    }
}
