//! Mantém este PC acordado enquanto o usuário usa o outro.
//!
//! Cada lado manda `Msg::Awake` a cada 5s se teve input de usuário nos últimos 15s.
//! Quem recebe "cutuca" o SO:
//! - Windows: SendInput de movimento 0,0 (reseta descanso de tela/bloqueio/suspensão).
//!   O próprio cutucão é ignorado na medição p/ não criar loop entre os PCs.
//! - Linux (GNOME): inibe "ocioso" via portal enquanto chegarem cutucões (+60s).

use std::time::Duration;

pub const REPORT_EVERY: Duration = Duration::from_secs(5);
const ACTIVE_WITHIN_MS: u64 = 15_000;

#[cfg(target_os = "linux")]
pub use linux::Awake;
#[cfg(windows)]
pub use windows::Awake;

#[cfg(target_os = "linux")]
mod linux {
    use super::ACTIVE_WITHIN_MS;
    use ashpd::desktop::inhibit::{InhibitFlags, InhibitOptions, InhibitProxy};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc;

    const HOLD: Duration = Duration::from_secs(60);

    pub struct Awake {
        active: Arc<AtomicBool>,
        poke: mpsc::UnboundedSender<()>,
    }

    impl Awake {
        pub fn start() -> Self {
            let active = Arc::new(AtomicBool::new(false));
            let (tx, rx) = mpsc::unbounded_channel();
            let a = active.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
                match rt {
                    Ok(rt) => rt.block_on(run(a, rx)),
                    Err(e) => log::warn!("keep-awake: {e}"),
                }
            });
            Self { active, poke: tx }
        }

        pub fn user_active(&self) -> bool {
            self.active.load(Ordering::Relaxed)
        }

        pub fn poke(&self) {
            let _ = self.poke.send(());
        }
    }

    async fn idle_ms(conn: &ashpd::zbus::Connection) -> anyhow::Result<u64> {
        let reply = conn
            .call_method(
                Some("org.gnome.Mutter.IdleMonitor"),
                "/org/gnome/Mutter/IdleMonitor/Core",
                Some("org.gnome.Mutter.IdleMonitor"),
                "GetIdletime",
                &(),
            )
            .await?;
        Ok(reply.body().deserialize::<u64>()?)
    }

    async fn run(active: Arc<AtomicBool>, mut pokes: mpsc::UnboundedReceiver<()>) {
        let conn = match ashpd::zbus::Connection::session().await {
            Ok(c) => c,
            Err(e) => return log::warn!("keep-awake: sem D-Bus: {e}"),
        };
        let proxy = InhibitProxy::new().await.ok();
        let mut held: Option<ashpd::desktop::Request<()>> = None;
        let mut until = Instant::now();
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        let mut warned = false;
        loop {
            tokio::select! {
                p = pokes.recv() => {
                    if p.is_none() { break; }
                    until = Instant::now() + HOLD;
                    if held.is_none() {
                        if let Some(proxy) = &proxy {
                            let opts = InhibitOptions::default().set_reason("Em uso pelo outro PC (lan-keyboard)");
                            match proxy.inhibit(None, InhibitFlags::IDLE, opts).await {
                                Ok(r) => { log::debug!("keep-awake: inibindo ocioso"); held = Some(r); }
                                Err(e) => log::warn!("keep-awake: inhibit falhou: {e}"),
                            }
                        }
                    }
                }
                _ = tick.tick() => {
                    match idle_ms(&conn).await {
                        Ok(ms) => active.store(ms < ACTIVE_WITHIN_MS, Ordering::Relaxed),
                        Err(e) if !warned => { warned = true; log::warn!("keep-awake: idle do GNOME indisponível: {e}"); }
                        Err(_) => {}
                    }
                    if Instant::now() > until {
                        if let Some(r) = held.take() {
                            log::debug!("keep-awake: liberando");
                            let _ = r.close().await;
                        }
                    }
                }
            }
        }
        if let Some(r) = held.take() {
            let _ = r.close().await;
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::ACTIVE_WITHIN_MS;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use windows::Win32::System::Power::{ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    const POKE_EVERY: Duration = Duration::from_secs(20);

    pub struct Awake {
        state: Mutex<(Option<Instant>, u32)>, // (último cutucão, tick do último input gerado por nós)
    }

    fn last_input_tick() -> u32 {
        let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        unsafe {
            let _ = GetLastInputInfo(&mut info);
        }
        info.dwTime
    }

    impl Awake {
        pub fn start() -> Self {
            Self { state: Mutex::new((None, 0)) }
        }

        pub fn user_active(&self) -> bool {
            let last = last_input_tick();
            let own = self.state.lock().unwrap().1;
            if last == own {
                return false;
            }
            let now = unsafe { GetTickCount() };
            (now.wrapping_sub(last) as u64) < ACTIVE_WITHIN_MS
        }

        pub fn poke(&self) {
            let mut st = self.state.lock().unwrap();
            if st.0.is_some_and(|t| t.elapsed() < POKE_EVERY) {
                return;
            }
            st.0 = Some(Instant::now());
            unsafe {
                SetThreadExecutionState(ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 },
                    },
                };
                SendInput(&[input], size_of::<INPUT>() as i32);
            }
            st.1 = last_input_tick();
        }
    }
}
