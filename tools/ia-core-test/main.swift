import Foundation
// The app's side of a tab's AI against the real core, run by
// tools/ia-core-test.sh: the app's own Daemon.swift, KeepCLI.swift,
// AIUsage.swift and AIChoice.swift asking `keep ia` (target/debug/keep) about
// a daemon of the test's own, whose tab runs a fake Claude and a fake Codex
// (crates/keep-ia/examples/ia_falsa.rs). Every answer the app reads is read
// here as the app reads it. No window, no real login, no real daemon.

// The app types Daemon.swift reaches for, stubbed.
enum Trace { static func log(_ kind: String, _ detail: @autoclosure () -> String) {} }
enum KeepStateFile { static func validateConnection(socket: String, directory: URL) throws {} }
func stateDirectory() -> URL { URL(fileURLWithPath: NSTemporaryDirectory()) }

var falhas = 0
var casos = 0
func confere(_ ok: Bool, _ nome: String, _ detalhe: @autoclosure () -> String = "") {
    casos += 1
    if ok { print("ok   \(nome)") } else { falhas += 1; print("FALHA \(nome) \(detalhe())") }
}

/// O que, no ambiente entregue a um `keep`, sai da casa falsa `casa`: nil
/// quando nada sai. A casa e a pasta de estado são as dela; o Chaveiro é um
/// de mentira; cada serviço tem endereço, e todos nesta máquina. (O mesmo de
/// tools/usage-test/main.swift.)
func furoNoIsolamento(_ e: [String: String], casa: String) -> String? {
    if e["KEEP_IA_HOME"] != casa { return "KEEP_IA_HOME=\(e["KEEP_IA_HOME"] ?? "nada")" }
    guard let estado = e["KEEP_IA_ESTADO"], estado.hasPrefix(casa + "/") else {
        return "KEEP_IA_ESTADO=\(e["KEEP_IA_ESTADO"] ?? "nada")"
    }
    guard let seguranca = e["KEEP_IA_SECURITY"], !seguranca.isEmpty, !seguranca.hasPrefix("/usr/bin/") else {
        return "KEEP_IA_SECURITY=\(e["KEEP_IA_SECURITY"] ?? "nada") (o Chaveiro de verdade)"
    }
    let servicos = [["KEEP_IA_PERFIL_URL", "CLAUDE_KIT_PERFIL_URL"], ["KEEP_IA_CLAUDE_URL", "KEEP_AI_USAGE_CLAUDE_URL"],
                    ["KEEP_IA_CODEX_URL", "KEEP_AI_USAGE_CODEX_URL"], ["KEEP_IA_OPENROUTER_URL"],
                    ["KEEP_IA_OPENROUTER_AUTH_URL"], ["KEEP_IA_TOKEN_URL"]]
    for nomes in servicos {
        let valores = nomes.compactMap { e[$0] }
        if valores.isEmpty || !valores.allSatisfy({ $0.hasPrefix("http://127.0.0.1") }) {
            return "\(nomes[0])=\(valores.joined(separator: ",")) (um serviço de verdade)"
        }
    }
    return nil
}

let ambiente = ProcessInfo.processInfo.environment
guard KeepCLI.madeUpHome(ambiente) != nil, let keepdPid = ambiente["KEEPD_PID"].flatMap(UInt32.init),
      let digita = ambiente["DIGITA"], AIHelper.path != nil
else {
    print("FALHA recuso rodar: falta a casa falsa, o keepd do teste, o digita.py ou o keep (KEEP_IA_BIN)")
    exit(2)
}
// Trocar, entrar e mover na ordem gravam na casa que o núcleo recebe: com o
// isolamento furado, seria a da pessoa. Nada roda antes de conferir.
if let furo = furoNoIsolamento(KeepCLI.environment(ambiente), casa: KeepCLI.madeUpHome(ambiente)!) {
    print("FALHA recuso rodar fora da casa falsa: \(furo)")
    exit(2)
}
let ws = "T"

