## Keep para Windows e macOS

**Windows (x64):** baixe `Keep_<versão>_x64-setup.exe` e rode. Instala só para o seu usuário,
sem administrador, com o daemon (`keepd`), o cliente de linha de comando (`keep`, em
`%LOCALAPPDATA%\Keep\bin`) e o console do Windows Terminal (ConPTY da Microsoft).
O instalador não é assinado: o Windows pode mostrar "O Windows protegeu o computador" —
clique em **Mais informações** e **Executar assim mesmo**.

**macOS (Apple Silicon):** `Keep-macOS-arm64.zip`, assinado ad hoc. Depois de descompactar e
mover para Aplicativos: `xattr -dr com.apple.quarantine /Applications/Keep.app`. No Mac do autor
o app é instalado pelo kit, compilado do ramo `main` e assinado com a identidade local.

Fechar a janela não fecha nada: os terminais continuam no daemon e voltam como estavam.
