//! Captura no Linux (Wayland/GNOME) via portal InputCapture + libei.

use crate::config::Side;
use crate::keymap;
use crate::protocol::{Button, Msg};
use crate::server::{CapCmd, CapEvent, CaptureHandle, Desktop, Hotkey, KeyKind};
use anyhow::Context as _;
use ashpd::desktop::Session;
use ashpd::desktop::input_capture::{
    Barrier, BarrierID, BarrierPosition, Capabilities, CreateSessionOptions, InputCapture,
    ReleaseOptions,
};
use futures_util::StreamExt;
use reis::ei;
use reis::event::{DeviceCapability, EiEvent};
use std::os::unix::net::UnixStream;
use tokio::sync::mpsc;

/// Streams do ashpd/reis não são Send: tudo roda numa thread própria.
pub async fn start(side: Side) -> anyhow::Result<CaptureHandle> {
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<anyhow::Result<Desktop>>();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return drop(ready_tx.send(Err(e.into()))),
        };
        rt.block_on(async move {
            let mut ready = Some(ready_tx);
            if let Err(e) = setup_and_run(side, &ev_tx, cmd_rx, &mut ready).await {
                match ready.take() {
                    Some(r) => drop(r.send(Err(e))),
                    None => drop(ev_tx.send(CapEvent::Failed(format!("{e:#}")))),
                }
            }
        });
    });
    let desktop = ready_rx.await.map_err(|_| anyhow::anyhow!("thread de captura morreu"))??;
    Ok(CaptureHandle { desktop, events: ev_rx, cmds: cmd_tx })
}

async fn setup_and_run(
    side: Side,
    tx: &mpsc::UnboundedSender<CapEvent>,
    cmd_rx: mpsc::UnboundedReceiver<CapCmd>,
    ready: &mut Option<tokio::sync::oneshot::Sender<anyhow::Result<Desktop>>>,
) -> anyhow::Result<()> {
    let ic = InputCapture::new().await.context("portal InputCapture indisponível")?;
    let (session, _caps) = ic
        .create_session(
            None,
            CreateSessionOptions::default()
                .set_capabilities(Capabilities::KEYBOARD | Capabilities::POINTER),
        )
        .await
        .context("permissão de captura negada")?;

    let fd = ic.connect_to_eis(&session, Default::default()).await?;
    let stream = UnixStream::from(fd);
    stream.set_nonblocking(true)?;
    let context = ei::Context::new(stream)?;
    let (_conn, ei_events) = context
        .handshake_tokio("lan-keyboard", ei::handshake::ContextType::Receiver)
        .await
        .context("handshake libei falhou")?;

    let zones = ic.zones(&session, Default::default()).await?.response()?;
    let regions: Vec<_> = zones
        .regions()
        .iter()
        .map(|r| (r.x_offset(), r.y_offset(), r.width() as i32, r.height() as i32))
        .collect();
    let desktop = Desktop::bounding_box(regions.iter().copied()).context("nenhum monitor")?;
    let barriers: Vec<Barrier> = regions
        .iter()
        .enumerate()
        .map(|(n, &(x, y, w, h))| {
            let pos = match side {
                Side::Left => BarrierPosition::new(x, y, x, y + h - 1),
                Side::Right => BarrierPosition::new(x + w, y, x + w, y + h - 1),
                Side::Top => BarrierPosition::new(x, y, x + w - 1, y),
                Side::Bottom => BarrierPosition::new(x, y + h, x + w - 1, y + h),
            };
            Barrier::new(BarrierID::new(n as u32 + 1).unwrap(), pos)
        })
        .collect();
    log::info!("zonas {regions:?}, barreiras {:?}", barriers.iter().map(|b| b.position().as_tuple()).collect::<Vec<_>>());
    let resp = ic
        .set_pointer_barriers(&session, &barriers, zones.zone_set(), Default::default())
        .await?
        .response()?;
    let failed = resp.failed_barriers();
    anyhow::ensure!(failed.len() < barriers.len(), "GNOME recusou todas as barreiras na borda {side:?}");
    if !failed.is_empty() {
        log::info!("barreiras recusadas (entre monitores): {failed:?}");
    }

    let activated = ic.receive_activated().await?;
    let deactivated = ic.receive_deactivated().await?;
    ic.enable(&session, Default::default()).await?;

    if let Some(r) = ready.take() {
        let _ = r.send(Ok(desktop));
    }
    run(side, desktop, &ic, &session, context, ei_events, activated, deactivated, tx, cmd_rx).await
}