func tela(_ tab: UInt32) -> String { (try? Daemon.preview(workspace: ws, tab: tab)) ?? "" }
/// The tab's screen shows `text`, its wrapped lines and spaces aside, within
/// `seconds`.
func mostra(_ text: String, em tab: UInt32, _ seconds: Double = 15) -> Bool {
    let squeeze = { (s: String) in s.filter { !$0.isWhitespace } }
    let wanted = squeeze(text)
    let until = Date().addingTimeInterval(seconds)
    while Date() < until {
        if squeeze(tela(tab)).contains(wanted) { return true }
        Thread.sleep(forTimeInterval: 0.1)
    }
    return false
}
func programa(_ tab: UInt32) -> AIProgramKind {
    let found = (try? Daemon.list())?.first { $0.name == ws }?.tabs.first { $0.id == tab }
    return AIProgramKind(command: found?.command ?? "")
}
func escreve(_ tab: UInt32, _ text: String, enter: Bool = true) {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: digita)
    p.arguments = [Daemon.socketPath, ws, String(tab), "160", "40", text] + (enter ? ["--enter"] : [])
    try? p.run()
    p.waitUntilExit()
}
func ms() -> Double { Date().timeIntervalSince1970 * 1000 }

// --- o daemon, como o app o vê
let lista = try? Daemon.list()
let info = Daemon.lastInfo
confere(lista != nil, "daemon: listado pela terceira lista")
confere(info?.pid == keepdPid && info?.startedMs != nil, "daemon: diz quem é (processo e início)", "\(String(describing: info))")

guard let aba = try? Daemon.newTab(in: ws, cwd: KeepCLI.madeUpHome(ambiente)!, cols: 160, rows: 40) else {
    print("FALHA não abri a aba"); exit(1)
}
confere(mostra("$", em: aba), "aba: o shell no prompt", tela(aba))

/// What the core says the tab runs on, read as the app reads it, for this
/// daemon.
func conta(_ tab: UInt32) -> (escolha: String?, roda: String?, rodape: String?, crida: Bool) {
    MainActor.assumeIsolated {
        let loja = AITabAccounts()
        _ = loja.daemon(Daemon.startedAt)
        guard case .success(let dados) = AIHelper.tabs() else { return (nil, nil, nil, false) }
        let crida = loja.adopt(dados, askedAt: ms(), daemonPid: info?.pid, daemonStartedMs: info?.startedMs)
        let kind = programa(tab)
        return (loja.account(workspace: ws, tab: tab, program: kind),
                loja.running(workspace: ws, tab: tab, program: kind),
                loja.confirmedAccount(workspace: ws, tab: tab, program: kind), crida)
    }
}

// --- seguir a ordem numa aba só com o shell: o núcleo sobe o Claude nela
switch AIHelper.switchAccount(workspace: ws, tab: aba, to: AIHelper.followOrder, interrupt: false) {
case .success(let feito): confere(feito.contains("Claude"), "seguir a ordem numa concha: \(feito)")
case .failure(let p): confere(false, "seguir a ordem numa concha", "\(p)")
}
confere(mostra("FALSO claude", em: aba), "o Claude falso subiu na aba", tela(aba))
var agora = conta(aba)
confere(agora.crida, "abas: a resposta do núcleo é crida para este daemon")
confere(agora.escolha == AIHelper.followOrder && agora.roda == "claude:ana",
        "abas: segue a ordem, e roda na ana", "\(agora)")
confere(agora.rodape == "claude:ana", "rodapé: o ponto verde na conta em que roda", "\(agora)")

// --- Claude → Claude noutra conta: a mesma conversa
switch AIHelper.switchAccount(workspace: ws, tab: aba, to: "claude:bia", interrupt: false) {
case .success(let feito): confere(feito == "retomou Claude · bia", "trocar para claude:bia: \(feito)")
case .failure(let p): confere(false, "trocar para claude:bia", "\(p)")
}
confere(mostra("escolha=claude:bia", em: aba), "a aba roda na bia", tela(aba))
agora = conta(aba)
confere(agora.escolha == "claude:bia" && agora.roda == "claude:bia", "abas: fixa na bia", "\(agora)")

// --- trabalhando: só com a pessoa dizendo que pode interromper
escreve(aba, "/trabalhar")
confere(mostra("esc to interrupt", em: aba), "a aba trabalha", tela(aba))
switch AIHelper.switchAccount(workspace: ws, tab: aba, to: "gpt:principal", interrupt: false) {
case .success(let feito): confere(false, "ocupada: devia recusar", feito)
case .failure(let p): confere(p.reason == "ocupada" && !p.detail.isEmpty, "ocupada: o motivo chega ao app (\(p.detail))", "\(p)")
}
switch AIHelper.switchAccount(workspace: ws, tab: aba, to: "gpt:principal", interrupt: true) {
case .success(let feito):
    confere(feito.hasPrefix("abriu conversa nova em GPT · principal"), "Claude → Codex com --interromper: \(feito)")
case .failure(let p): confere(false, "Claude → Codex", "\(p)")
}
confere(mostra("FALSO codex", em: aba), "o Codex falso subiu na aba", tela(aba))
agora = conta(aba)
confere(programa(aba) == .codex && agora.escolha == "gpt:principal" && agora.roda == "gpt:principal",
        "abas: Codex no principal", "\(agora) \(programa(aba))")
