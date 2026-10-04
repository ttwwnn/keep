# Keep

Terminal com **workspaces que sobrevivem à janela**, para **macOS** e **Windows**.

Fechar a janela — ou o app inteiro — não fecha nada: os shells continuam rodando num
daemon, e ao reabrir cada aba está onde ficou, com a tela como estava. Um workspace é
um projeto; um workspace tem abas; abas podem ser divididas em painéis.

> Projeto de [ttwwnn](https://github.com/ttwwnn), derivado de
> [luhw-dev/keep](https://github.com/luhw-dev/keep) (licença MIT). Este repositório
> segue o próprio caminho: as mudanças entram aqui, não como PR no projeto original.

## Como funciona

```
┌─ daemon (keepd) ───────────────────────────────┐
│  dono dos terminais; sobrevive a todo cliente  │
│  libghostty-vt guarda a tela de cada aba       │
└────────────────────────────────────────────────┘
          ▲  socket unix (macOS/Linux) · pipe nomeado (Windows)
   ┌──────┴──────────────┬──────────────────────┐
   │ app do macOS        │ app do Windows       │  keep (cliente de
   │ Swift + libghostty  │ Tauri + xterm.js     │  terminal, qualquer SO)
   └─────────────────────┴──────────────────────┘
```

O núcleo é o mesmo nas duas plataformas (Rust, em `crates/`):

| Crate | O que faz |
|---|---|
| `keepd` | o daemon: abre os terminais (PTY no Unix, ConPTY no Windows), guarda a tela de cada aba, atende os clientes |
| `keep-proto` | o protocolo entre daemon e clientes, e o transporte (socket unix ou pipe nomeado) |
| `keep-vt` | ligação segura com o [libghostty-vt](https://libghostty.tip.ghostty.org/) |
| `keep` | cliente de linha de comando: anexa a um workspace de dentro de qualquer terminal |

Cada plataforma tem o seu app nativo por cima (`apps/macos`, `apps/windows`).

## Windows

**Instalar:** baixe `Keep_<versão>_x64-setup.exe` em
[Releases](https://github.com/ttwwnn/keep/releases) e rode. Instala só para o seu usuário
(sem administrador), com o daemon, o cliente `keep` e o console do Windows Terminal
(ConPTY da Microsoft) junto.

- O shell padrão é o PowerShell 7 se estiver instalado, senão o Windows PowerShell; dá
  para trocar nas configurações (PowerShell, Prompt de Comando, Git Bash, WSL).
- Depois de reiniciar o computador, o Keep reabre os workspaces e as abas nas mesmas
  pastas.
- Arrastar arquivos do Explorer para o terminal cola os caminhos.
- Estado do Claude Code e do Codex na aba: trabalhando, esperando você (laranja), em
  segundo plano (azul) ou concluído (cinza).

| Atalho | Ação |
|---|---|
| Ctrl+Shift+T | nova aba |
| Ctrl+Shift+N | novo workspace |
| Ctrl+Tab / Ctrl+Shift+Tab | próxima / anterior aba |
| Ctrl+1 … Ctrl+9 | ir para a aba |
| Ctrl+Shift+P | ir para qualquer workspace ou aba |
| Ctrl+Shift+F | buscar em todas as abas |
| Ctrl+F | buscar na aba |
| Ctrl+Shift+D / Ctrl+Shift+E | dividir à direita / abaixo |
| Ctrl+= / Ctrl+− / Ctrl+0 | zoom |
| F2 | renomear |
| Ctrl+C / Ctrl+V | copiar a seleção / colar |
| Shift+Enter | nova linha no Claude Code |

Fechar aba ou workspace sempre pede confirmação (não há atalho para fechar).

## macOS

O app nativo (`apps/macos`, Swift/AppKit com o libghostty completo: Metal, as fontes e
temas do Ghostty) tem barra lateral de workspaces, abas na barra de título, divisões,
seletor (⌘P), comandos (⌘⇧P), zoom, cores de estado do Claude Code e do Codex, contas de
IA por aba e consumo de IA no rodapé.

Compilar:

```sh
./vendor/fetch.sh                    # libghostty-vt
cargo build --release -p keep -p keepd
cd apps/macos && xcodebuild -project Keep.xcodeproj -scheme Keep -configuration Release \
  -derivedDataPath build build
```

O Xcode não copia `keep` e `keepd` para dentro do app: copie `target/release/keep` e
`target/release/keepd` para `Keep.app/Contents/Resources/` e assine. No Mac do autor isso
é feito pelo instalador do kit (`instalar-keep-app.sh`), que compila do ramo `main`.

## Linux

O daemon e o cliente rodam no Linux (`tools/build-linux.sh` compila a partir de um Mac).
Lá o cliente `keep` é o produto, como o tmux:

```sh
keep              # escolher um workspace, ou digitar um nome para criar
keep projeto      # anexar a "projeto", criando se precisar
keep ls           # listar
keep kill nome    # encerrar um workspace
```

Dentro de um workspace, `Ctrl+\` desanexa e deixa tudo rodando.

## Compilar o app do Windows

Requisitos: Rust (MSVC), Node 22, Zig 0.16.0, Visual Studio Build Tools.

```sh
./vendor/build-vt.sh x86_64-windows-msvc vendor/libghostty-vt-windows-x86_64   # no Git Bash
cargo build --release -p keep -p keepd
mkdir -p apps/windows/src-tauri/resources/bin
cp target/release/keep.exe target/release/keepd.exe apps/windows/src-tauri/resources/bin/
# conpty.dll e OpenConsole.exe: pacote NuGet Microsoft.Windows.Console.ConPTY (ver CI)
cd apps/windows && npm ci && npx tauri build
```

O CI (`.github/workflows/windows-app.yml`) faz exatamente isso, instala o resultado num
Windows de verdade, abre o app e confere o roteiro de ponta a ponta.

## Testes

```sh
cargo test --workspace          # núcleo: macOS, Linux e Windows no CI
cd apps/windows && npm test     # lógica da interface do Windows
```

O libghostty-vt não tem ABI estável: a versão é fixada pelo commit do Ghostty
(`GHOSTTY_COMMIT` no CI) e o `keep-vt` confere os tamanhos das structs C nos testes.

As notas de projeto originais (inglês) estão em [`docs/README-upstream.md`](docs/README-upstream.md),
[`DESIGN.md`](DESIGN.md) e [`PRODUCT.md`](PRODUCT.md).

## Licença

MIT — ver [LICENSE](LICENSE).
