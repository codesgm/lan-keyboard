//! Cliente: conecta no servidor, recebe eventos e injeta. Reconecta sozinho.

use crate::config::{Config, Side};
use crate::protocol::{self, Button, Msg, PROTOCOL_VERSION};
use crate::worker::Shared;
use anyhow::Context as _;
use std::collections::HashSet;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

/// Injeção de input do SO. Coordenadas da tela = desktop virtual inteiro.
pub trait Injector: Send {
    fn screen(&self) -> (u32, u32);
    /// Coloca cursor na borda `edge`, posição `pos` (0..1).
    fn warp_to_edge(&mut self, edge: Side, pos: f32);
    /// Move relativo. Retorna Some(pos) se o cursor tentou passar da borda `exit` (sem monitor além).
    fn move_rel(&mut self, dx: i32, dy: i32, exit: Side) -> Option<f32>;
    fn button(&mut self, b: Button, down: bool);
    fn scroll(&mut self, dx: i32, dy: i32);
    fn key(&mut self, hid: u16, down: bool);
}

#[cfg(windows)]
fn new_injector(_cfg: &Config) -> anyhow::Result<Box<dyn Injector>> {
    Ok(Box::new(crate::inject_windows::WinInjector::new()))
}

#[cfg(target_os = "linux")]
fn new_injector(cfg: &Config) -> anyhow::Result<Box<dyn Injector>> {
    if std::env::var_os("LANKB_SIM").is_some() {
        log::warn!("LANKB_SIM: cliente simulado (só loga eventos)");
        return Ok(Box::new(LogInjector { x: 960, y: 540 }));
    }
    Ok(Box::new(crate::inject_linux::LinuxInjector::new(cfg.local_size)?))
}

