# lan-keyboard — Planejamento

Compartilhar 1 teclado + 1 mouse entre Ubuntu (Wayland) e Windows na LAN. Estilo Universal Control do Mac.

## Decisões
- Linguagem: **Rust**, um único binário `lan-keyboard` p/ os dois SOs.
- Papel (servidor/cliente) escolhido na GUI. Servidor = PC com teclado/mouse físico.
- GUI: **egui/eframe**.
- Linux: **evdev** (captura, grab exclusivo) + **uinput** (injeção). Funciona em Wayland.
- Windows: **SetWindowsHookEx** (WH_KEYBOARD_LL / WH_MOUSE_LL) captura + **SendInput** injeção.
- Rede: TCP + `TCP_NODELAY`. Depois TLS + pareamento por código.
- Teclas: código neutro = **USB HID usage**. Tradução evdev keycode <-> HID <-> Windows scancode.
- Mouse: movimento **relativo** (dx, dy). Sensibilidade aplicada no cliente.
- Clipboard: fora da v1.
- Atalho emergência: `Ctrl+Alt+Pause` -> devolve controle ao PC local, solta grab.
- Build Windows: cross-compile `x86_64-pc-windows-gnu` (mingw) no Ubuntu.

## Ambiente (pronto)
- [x] rustup + cargo, target `x86_64-pc-windows-gnu`
- [x] mingw-w64, libs GUI, libevdev, libudev
- [x] regra udev `/dev/uinput` grupo `input`
- [ ] usuário no grupo `input` ativo (logout/login)

## Estrutura
```
lan-keyboard/
  Cargo.toml
  src/
    main.rs            # inicia GUI
    config.rs          # load/save TOML (dirs::config_dir)
    protocol.rs        # mensagens + encode/decode (bincode, len-prefixed)
    keymap.rs          # HID <-> evdev <-> win scancode
    net/
      server.rs        # aceita cliente, envia eventos
      client.rs        # conecta, recebe, injeta
    input/
      mod.rs           # traits Capture / Inject
      linux.rs         # evdev + uinput
      windows.rs       # hooks + SendInput
    switch.rs          # lógica de borda / foco local vs remoto
    gui/
      mod.rs           # app eframe, abas
      layout.rs        # canvas arrastar telas
      settings.rs      # IP, porta, sensibilidade, atalho
```

## Traits
```rust
trait Capture { fn start(&mut self, tx: Sender<Event>); fn grab(&mut self, on: bool); }
trait Inject  { fn mouse_move(&mut self, dx: i32, dy: i32); fn button(&mut self, b: Button, down: bool);
                fn scroll(&mut self, dx: i32, dy: i32); fn key(&mut self, hid: u16, down: bool); }
```

## Protocolo
| Msg | Campos |
|---|---|
| Hello | versão, nome host, resolução tela |
| MouseMove | dx, dy |
| MouseButton | botão, down |
| Scroll | dx, dy |
| Key | hid, down |
| Enter | borda, posição relativa 0..1 |
| Leave | — |
| ReleaseAll | — (solta teclas presas ao trocar foco) |
| Heartbeat | — |

## Troca de tela (switch.rs)
- Servidor rastreia cursor virtual.
- Cursor cruza borda configurada -> envia `Enter`, ativa grab, eventos vão p/ cliente.
- Cliente: cursor cruza borda oposta -> envia `Leave` -> servidor solta grab, posiciona cursor local.
- Ao trocar: `ReleaseAll` evita tecla presa.

## GUI
- **Aba Layout**: canvas c/ retângulo da tela local fixo + tela remota arrastável. Snap em esquerda/direita/cima/baixo. Proporção pela resolução do Hello.
- **Aba Conexão**: modo servidor/cliente, IP, porta (default 24800), botão iniciar/parar, status.
- **Aba Ajustes**: sensibilidade mouse cliente (slider 0.25x–3x), velocidade scroll, atalho emergência, iniciar com sistema.
- Config salva em TOML.

## Fases
1. ✅ **Esqueleto**: Cargo, config, protocolo, GUI vazia com abas. Compila Linux + Windows.
2. **Linux input**: captura evdev + injeção uinput. Teste local (loopback).
3. **Rede**: servidor/cliente TCP, Hello, MouseMove/Key. Teste Linux->Linux loopback.
4. **Windows input**: hooks + SendInput. Teste Linux<->Windows.
5. **Keymap** completo (ABNT2, AltGr, Super/Win, mídia).
6. **Switch por borda** + layout GUI funcional.
7. **Sensibilidade** + scroll + ajustes.
8. **Segurança**: TLS (rustls) + pareamento código 6 dígitos + pinning.
9. **Polimento**: tray, autostart (systemd user unit / Task Scheduler), reconexão, logs.
10. **Clipboard** (opcional, v2).

## Riscos
- Grab evdev bugado -> PC sem input. Mitigação: atalho emergência + timeout heartbeat solta grab.
- Windows: janelas admin/UAC ignoram SendInput se app não elevado.
- Tela de login/bloqueio não recebe input injetado.
- Wayland: sem posição absoluta do cursor -> rastrear virtual a partir de dx/dy.

## Teste
- Linux: testo aqui.
- Windows: usuário roda `.exe`, retorna erros.
