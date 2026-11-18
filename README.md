# lan-keyboard

Um teclado e um mouse para dois PCs na mesma rede (Ubuntu e Windows). Leve o cursor até a borda da tela e ele passa para o outro PC, junto com o teclado. Parecido com o Universal Control do Mac.

## Recursos

- Funciona nos dois sentidos: o servidor (PC com teclado/mouse plugados) pode ser o Ubuntu ou o Windows.
- Troca de tela pela borda, com a posição das telas configurada arrastando na interface.
- Teclado completo, incluindo ABNT2, AltGr, Win/Super e teclas de mídia.
- Ajuste de sensibilidade do mouse e velocidade do scroll no cliente.
- Conexão protegida por PIN: o PIN nunca trafega na rede (SPAKE2) e todo o tráfego é criptografado (ChaCha20-Poly1305). Depois de 5 PINs errados em 1 minuto, o servidor bloqueia novas tentativas por 1 minuto.
- Mantém o outro PC acordado: enquanto você usa um PC, o outro não bloqueia nem entra em descanso de tela.
- Reconecta sozinho se a rede cair.
- `Ctrl+Alt+Pause` devolve o controle ao PC local a qualquer momento.

## Requisitos

| SO | Requisito |
|---|---|
| Ubuntu | GNOME 46+ em Wayland (Ubuntu 24.04+). A captura usa o portal InputCapture do GNOME. |
| Windows | Windows 10 ou 11. |

## Download

Baixe a versão mais recente em [Releases](https://github.com/codesgm/lan-keyboard/releases/latest):

- **Windows:** [lan-keyboard-windows-x64.exe](https://github.com/codesgm/lan-keyboard/releases/latest/download/lan-keyboard-windows-x64.exe)
- **Ubuntu:** [lan-keyboard-linux-x64](https://github.com/codesgm/lan-keyboard/releases/latest/download/lan-keyboard-linux-x64)

## Instalação

### Windows

Baixe o `.exe` (link acima) e execute. Não precisa instalar nada. O Windows SmartScreen pode avisar que o app não é reconhecido, porque o executável não é assinado: clique em **Mais informações** → **Executar assim mesmo**.

Na primeira execução, o Firewall do Windows pergunta se libera o programa. Clique em **Permitir** para rede privada.

### Ubuntu

1. Baixe o binário (link acima) e torne-o executável:
   ```bash
   chmod +x ~/Downloads/lan-keyboard-linux-x64
   ```
2. Dê acesso ao `/dev/uinput`. É necessário para o Ubuntu funcionar como cliente:
   ```bash
   echo 'KERNEL=="uinput", GROUP="input", MODE="0660", TAG+="uaccess", OPTIONS+="static_node=uinput"' \
     | sudo tee /etc/udev/rules.d/99-lan-keyboard.rules
   sudo modprobe uinput && echo uinput | sudo tee /etc/modules-load.d/uinput.conf
   sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=uinput
   ```
3. Se o firewall (ufw) estiver ativo e o Ubuntu for o servidor, libere a porta:
   ```bash
   sudo ufw allow 24800/tcp
   ```
4. Rode:
   ```bash
   ~/Downloads/lan-keyboard-linux-x64
   ```

## Uso

1. **No servidor** (o PC com teclado e mouse plugados):
   - Aba **Conexão**: escolha **Servidor** e anote o **PIN**.
   - Aba **Layout**: arraste a tela remota até a posição real dela em relação a esta tela.
   - Clique em **Iniciar**. No Ubuntu, o GNOME pede permissão para capturar o input: clique em **Permitir**.
2. **No cliente:**
   - Aba **Conexão**: escolha **Cliente**, informe o IP do servidor e o mesmo PIN.
   - Clique em **Iniciar**.
3. Leve o cursor até a borda configurada. O mouse e o teclado passam para o outro PC. Para voltar, leve o cursor de volta pela mesma borda.

Para descobrir o IP: no Ubuntu, rode `hostname -I`. No Windows, rode `ipconfig`.

As configurações ficam em `~/.config/lan-keyboard/config.toml` (Ubuntu) e `%APPDATA%\lan-keyboard\config.toml` (Windows). Mudanças valem ao reiniciar com **Parar** e **Iniciar**.

### Aba Ajustes

- **Sensibilidade do mouse / velocidade do scroll**: valem no cliente, de 0,25x a 3x.
- **Manter o outro PC acordado**: cada PC avisa o outro quando está em uso. Quem recebe o aviso não bloqueia nem apaga a tela. Quando você para de usar os dois, ambos entram em descanso normalmente.

## Modo sem interface

```bash
lan-keyboard --headless
```

Usa a config salva. Útil para iniciar junto com o sistema.

## Limitações

- A tela de bloqueio, a tela de login e os pedidos de UAC do Windows não aceitam input vindo do programa.
- Janelas abertas como administrador no Windows só recebem input se o `lan-keyboard` também rodar como administrador.
- Com o Windows como servidor, o cursor fica parado no centro da tela enquanto você controla o outro PC.
- O GNOME pede permissão de captura toda vez que o servidor inicia no Ubuntu.
- O clipboard ainda não é compartilhado.

## Problemas comuns

| Sintoma | Solução |
|---|---|
| `sem acesso a /dev/uinput` | Faça a etapa 2 da instalação do Ubuntu. |
| `PIN incorreto` | Copie o PIN do servidor para o cliente, clique em **Salvar** e depois em **Iniciar**. |
| Cliente fica em "Conectando…" | Confira o IP e a porta. Confira também o firewall do servidor (ufw no Ubuntu; no Windows, permita o programa). |
| Mouse ou teclado travado no outro PC | Aperte `Ctrl+Alt+Pause`. |
| Teclas não chegam | Rode com `RUST_LOG=lan_keyboard=debug` e veja o log. No Windows, o log fica em `%APPDATA%\lan-keyboard\log.txt`. Atenção: com `debug` ligado, o log grava as teclas digitadas. Apague o arquivo depois. |

## Desenvolvimento

Para compilar no Ubuntu:

```bash
sudo apt install -y build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libudev-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo run --release
```

Para publicar uma versão nova: `git tag v0.2.0 && git push origin v0.2.0`. O GitHub Actions compila e anexa os binários na release.

```bash
cargo test
cargo build --release --target x86_64-pc-windows-gnu   # .exe a partir do Linux (precisa do pacote mingw-w64)
```

| Módulo | Função |
|---|---|
| `server.rs` / `client.rs` | Rede, troca de tela e heartbeat |
| `secure.rs` | Handshake com PIN e criptografia |
| `capture_linux.rs` / `capture_windows.rs` | Captura (portal InputCapture / hooks de baixo nível) |
| `inject_linux.rs` / `inject_windows.rs` | Injeção (uinput / SendInput) |
| `keymap.rs` | Tradução de teclas: evdev ↔ USB HID ↔ scancode do Windows |
| `awake.rs` | Manter o outro PC acordado |
| `gui/` | Interface (egui) |

O plano completo está em [PLANNING.md](PLANNING.md).