pub async fn run(cfg: Config, shared: Shared) -> anyhow::Result<()> {
    let mut inj = new_injector(&cfg)?;
    loop {
        shared.status(format!("Conectando em {}:{}…", cfg.server_addr, cfg.port));
        match session(&cfg, &shared, inj.as_mut()).await {
            Ok(()) => {}
            Err(e) => shared.status(format!("Desconectado: {e:#}. Tentando de novo em 2s…")),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

struct State {
    active: bool,
    /// Borda por onde o cursor entrou (= borda de saída).
    exit: Side,
    keys: HashSet<u16>,
    buttons: HashSet<Button>,
    rem: (f32, f32),
    scroll_rem: (f32, f32),
}

impl State {
    fn release_all(&mut self, inj: &mut dyn Injector) {
        for k in self.keys.drain() {
            inj.key(k, false);
        }
        for b in self.buttons.drain() {
            inj.button(b, false);
        }
        self.active = false;
    }
}

async fn session(cfg: &Config, shared: &Shared, inj: &mut dyn Injector) -> anyhow::Result<()> {
    let addr = (cfg.server_addr.trim(), cfg.port);
    let sock = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .context("timeout")?
        .context("falha ao conectar")?;
    sock.set_nodelay(true)?;
    let (mut reader, mut writer) = sock.into_split();
    let screen = inj.screen();
    let hello = Msg::Hello { version: PROTOCOL_VERSION, host: crate::server::hostname(), screen };
    writer.write_all(&protocol::encode(&hello)?).await?;

    match tokio::time::timeout(Duration::from_secs(5), protocol::read_msg(&mut reader)).await?? {
        Msg::Hello { version, host, .. } => {
            anyhow::ensure!(version == PROTOCOL_VERSION, "versão do servidor {version} ≠ {PROTOCOL_VERSION}");
            shared.status(format!("Conectado a {host}"));
        }
        other => anyhow::bail!("esperava Hello, veio {other:?}"),
    }

    let mut st = State {
        active: false,
        exit: Side::Bottom,
        keys: HashSet::new(),
        buttons: HashSet::new(),
        rem: (0.0, 0.0),
        scroll_rem: (0.0, 0.0),
    };
    let mut hb = tokio::time::interval(Duration::from_secs(1));
    let res = loop {
        tokio::select! {
            msg = tokio::time::timeout(Duration::from_secs(4), protocol::read_msg(&mut reader)) => {
                let msg = match msg {
                    Ok(Ok(m)) => m,
                    Ok(Err(e)) => break Err(e),
                    Err(_) => break Err(anyhow::anyhow!("servidor sem heartbeat")),
                };
                if let Some(out) = handle(msg, &mut st, cfg, inj) {
                    if let Err(e) = writer.write_all(&protocol::encode(&out)?).await {
                        break Err(e.into());
                    }
                }
            }
            _ = hb.tick() => {
                if let Err(e) = writer.write_all(&protocol::encode(&Msg::Heartbeat)?).await {
                    break Err(e.into());
                }
            }
        }
    };
    st.release_all(inj);
    res
}

fn handle(msg: Msg, st: &mut State, cfg: &Config, inj: &mut dyn Injector) -> Option<Msg> {
    match msg {
        Msg::Enter { edge, pos } => {
            st.active = true;
            st.exit = edge;
            st.rem = (0.0, 0.0);
            inj.warp_to_edge(edge, pos);
        }
        Msg::ReleaseAll => st.release_all(inj),
        _ if !st.active => {}
        Msg::MouseMove { dx, dy } => {
            let fx = dx * cfg.sensitivity + st.rem.0;
            let fy = dy * cfg.sensitivity + st.rem.1;
            let (ix, iy) = (fx.trunc(), fy.trunc());
            st.rem = (fx - ix, fy - iy);
            if let Some(pos) = inj.move_rel(ix as i32, iy as i32, st.exit) {
                let edge = st.exit;
                st.release_all(inj);
                return Some(Msg::Leave { edge, pos });
            }
        }
        Msg::MouseButton { button, down } => {
            if down {
                st.buttons.insert(button);
            } else {
                st.buttons.remove(&button);
            }
            inj.button(button, down);
        }
        Msg::Scroll { dx, dy } => {
            let fx = dx as f32 * cfg.scroll_speed + st.scroll_rem.0;
            let fy = dy as f32 * cfg.scroll_speed + st.scroll_rem.1;
            let (ix, iy) = (fx.trunc(), fy.trunc());
            st.scroll_rem = (fx - ix, fy - iy);
            inj.scroll(ix as i32, iy as i32);
        }
        Msg::Key { hid, down } => {
            if down {
                st.keys.insert(hid);
            } else {
                st.keys.remove(&hid);
            }
            inj.key(hid, down);
        }
        Msg::Hello { .. } | Msg::Leave { .. } | Msg::Heartbeat => {}
    }
    None
}


#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        log: Vec<String>,
        y: i32,
    }

    impl Injector for Fake {
        fn screen(&self) -> (u32, u32) {
            (100, 100)
        }
        fn warp_to_edge(&mut self, edge: Side, pos: f32) {
            self.y = 99;
            self.log.push(format!("warp {edge:?} {pos}"));
        }
        fn move_rel(&mut self, _dx: i32, dy: i32, exit: Side) -> Option<f32> {
            if exit == Side::Bottom && self.y >= 99 && dy > 0 {
                return Some(0.5);
            }
            self.y = (self.y + dy).clamp(0, 99);
            None
        }
        fn button(&mut self, b: Button, down: bool) {
            self.log.push(format!("btn {b:?} {down}"));
        }
        fn scroll(&mut self, _dx: i32, dy: i32) {
            self.log.push(format!("scroll {dy}"));
        }
        fn key(&mut self, hid: u16, down: bool) {
            self.log.push(format!("key {hid} {down}"));
        }
    }

    #[test]
    fn enter_move_leave_releases_keys() {
        let cfg = Config { sensitivity: 2.0, ..Config::default() };
        let mut st = State { active: false, exit: Side::Top, keys: HashSet::new(), buttons: HashSet::new(), rem: (0.0, 0.0), scroll_rem: (0.0, 0.0) };
        let mut f = Fake::default();
        assert!(handle(Msg::Key { hid: 4, down: true }, &mut st, &cfg, &mut f).is_none());
        assert!(f.log.is_empty(), "inativo não injeta");
        handle(Msg::Enter { edge: Side::Bottom, pos: 0.5 }, &mut st, &cfg, &mut f);
        handle(Msg::Key { hid: 4, down: true }, &mut st, &cfg, &mut f);
        handle(Msg::MouseMove { dx: 0.0, dy: -10.0 }, &mut st, &cfg, &mut f);
        assert_eq!(f.y, 79, "sensibilidade 2x");
        f.y = 99;
        let out = handle(Msg::MouseMove { dx: 0.0, dy: 1.0 }, &mut st, &cfg, &mut f);
        assert_eq!(out, Some(Msg::Leave { edge: Side::Bottom, pos: 0.5 }));
        assert!(!st.active);
        assert_eq!(f.log.last().unwrap(), "key 4 false");
    }
}

/// Cliente falso p/ teste: tela 1920x1080, só loga.
#[allow(dead_code)]
struct LogInjector {
    x: i32,
    y: i32,
}

impl Injector for LogInjector {
    fn screen(&self) -> (u32, u32) {
        (1920, 1080)
    }
    fn warp_to_edge(&mut self, edge: Side, pos: f32) {
        (self.x, self.y) = match edge {
            Side::Bottom => ((pos * 1919.0) as i32, 1077),
            Side::Top => ((pos * 1919.0) as i32, 2),
            Side::Left => (2, (pos * 1079.0) as i32),
            Side::Right => (1917, (pos * 1079.0) as i32),
        };
        log::info!("[sim] entrou por {edge:?} em ({}, {})", self.x, self.y);
    }
    fn move_rel(&mut self, dx: i32, dy: i32, exit: Side) -> Option<f32> {
        let (nx, ny) = (self.x + dx, self.y + dy);
        let out = match exit {
            Side::Bottom => (ny > 1079).then(|| self.x as f32 / 1920.0),
            Side::Top => (ny < 0).then(|| self.x as f32 / 1920.0),
            Side::Right => (nx > 1919).then(|| self.y as f32 / 1080.0),
            Side::Left => (nx < 0).then(|| self.y as f32 / 1080.0),
        };
        if out.is_some() {
            log::info!("[sim] saiu por {exit:?}");
        }
        self.x = nx.clamp(0, 1919);
        self.y = ny.clamp(0, 1079);
        log::debug!("[sim] cursor ({}, {})", self.x, self.y);
        out
    }
    fn button(&mut self, b: Button, down: bool) {
        log::info!("[sim] botão {b:?} {down}");
    }
    fn scroll(&mut self, dx: i32, dy: i32) {
        log::info!("[sim] scroll {dx} {dy}");
    }
    fn key(&mut self, hid: u16, down: bool) {
        log::info!("[sim] tecla hid={hid:#04x} {down}");
    }
}