confere(AIChoices.programLabel(command: "codex", account: agora.escolha) == "codex", "lateral: Codex no principal diz só codex")

// --- Codex → seguir a ordem: o Claude da frente da fila
switch AIHelper.switchAccount(workspace: ws, tab: aba, to: AIHelper.followOrder, interrupt: false) {
case .success(let feito): confere(feito.contains("Claude · ana"), "Codex → seguir a ordem: \(feito)")
case .failure(let p): confere(false, "Codex → seguir a ordem", "\(p)")
}
confere(mostra("FALSO claude", em: aba), "o Claude voltou", tela(aba))

// --- o consumo, e a ordem seguida sozinha
switch AIHelper.usage(.due) {
case .success(let resposta):
    let ana = resposta.linhas.first { $0.account.alias == "ana" }
    let bia = resposta.linhas.first { $0.account.alias == "bia" }
    confere(ana?.reading?.windows.map(\.label) == ["5h", "7d"] && ana?.isAvailable == false,
            "uso: a ana no limite da semana sai da fila", "\(String(describing: ana))")
    confere(bia?.isAvailable == true && bia?.reading != nil, "uso: a bia medida e disponível", "\(String(describing: bia))")
    switch AIHelper.sync() {
    case .success(let rodada):
        confere(rodada["alvo"] as? String == "claude:bia" && (rodada["alteradas"] as? [Any])?.count == 1,
                "sincronizar: a aba que segue a ordem vai para a bia", "\(rodada)")
    case .failure(let p): confere(false, "sincronizar", "\(p)")
    }
    confere(mostra("escolha=claude:ordem", em: aba), "a aba segue a ordem, agora na bia", tela(aba))
    agora = conta(aba)
    confere(agora.escolha == AIHelper.followOrder && agora.roda == "claude:bia", "abas: segue a ordem, roda na bia", "\(agora)")
    let menu = AIChoices.rows(lines: resposta.linhas, current: agora.escolha, running: agora.roda, program: programa(aba))
    confere(menu.first?.mark == .on && menu.first?.key == AIHelper.followOrder, "menu: ✓ em seguir a ordem, que pede claude:ordem")
    let traco = menu.filter { $0.mark == .mixed }.map(\.label)
    confere(traco == ["Claude · bia"], "menu: o traço na conta em que a aba roda", "\(traco)")
    confere(menu.contains { $0.title.hasPrefix("Claude · ana") && $0.title.hasSuffix("— no limite") },
            "menu: a ana diz que está no limite", "\(menu.map(\.title))")
    switch AIHelper.sync() {
    case .success(let rodada):
        confere((rodada["alteradas"] as? [Any])?.isEmpty == true, "sincronizar: na conta certa, nada mexe", "\(rodada)")
    case .failure(let p): confere(false, "sincronizar de novo", "\(p)")
    }
case .failure(let p):
    confere(false, "uso: a rodada", "\(p)")
}

// --- entrar noutra conta: a aba do login, aberta pelo núcleo
switch AIHelper.signIn(.claude, workspace: ws) {
case .success(let aberta):
    confere(aberta.workspace == ws && aberta.tab != aba, "entrar: a aba nova do login (\(aberta.tab))")
    confere(mostra("ia login claude", em: aberta.tab), "entrar: ela roda o login do keep", tela(aberta.tab))
    try? Daemon.closeTab(aberta.tab, in: ws)
case .failure(let p):
    confere(false, "entrar", "\(p)")
}

// --- a ordem, mexida pelo app
switch AIHelper.moveOrder("gpt:principal", up: true) {
case .success(let ordem):
    confere(ordem == ["claude:ana", "gpt:principal", "claude:bia"], "ordem: o GPT sobe um lugar", "\(ordem)")
case .failure(let p): confere(false, "ordem: mover", "\(p)")
}

print("\(casos - falhas)/\(casos) ok")
exit(falhas == 0 ? 0 : 1)