#[allow(clippy::too_many_arguments)]
async fn run(
    side: Side,
    desktop: Desktop,
    ic: &InputCapture,
    session: &Session<InputCapture>,
    context: ei::Context,
    mut ei_events: reis::tokio::EiConvertEventStream,
    activated: impl futures_util::Stream<Item = ashpd::desktop::input_capture::Activated>,
    deactivated: impl futures_util::Stream<Item = ashpd::desktop::input_capture::Deactivated>,
    tx: &mpsc::UnboundedSender<CapEvent>,
    mut cmds: mpsc::UnboundedReceiver<CapCmd>,
) -> anyhow::Result<()> {
    futures_util::pin_mut!(activated, deactivated);
    // (activation_id, along na entrada)
    let mut active: Option<(Option<u32>, f32)> = None;
    let mut hotkey = Hotkey::default();
    let mut scroll_px = (0.0f32, 0.0f32);
    let mut scroll_discrete = false;

    let release = async |act_id: Option<u32>, along: f32| {
        let opts = ReleaseOptions::default()
            .set_activation_id(act_id)
            .set_cursor_position(Some(desktop.return_cursor(side, along)));
        if let Err(e) = ic.release(session, opts).await {
            log::warn!("release falhou: {e}");
        }
    };

    loop {
        tokio::select! {
            Some(act) = activated.next() => {
                let (x, y) = act.cursor_position().unwrap_or((0.0, 0.0));
                let along = desktop.along_edge(side, x, y);
                active = Some((act.activation_id(), along));
                hotkey.reset();
                let _ = tx.send(CapEvent::Activated { along });
            }
            Some(_) = deactivated.next() => {
                if active.take().is_some() {
                    let _ = tx.send(CapEvent::Released);
                }
            }
            cmd = cmds.recv() => match cmd {
                Some(CapCmd::Release { along }) => {
                    if let Some((id, _)) = active.take() {
                        release(id, along).await;
                    }
                }
                None => return Ok(()),
            },
            ev = ei_events.next() => {
                let ev = match ev {
                    Some(Ok(ev)) => ev,
                    Some(Err(e)) => anyhow::bail!("libei: {e}"),
                    None => anyhow::bail!("libei desconectou"),
                };
                if let EiEvent::SeatAdded(s) = &ev {
                    s.seat.bind_capabilities(
                        DeviceCapability::Pointer
                            | DeviceCapability::PointerAbsolute
                            | DeviceCapability::Keyboard
                            | DeviceCapability::Scroll
                            | DeviceCapability::Button,
                    );
                    let _ = context.flush();
                    continue;
                }
                let Some((id, entry)) = active else { continue };
                if let EiEvent::KeyboardKey(k) = &ev {
                    let kind = match k.key {
                        29 | 97 => KeyKind::Ctrl,
                        56 | 100 => KeyKind::Alt,
                        119 => KeyKind::Pause,
                        _ => KeyKind::Other,
                    };
                    if hotkey.on_key(kind, k.state == ei::keyboard::KeyState::Press) {
                        active = None;
                        release(id, entry).await;
                        let _ = tx.send(CapEvent::Released);
                        continue;
                    }
                }
                let msg = match ev {
                    EiEvent::PointerMotion(m) => Some(Msg::MouseMove { dx: m.dx, dy: m.dy }),
                    EiEvent::Button(b) => map_button(b.button).map(|button| Msg::MouseButton {
                        button,
                        down: b.state == ei::button::ButtonState::Press,
                    }),
                    EiEvent::KeyboardKey(k) => keymap::evdev_to_hid(k.key).map(|hid| Msg::Key {
                        hid,
                        down: k.state == ei::keyboard::KeyState::Press,
                    }),
                    EiEvent::ScrollDiscrete(s) => {
                        scroll_discrete = true;
                        Some(Msg::Scroll { dx: s.discrete_dx, dy: s.discrete_dy })
                    }
                    EiEvent::ScrollDelta(s) => {
                        scroll_px.0 += s.dx;
                        scroll_px.1 += s.dy;
                        None
                    }
                    EiEvent::Frame(_) => {
                        let (dx, dy) = std::mem::take(&mut scroll_px);
                        let discrete = std::mem::take(&mut scroll_discrete);
                        // Scroll suave (touchpad): ~15px = 1 clique = 120 unidades.
                        (!discrete && (dx != 0.0 || dy != 0.0))
                            .then(|| Msg::Scroll { dx: (dx * 8.0) as i32, dy: (dy * 8.0) as i32 })
                    }
                    _ => None,
                };
                if let Some(msg) = msg {
                    let _ = tx.send(CapEvent::Input(msg));
                }
            }
        }
    }
}

fn map_button(code: u32) -> Option<Button> {
    Some(match code {
        0x110 => Button::Left,
        0x111 => Button::Right,
        0x112 => Button::Middle,
        0x113 => Button::Back,
        0x114 => Button::Forward,
        _ => return None,
    })
}
