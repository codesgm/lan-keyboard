//! Roda servidor/cliente numa thread com runtime tokio. GUI lê `Shared`.

use crate::config::{Config, Role};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::sync::oneshot;

#[derive(Default)]
pub struct SharedState {
    pub status: String,
    pub local_size: Option<(u32, u32)>,
    pub remote_size: Option<(u32, u32)>,
}

#[derive(Clone)]
pub struct Shared {
    inner: Arc<Mutex<SharedState>>,
    ctx: Option<eframe::egui::Context>,
}

impl Shared {
    pub fn new(ctx: Option<eframe::egui::Context>) -> Self {
        Self { inner: Default::default(), ctx }
    }

    pub fn update(&self, f: impl FnOnce(&mut SharedState)) {
        f(&mut self.inner.lock().unwrap());
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    pub fn status(&self, s: impl Into<String>) {
        let s = s.into();
        log::info!("{s}");
        self.update(|st| st.status = s);
    }

    pub fn read<R>(&self, f: impl FnOnce(&SharedState) -> R) -> R {
        f(&self.inner.lock().unwrap())
    }
}

/// Sem GUI (autostart / teste). Bloqueia até erro ou Ctrl+C.
pub fn run_headless(cfg: Config) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.block_on(async {
        tokio::select! {
            r = run(cfg, Shared::new(None)) => r,
            _ = tokio::signal::ctrl_c() => Ok(()),
        }
    })
}

pub struct Worker {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(cfg: Config, shared: Shared) -> Self {
        let (tx, rx) = oneshot::channel();
        let thread = std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
                Ok(rt) => rt,
                Err(e) => return shared.status(format!("Erro runtime: {e}")),
            };
            rt.block_on(async {
                let res = tokio::select! {
                    r = run(cfg, shared.clone()) => r,
                    _ = rx => Ok(()),
                };
                match res {
                    Ok(()) => shared.status("Parado"),
                    Err(e) => shared.status(format!("Erro: {e:#}")),
                }
            });
            // Drop do runtime cancela tasks e fecha sessão do portal (libera captura).
            rt.shutdown_timeout(std::time::Duration::from_secs(1));
        });
        Self { stop: Some(tx), thread: Some(thread) }
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

async fn run(cfg: Config, shared: Shared) -> anyhow::Result<()> {
    match cfg.role {
        Role::Server => crate::server::run(cfg, shared).await,
        Role::Client => crate::client::run(cfg, shared).await,
    }
}
