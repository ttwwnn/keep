import Foundation
// The model test of AIUsage.swift (what the app holds of the core's usage
// answer), AIChoice.swift (which account each tab is on, and the menu that
// changes it) and Daemon/KeepCLI.swift (asking the `keep` inside the app),
// run by tools/usage-test.sh. Foundation only: no app, no real login, no
// real kit, no network beyond a stand-in on this machine.

// The two app types KeepCLI.swift reaches for, stubbed.
enum Trace { static func log(_ kind: String, _ detail: @autoclosure () -> String) {} }
enum Daemon { static var socketPath: String { "/tmp/sock-de-teste-uso" } }

var falhas = 0
var casos = 0
func confere(_ ok: Bool, _ nome: String, _ detalhe: @autoclosure () -> String = "") {
    casos += 1
    if ok { print("ok   \(nome)") } else { falhas += 1; print("FALHA \(nome) \(detalhe())") }
}

// Runs as an app that reads a made-up home (tools/usage-test.sh): everything
// handed to a `keep` here is pointed at it. Refused outright otherwise.
let ambiente = ProcessInfo.processInfo.environment
guard let casaFalsa = KeepCLI.madeUpHome(ambiente), casaFalsa != NSHomeDirectory() else {
    print("FALHA recuso rodar: KEEP_AI_USAGE_HOME tem de ser uma casa falsa")
    exit(2)
}
let fm = FileManager.default
let casa = URL(fileURLWithPath: casaFalsa)
// Each run from an empty home: the sabotage runs this again on the same one.
for resto in (try? fm.contentsOfDirectory(at: casa, includingPropertiesForKeys: nil)) ?? [] {
    try? fm.removeItem(at: resto)
}

// --- a resposta do núcleo, como o `keep ia uso --json` a imprime
let respostaDoNucleo = """
{"ok":true,"versao":1,"medidoEm":1791084861.4,"gerenteExterno":true,
 "ordem":["claude:reserva","gpt:principal","claude:principal"],
 "linhas":[
  {"account":{"engine":"claude","key":"4f35c383-uuid","alias":"reserva","aliases":["reserva"],
              "email":"ds@x.com","plan":"Max 20x","isActive":true,"isPreferred":true,"warning":null,
              "orderKey":"claude:reserva","hasToken":true,"roda":true,
              "armazens":[{"tipo":"cofre","arquivo":"/x"},{"tipo":"global"}]},
   "reading":{"windows":[{"label":"5h","title":"Sessão (5h)","percent":82.0,"resetsAt":1791086400.0},
                         {"label":"Fable","title":"Semanal — Fable","percent":0.0,"resetsAt":null}],
              "limitReached":false},
   "measuredAt":1791084861.45,"problem":null},
  {"account":{"engine":"codex","key":"acct","alias":"principal","aliases":["principal"],"email":null,
              "plan":"Pro","isActive":true,"isPreferred":false,"warning":null,"orderKey":"gpt:principal",
              "hasToken":true,"roda":true,"armazens":[]},
   "reading":null,"measuredAt":null,"problem":"consultas demais (HTTP 429); tenta de novo hoje às 01:50"},
  {"account":{"engine":"claude","key":"d32da84e","alias":"principal","aliases":["principal","assinaturas"],
              "email":"a@y.com","plan":null,"isActive":false,"isPreferred":false,
              "warning":"login recusado: entre de novo com /login","orderKey":"claude:principal",
              "hasToken":false,"roda":false,"armazens":[]},
   "reading":{"windows":[{"label":"7d","title":"Semanal (7 dias)","percent":100,"resetsAt":1791194400}],
              "limitReached":true},
   "measuredAt":1791084000,"problem":null}]}
""".data(using: .utf8)!
let resposta = UsageAnswer.decode(respostaDoNucleo)
confere(resposta?.linhas.count == 3, "núcleo: três linhas lidas", "\(String(describing: resposta))")
confere(resposta?.linhas.map(\.account.order) == ["claude:reserva", "gpt:principal", "claude:principal"],
        "núcleo: na ordem em que ele as deu")
confere(resposta?.ordem == ["claude:reserva", "gpt:principal", "claude:principal"] && resposta?.gerenteExterno == true,
        "núcleo: a ordem e o gerente externo")
let primeira = resposta?.linhas.first
confere(primeira?.reading?.windows.first?.resetsAt == Date(timeIntervalSince1970: 1791086400),
        "núcleo: datas em segundos desde 1970", "\(String(describing: primeira?.reading?.windows.first?.resetsAt?.timeIntervalSince1970))")
confere(primeira?.measuredAt == Date(timeIntervalSince1970: 1791084861.45), "núcleo: medido em, em segundos desde 1970")
confere(primeira?.reading?.windows.last?.resetsAt == nil && primeira?.reading?.windows.last?.percent == 0,
        "núcleo: janela a 0% e sem reinício")
confere(primeira?.account.hasToken == true && primeira?.account.isActive == true && primeira?.account.plan == "Max 20x",
        "núcleo: o resumo da conta, sem os campos que o app não usa")
let segunda = resposta?.linhas[1]
confere(segunda?.reading == nil && segunda?.problem?.hasPrefix("consultas demais") == true && segunda?.account.email == nil,
        "núcleo: sem leitura, com o problema dito")
let terceira = resposta?.linhas[2]
confere(terceira?.account.keys == ["claude:principal", "claude:assinaturas"] && terceira?.isAvailable == false,
        "núcleo: apelidos viram chaves; aviso e limite tiram da fila")
confere(UsageAnswer.decode("{\"ok\":true}".data(using: .utf8)!) == nil, "núcleo: resposta sem linhas não é uma resposta")
confere(UsageAnswer.decode("lixo".data(using: .utf8)!) == nil, "núcleo: lixo não é uma resposta")
// O rodapé guarda as linhas como sempre guardou: o script de prioridade do
// kit lê `measuredAt` em segundos desde 2001.
if let linhas = resposta?.linhas, let guardado = try? JSONEncoder().encode(linhas),
   let lido = try? JSONSerialization.jsonObject(with: guardado) as? [[String: Any]] {
    let medido = lido.first?["measuredAt"] as? Double
    confere(medido == Date(timeIntervalSince1970: 1791084861.45).timeIntervalSinceReferenceDate,
            "guardado: no formato do Foundation, que o kit lê", "\(String(describing: medido))")
    confere((try? JSONDecoder().decode([AccountUsage].self, from: guardado)) == linhas, "guardado: volta igual")
} else { confere(false, "guardado: codifica") }

// --- o crédito do Jev, como o núcleo o escreve
let respostaDoJev = """
{"ok":true,"versao":1,"medidoEm":1791084861.5,
 "jev":{"credit":{"total":10,"used":0.0246,"keyLimit":5,"keyUsed":0.0042,"usedToday":0.0018,"usedThisMonth":0.0042},
        "measuredAt":1791084861.45,"problem":null}}
""".data(using: .utf8)!
let jev = JevAnswer.decode(respostaDoJev)?.jev
confere(jev?.credit?.total == 10 && jev?.credit?.keyLimit == 5 && jev?.credit?.usedToday == 0.0018,
        "jev: o crédito lido como o núcleo o escreve", "\(String(describing: jev))")
confere(jev?.measuredAt == Date(timeIntervalSince1970: 1791084861.45), "jev: medido em, em segundos desde 1970")
confere(abs((jev?.credit?.accountLeft ?? 0) - 9.9754) < 1e-9 && abs((jev?.credit?.keyLeft ?? 0) - 4.9958) < 1e-9,
        "jev: o saldo da conta e o da chave")
confere(abs((jev?.credit?.available ?? 0) - 4.9958) < 1e-9, "jev: o que ainda pode gastar é o menor dos dois")
confere(jev?.credit?.accountPercent ?? 0 < 1 && abs((jev?.credit?.keyPercent ?? 0) - 0.084) < 1e-9,
        "jev: o gasto em percentual, para a barra", "\(String(describing: jev?.credit?.keyPercent))")
let chaveSemTeto = JevCredit(total: 10, used: 0.02, keyLimit: nil, keyUsed: 2.5, usedToday: nil, usedThisMonth: nil)
confere(chaveSemTeto.keyPercent == nil && chaveSemTeto.keySharePercent == 25 && abs(chaveSemTeto.available - 9.98) < 1e-9,
        "jev: chave sem teto gasta do crédito todo; a barra dela é a parte do crédito que ela gastou",
        "\(chaveSemTeto.keySharePercent)")
let semTeto = JevCredit(total: 10, used: 12, keyLimit: nil, keyUsed: nil, usedToday: nil, usedThisMonth: nil)
confere(semTeto.available == 0 && semTeto.keyPercent == nil && semTeto.accountPercent == 100,
        "jev: gasto além do comprado não dá saldo negativo nem barra além do fim")
confere(UsageText.dollars(9.9754) == "US$ 9,98" && UsageText.dollars(0) == "US$ 0,00"
        && UsageText.dollars(0.004) == "< US$ 0,01" && UsageText.dollars(-1) == "US$ 0,00",
        "jev: dólar com vírgula, duas casas, e o que arredonda a nada dito como tal",
        "\(UsageText.dollars(9.9754)) \(UsageText.dollars(0.004))")
confere(JevAnswer.decode("{\"ok\":true,\"jev\":null}".data(using: .utf8)!)?.jev == nil
        && JevAnswer.decode("{\"ok\":true,\"jev\":null}".data(using: .utf8)!) != nil,
        "jev: sem chave é uma resposta, com jev nulo")
confere(JevAnswer.decode("lixo".data(using: .utf8)!) == nil, "jev: lixo não é uma resposta")
if let jev, let guardado = try? JSONEncoder().encode(jev) {
    confere((try? JSONDecoder().decode(JevLine.self, from: guardado)) == jev, "jev: guardado volta igual")
} else { confere(false, "jev: codifica") }

// --- a assinatura dos arquivos (o vigia de 2 s)
let cofre = casa.appendingPathComponent(".claude/contas")
try! fm.createDirectory(at: cofre.appendingPathComponent("fixas"), withIntermediateDirectories: true)
let assinatura0 = AIAccounts.signature(home: casa)
confere(assinatura0 == AIAccounts.signature(home: casa), "assinatura: igual quando nada muda")
func muda(_ nome: String, _ faz: () -> Void) {
    let antes = AIAccounts.signature(home: casa)
    Thread.sleep(forTimeInterval: 0.01)
    faz()
    confere(AIAccounts.signature(home: casa) != antes, "assinatura: muda com \(nome)")
}
muda("a .ordem") { try! "gpt:principal\n".write(to: cofre.appendingPathComponent(".ordem"), atomically: true, encoding: .utf8) }
muda("um slot do cofre do kit") { try! "{}".write(to: cofre.appendingPathComponent("reserva.json"), atomically: true, encoding: .utf8) }
muda("uma pasta de login do Keep nova") { try! fm.createDirectory(at: cofre.appendingPathComponent("fixas/k-0001"), withIntermediateDirectories: true) }
muda("o keep.json dela") { try! "{}".write(to: cofre.appendingPathComponent("fixas/k-0001/keep.json"), atomically: true, encoding: .utf8) }
muda("o conta.json de uma pasta do kit") {
    try! fm.createDirectory(at: cofre.appendingPathComponent("fixas/4f35c383"), withIntermediateDirectories: true)
    try! "{}".write(to: cofre.appendingPathComponent("fixas/4f35c383/conta.json"), atomically: true, encoding: .utf8)
}
muda("o login do Codex") {
    try! fm.createDirectory(at: casa.appendingPathComponent(".codex"), withIntermediateDirectories: true)
    try! "{}".write(to: casa.appendingPathComponent(".codex/auth.json"), atomically: true, encoding: .utf8)
}
muda("a leitura do Jev, gravada pelo núcleo") {
    try! fm.createDirectory(at: casa.appendingPathComponent(".keep-ia-estado"), withIntermediateDirectories: true)
    try! "{}".write(to: casa.appendingPathComponent(".keep-ia-estado/jev.json"), atomically: true, encoding: .utf8)
}
muda("uma conta GPT nova") {
    try! fm.createDirectory(at: casa.appendingPathComponent(".codex-contas/nova"), withIntermediateDirectories: true)
    try! "{}".write(to: casa.appendingPathComponent(".codex-contas/nova/auth.json"), atomically: true, encoding: .utf8)
}
let antesDoOculto = AIAccounts.signature(home: casa)
try! "{}".write(to: cofre.appendingPathComponent(".oculto.json"), atomically: true, encoding: .utf8)
try! fm.createDirectory(at: cofre.appendingPathComponent("fixas/.tmp"), withIntermediateDirectories: true)
confere(AIAccounts.signature(home: casa) == antesDoOculto, "assinatura: arquivo e pasta ocultos não contam")

// --- texto
let t0 = Date(timeIntervalSince1970: 1_790_000_000)
confere(UsageText.until(t0.addingTimeInterval(41 * 60), now: t0) == "41min", "falta 41min")
confere(UsageText.until(t0.addingTimeInterval(4 * 3600 + 10 * 60), now: t0) == "4h10", "falta 4h10")
confere(UsageText.until(t0.addingTimeInterval(3 * 3600), now: t0) == "3h", "falta 3h")
confere(UsageText.until(t0.addingTimeInterval(37 * 3600 + 5), now: t0) == "1d13h", "falta 1d13h")
confere(UsageText.until(t0.addingTimeInterval(-5), now: t0) == "agora", "reset passado: agora")
confere(UsageText.until(nil, now: t0) == nil, "sem reset: nada")
confere(UsageText.percent(16.6) == "17%" && UsageText.percent(-3) == "0%", "percentual arredondado e sem negativo")
confere(UsageText.level(74.9) == .normal && UsageText.level(75) == .attention && UsageText.level(89.9) == .attention && UsageText.level(90) == .critical, "faixas 75/90")
var cal = Calendar(identifier: .gregorian); cal.timeZone = TimeZone(identifier: "America/Sao_Paulo")!
let meioDia = cal.date(from: DateComponents(year: 2026, month: 9, day: 26, hour: 12))!
confere(UsageText.moment(cal.date(byAdding: .hour, value: 3, to: meioDia)!, now: meioDia, calendar: cal) == "hoje às 15:00", "momento: hoje")
confere(UsageText.moment(cal.date(byAdding: .hour, value: 19, to: meioDia)!, now: meioDia, calendar: cal) == "amanhã às 07:00", "momento: amanhã")
confere(UsageText.moment(cal.date(byAdding: .day, value: 2, to: meioDia)!, now: meioDia, calendar: cal) == "em 28/09 às 12:00", "momento: data")
confere(UsageText.ago(t0.addingTimeInterval(-30), now: t0) == "agora" && UsageText.ago(t0.addingTimeInterval(-23 * 60), now: t0) == "há 23 min", "há quanto tempo")

// --- disponibilidade
func linha(_ conta: AIAccountSummary, _ janelas: [(String, Double)] = [], limite: Bool = false, lida: Bool = true) -> AccountUsage {
    AccountUsage(account: conta,
                 reading: lida ? UsageReading(windows: janelas.map { UsageWindow(label: $0.0, title: $0.0, percent: $0.1, resetsAt: nil) }, limitReached: limite) : nil)
}
func conta(_ motor: AIEngine, _ apelido: String, ativa: Bool = false, aviso: String? = nil, apelidos: [String]? = nil, email: String? = nil) -> AIAccountSummary {
    AIAccountSummary(engine: motor, key: "\(motor.orderPrefix)-\(apelido)", alias: apelido, aliases: apelidos ?? [apelido],
                     email: email ?? "\(apelido)@x", plan: nil, isActive: ativa, isPreferred: false, warning: aviso)
}
confere(linha(conta(.claude, "a"), lida: false).isAvailable, "disponível: sem leitura")
confere(linha(conta(.claude, "a"), [("5h", 94.9), ("7d", 50), ("Fable", 100)]).isAvailable, "disponível: 5h/7d abaixo de 100 (janela por modelo não conta)")
confere(linha(conta(.claude, "a"), [("5h", 95)]).isAvailable, "disponível: 5h em 95% (perto do limite, ainda atende)")
confere(linha(conta(.claude, "a"), [("5h", 6), ("7d", 96)]).isAvailable, "disponível: 7d em 96%")
confere(linha(conta(.codex, "a"), [("7d", 99)]).isAvailable, "disponível: 7d em 99%")
confere(!linha(conta(.claude, "a"), [("5h", 100)]).isAvailable, "indisponível: 5h em 100%")
confere(!linha(conta(.claude, "a"), [("5h", 33), ("7d", 100)]).isAvailable, "indisponível: 7d em 100%")
confere(!linha(conta(.claude, "a"), [("5h", 10)], limite: true).isAvailable, "indisponível: limitReached")
confere(!linha(conta(.claude, "a", aviso: "login recusado"), lida: false).isAvailable, "indisponível: aviso da conta")

// --- o menu da aba (AIChoices)
let cheia = linha(conta(.claude, "cheia", email: "c@x"), [("5h", 100), ("7d", 20)])
let gptP = linha(conta(.codex, "principal", ativa: true, email: "g@x"), [("7d", 3)])
let ativa = linha(conta(.claude, "principal", ativa: true, apelidos: ["principal", "assinaturas"], email: "p@x"), [("5h", 10), ("7d", 10)])
let morta = linha(conta(.claude, "reserva", aviso: "login recusado: entre de novo", email: "r@x"), lida: false)
confere(gptP.account.isUsed(by: "gpt:principal"), "ponto verde: Codex da aba selecionada")
confere(!ativa.account.isUsed(by: "gpt:principal"), "ponto verde: Claude ativo não marca aba Codex")
confere(ativa.account.isUsed(by: "claude:ordem"), "ponto verde: Claude seguindo a ordem, sem a conta dita, marca a ativa")
confere(!gptP.account.isUsed(by: "claude:ordem"), "ponto verde: ordem Claude não marca Codex")
confere(ativa.account.isUsed(by: "claude:assinaturas"), "ponto verde: apelidos da mesma conta")
confere(!ativa.account.isUsed(by: nil) && !gptP.account.isUsed(by: nil), "ponto verde: sem conta conhecida, sem indicação")
let fila = [cheia, gptP, ativa, morta]
let linhasClaude = AIChoices.rows(lines: fila, current: "claude:reserva", program: .claude)
confere(linhasClaude.map(\.kind) == [.follow, .separator, .account, .account, .account, .account], "menu: seguir, separador, uma linha por conta na ordem")
confere(linhasClaude.map(\.title) == [AIChoices.followTitle, "", "Claude · cheia · c@x — no limite", "GPT · principal · g@x",
                                        "Claude · principal · p@x", "Claude · reserva · r@x — login recusado: entre de novo"],
        "menu: títulos, com 'no limite' ou o aviso", "\(linhasClaude.map(\.title))")
confere(linhasClaude.map(\.mark) == [.none, .none, .none, .none, .none, .on], "menu: ✓ na conta da aba", "\(linhasClaude.map(\.mark))")
confere(linhasClaude.allSatisfy { $0.kind == .separator || $0.enabled }, "menu: tudo habilitado para o Claude")
confere(linhasClaude.map(\.key) == [AIHelper.followOrder, nil, "claude:cheia", "gpt:principal", "claude:principal", "claude:reserva"],
        "menu: seguir a ordem pede claude:ordem, mesmo com a 1ª disponível no GPT (o núcleo resolve)", "\(linhasClaude.map(\.key))")
confere(linhasClaude.first?.help == "Agora: GPT · principal", "menu: ajuda do seguir diz a conta de agora", "\(String(describing: linhasClaude.first?.help))")
confere(linhasClaude.map(\.label) == ["a ordem de prioridade", "", "Claude · cheia", "GPT · principal", "Claude · principal", "Claude · reserva"],
        "menu: cada escolha tem o nome curto com que é dita depois", "\(linhasClaude.map(\.label))")
let pelaAssinatura = AIChoices.rows(lines: fila, current: "claude:assinaturas", program: .claude)
confere(pelaAssinatura[4].mark == .on, "menu: ✓ por qualquer apelido da conta")
let seguindo = AIChoices.rows(lines: fila, current: AIHelper.followOrder, program: .claude)
confere(seguindo[0].mark == .on && seguindo[4].mark == .mixed && seguindo.filter { $0.mark == .mixed }.count == 1,
        "menu: ✓ em seguir e, sem a conta dita, traço na Claude ativa", "\(seguindo.map(\.mark))")
let rodandoNoGPT = AIChoices.rows(lines: fila, current: AIHelper.followOrder, running: "gpt:principal", program: .codex)
confere(rodandoNoGPT[0].mark == .on && rodandoNoGPT[3].mark == .mixed && rodandoNoGPT.filter { $0.mark == .mixed }.count == 1,
        "menu: seguindo a ordem no GPT, o traço vai na conta em que roda", "\(rodandoNoGPT.map(\.mark))")
let rodandoNaOutra = AIChoices.rows(lines: fila, current: AIHelper.followOrder, running: "claude:assinaturas", program: .claude)
confere(rodandoNaOutra[4].mark == .mixed && rodandoNaOutra.filter { $0.mark == .mixed }.count == 1,
        "menu: a conta em que roda casa por qualquer apelido")
let rodandoSemLinha = AIChoices.rows(lines: fila, current: AIHelper.followOrder, running: "claude:sumida", program: .claude)
confere(rodandoSemLinha.filter { $0.mark == .mixed }.isEmpty,
        "menu: a conta em que roda, fora da fila, não põe o traço em outra")
let fixaComAtual = AIChoices.rows(lines: fila, current: "claude:reserva", running: "claude:principal", program: .claude)
confere(fixaComAtual[5].mark == .on && fixaComAtual.filter { $0.mark == .mixed }.isEmpty,
        "menu: aba fixa numa conta: ✓ nela, sem traço")
let outro = AIChoices.rows(lines: fila, current: nil, program: AIProgramKind(command: "sleep"))
confere(outro.first?.title == "Esta aba está rodando sleep" && outro.first?.kind == .note, "outro programa: a linha que diz o quê")
confere(outro.allSatisfy { !$0.enabled }, "outro programa: tudo desabilitado")
let concha = AIChoices.rows(lines: fila, current: nil, program: .shell)
confere(concha.allSatisfy { $0.kind == .separator || $0.enabled } && concha.allSatisfy { $0.mark == .none }, "concha: tudo habilitado, nada marcado")
confere(AIChoices.rows(lines: [], current: nil, program: .claude).map(\.kind) == [.follow], "sem contas: só seguir a ordem")
let perto = linha(conta(.claude, "reserva", email: "r@x"), [("5h", 6), ("7d", 96)])
confere(!AIChoices.title(perto).contains("no limite"), "menu: conta perto do limite não diz 'no limite'", AIChoices.title(perto))
confere(AIChoices.programLabel(command: "claude", account: "claude:reserva") == "claude · reserva", "lateral: claude · <apelido> quando fixa")
confere(AIChoices.programLabel(command: "claude", account: "claude:ordem") == "claude", "lateral: seguindo a ordem, como hoje")
confere(AIChoices.programLabel(command: "codex", account: "claude:ordem") == "codex", "lateral: Codex seguindo a ordem, o programa")
confere(AIChoices.programLabel(command: "codex", account: "gpt:trabalho") == "codex · trabalho", "lateral: codex · <apelido> fora do principal")
confere(AIChoices.programLabel(command: "codex", account: "gpt:principal") == "codex", "lateral: codex no principal, como hoje")
confere(AIChoices.programLabel(command: "zsh", account: nil) == "zsh", "lateral: sem IA, o programa")

// --- o programa da aba
confere(AIProgramKind(command: "-zsh") == .shell && AIProgramKind(command: "zsh") == .shell
        && AIProgramKind(command: "bash") == .shell && AIProgramKind(command: "fish") == .shell, "programa: conchas")
confere(AIProgramKind(command: "2.1.284") == .claude && AIProgramKind(command: "claude") == .claude, "programa: claude (e número de versão)")
confere(AIProgramKind(command: "codex") == .codex, "programa: codex")
confere(AIProgramKind(command: "") == .unknown && AIProgramKind(command: "").allowsChoice, "programa: vazio é desconhecido e liberado")
confere(AIProgramKind(command: "vim") == .other("vim") && !AIProgramKind(command: "vim").allowsChoice, "programa: outro programa trava o menu")
confere(AIProgramKind(command: "2.1") == .claude && AIProgramKind(command: "2.") == .other("2."), "programa: versão precisa de números dos dois lados")

// --- a IA de cada aba (keep ia abas)
let inicio: UInt64 = 1_791_000_000_123
func abas(exato: Bool = true, inicioMs: UInt64? = inicio, pid: UInt32 = 4242, versao: Int = 1, ok: Bool = true,
          ia: [[String: Any]]) -> Data {
    var keepd: [String: Any] = ["pid": pid]
    if let inicioMs { keepd["inicioMs"] = inicioMs }
    return try! JSONSerialization.data(withJSONObject: ["versao": versao, "ok": ok, "exato": exato, "keepd": keepd, "ia": ia])
}
let entradas: [[String: Any]] = [
    ["workspace": "w", "aba": 2, "agente": "claude", "conta": "claude:ordem", "atual": "claude:reserva", "vinculo": "exato", "pid": 10, "conversa": NSNull()],
    ["workspace": "w", "aba": 3, "agente": "codex", "conta": "gpt:trabalho", "atual": "gpt:trabalho", "vinculo": "provavel", "pid": 11],
    ["workspace": "w", "aba": 4, "agente": "claude", "conta": "claude:com espaço", "vinculo": "exato"],
    ["workspace": "w", "aba": 5, "agente": "codex", "conta": "claude:ordem", "atual": "gpt:principal", "vinculo": "exato"],
    ["workspace": "w", "aba": 6, "agente": "claude", "conta": "claude:reserva", "atual": "claude:ordem", "vinculo": "exato"],
]
func ms(_ d: Date = Date()) -> Double { d.timeIntervalSince1970 * 1000 }
MainActor.assumeIsolated {
    let loja = AITabAccounts()
    _ = loja.daemon(Date(timeIntervalSince1970: 1_791_000_000))
    confere(!loja.adopt(abas(ia: entradas), askedAt: ms(), daemonPid: nil, daemonStartedMs: nil),
            "abas: sem saber do daemon do app, nada é crido")
    confere(loja.adopt(abas(ia: entradas), askedAt: ms(), daemonPid: 4242, daemonStartedMs: inicio + 1),
            "abas: daemon novo, início dentro de 1 ms: lido")
    confere(loja.account(workspace: "w", tab: 2, program: .claude) == AIHelper.followOrder, "abas: a escolha da aba 2 (seguir a ordem)")
    confere(loja.running(workspace: "w", tab: 2, program: .claude) == "claude:reserva", "abas: a conta em que ela roda")
    confere(loja.confirmedAccount(workspace: "w", tab: 2, program: .claude) == "claude:reserva", "rodapé: marca a conta em que roda")
    confere(loja.account(workspace: "w", tab: 3, program: .codex) == "gpt:trabalho", "abas: Codex numa conta (vínculo provável também vale)")
    confere(loja.account(workspace: "w", tab: 4, program: .claude) == AIHelper.followOrder, "abas: chave inválida ignorada (fica o padrão)")
    confere(loja.account(workspace: "w", tab: 5, program: .codex) == AIHelper.followOrder
            && loja.running(workspace: "w", tab: 5, program: .codex) == "gpt:principal",
            "abas: Codex seguindo a ordem: a escolha é claude:ordem, roda no GPT")
    confere(loja.confirmedAccount(workspace: "w", tab: 5, program: .codex) == "gpt:principal", "rodapé: o Codex que segue a ordem marca o GPT")
    confere(loja.running(workspace: "w", tab: 6, program: .claude) == nil, "abas: 'atual' que não é conta é ignorado")
    confere(loja.account(workspace: "w", tab: 9, program: .claude) == AIHelper.followOrder, "abas: Claude sem linha segue a ordem")
    confere(loja.account(workspace: "w", tab: 9, program: .codex) == "gpt:principal", "abas: Codex sem linha está no principal")
    confere(loja.account(workspace: "w", tab: 2, program: .shell) == nil && loja.running(workspace: "w", tab: 2, program: .shell) == nil,
            "abas: aba de volta à concha não tem conta")
    confere(loja.account(workspace: "w", tab: 2, program: .other("vim")) == nil, "abas: outro programa não tem conta")
    confere(loja.confirmedAccount(workspace: "w", tab: 2, program: .shell) == "claude:reserva"
            && loja.confirmedAccount(workspace: "w", tab: 3, program: .unknown) == "gpt:trabalho",
            "rodapé: o daemon antigo diz zsh de uma aba com IA: vale o que o núcleo achou nela")
    confere(loja.confirmedAccount(workspace: "w", tab: 9, program: .shell) == nil
            && loja.confirmedAccount(workspace: "w", tab: 2, program: .other("vim")) == nil,
            "rodapé: concha sem IA que o núcleo ache, ou outro programa, não marca conta")
    confere(!loja.adopt(abas(ia: entradas), askedAt: ms(), daemonPid: 4242, daemonStartedMs: inicio), "abas: a mesma resposta não muda nada")
    // anotação otimista: vale até uma resposta pedida depois dela
    let antesDaTroca = ms()
    Thread.sleep(forTimeInterval: 0.01)
    loja.note(workspace: "w", tab: 2, key: "claude:principal")
    confere(loja.account(workspace: "w", tab: 2, program: .claude) == "claude:principal", "otimista: a troca aparece na hora")
    confere(loja.running(workspace: "w", tab: 2, program: .claude) == nil, "otimista: e a conta antiga não fica dita")
    _ = loja.adopt(abas(ia: entradas), askedAt: antesDaTroca, daemonPid: 4242, daemonStartedMs: inicio)
    confere(loja.account(workspace: "w", tab: 2, program: .claude) == "claude:principal", "otimista: resposta pedida antes da troca não a desfaz")
    var novas = entradas
    novas[0]["conta"] = "claude:assinaturas"
    novas[0]["atual"] = "claude:assinaturas"
    Thread.sleep(forTimeInterval: 0.01)
    confere(loja.adopt(abas(ia: novas), askedAt: ms(), daemonPid: 4242, daemonStartedMs: inicio), "otimista: resposta pedida depois publica")
    confere(loja.account(workspace: "w", tab: 2, program: .claude) == "claude:assinaturas", "otimista: vale o que o núcleo leu")
    confere(!loja.adopt(abas(ia: entradas), askedAt: antesDaTroca, daemonPid: 4242, daemonStartedMs: inicio)
            && loja.account(workspace: "w", tab: 2, program: .claude) == "claude:assinaturas",
            "abas: resposta mais velha que a que se tem é descartada")
    // de outro keepd, ou fora do contrato: ignorada
    let agora = ms() + 1000
    confere(!loja.adopt(abas(ia: entradas), askedAt: agora, daemonPid: 4242, daemonStartedMs: inicio + 2),
            "abas: daemon novo com início 2 ms diferente: ignorada")
    confere(!loja.adopt(abas(versao: 2, ia: entradas), askedAt: agora, daemonPid: 4242, daemonStartedMs: inicio), "abas: versão 2 ignorada")
    confere(!loja.adopt(abas(ok: false, ia: entradas), askedAt: agora, daemonPid: 4242, daemonStartedMs: inicio), "abas: ok false ignorada")
    confere(loja.account(workspace: "w", tab: 2, program: .claude) == "claude:assinaturas", "abas: as ignoradas não mexeram em nada")
    // daemon antigo: o núcleo não sabe o início do daemon; vale o processo
    // do outro lado do socket
    confere(loja.adopt(abas(exato: false, inicioMs: 1_790_000_000_000, pid: 4242, ia: entradas), askedAt: ms() + 2000,
                       daemonPid: 4242, daemonStartedMs: nil),
            "abas: daemon antigo, mesmo processo: lida, qualquer que seja o início que o núcleo viu")
    confere(loja.account(workspace: "w", tab: 2, program: .claude) == AIHelper.followOrder, "abas: daemon antigo: a escolha lida")
    confere(!loja.adopt(abas(exato: false, pid: 999, ia: novas), askedAt: ms() + 3000, daemonPid: 4242, daemonStartedMs: nil),
            "abas: daemon antigo de outro processo: ignorada")
    confere(!loja.adopt(abas(exato: false, ia: novas), askedAt: ms() + 4000, daemonPid: nil, daemonStartedMs: nil),
            "abas: daemon antigo, sem saber o processo do app: ignorada")
    // outro daemon no socket do app: o que se sabia era de abas que acabaram
    confere(loja.daemon(Date(timeIntervalSince1970: 1_791_000_500)) && loja.account(workspace: "w", tab: 3, program: .codex) == "gpt:principal",
            "abas: keepd novo apaga o que se leu")
    confere(!loja.daemon(Date(timeIntervalSince1970: 1_791_000_500)), "abas: o mesmo keepd não apaga nada")
}

// --- o keep do app: onde, e com que ambiente
let falso = ambiente["FALSO_IA"]!
let falsoDir = ambiente["FAKE_IA_DIR"]!
confere(AIHelper.path(environment: ["KEEP_IA_BIN": "/nao/existe"], bundled: falso) == nil, "ajudante: KEEP_IA_BIN inexistente = nenhum (sem cair no do app)")
confere(AIHelper.path(environment: ["KEEP_IA_BIN": "/var/empty"], bundled: falso) == nil, "ajudante: KEEP_IA_BIN pasta = nenhum")
confere(AIHelper.path(environment: ["KEEP_IA_BIN": falso], bundled: "/nao/existe") == falso, "ajudante: KEEP_IA_BIN executável = ele")
confere(AIHelper.path(environment: [:], bundled: falso) == falso, "ajudante: sem KEEP_IA_BIN, o keep de dentro do app")
confere(AIHelper.path(environment: [:], bundled: "/nao/existe") == nil, "ajudante: sem nenhum, nenhum")
confere(AIHelper.path(environment: [:], bundled: falsoDir) == nil, "ajudante: pasta no lugar do keep do app não conta")
confere(AIHelper.path(environment: [:]) == nil, "ajudante: um binário sem pacote não tem keep dentro")
confere(KeepCLI.bundledPath.hasSuffix("/Contents/Resources/keep"), "ajudante: o do app é Contents/Resources/keep")
let real = KeepCLI.environment(["HOME": "/Users/x", "PATH": "/bin"])
confere(real["KEEP_SOCKET"] == Daemon.socketPath && real["KEEP_IA_HOME"] == nil && real["KEEP_IA_SECURITY"] == nil
        && real["KEEP_IA_CLAUDE_URL"] == nil && real["PATH"] == "/bin",
        "ambiente: o app de verdade passa só o socket, por cima do próprio ambiente", "\(real)")
let teste = KeepCLI.environment(["KEEP_AI_USAGE_HOME": "/casa/falsa"])
confere(teste["KEEP_IA_HOME"] == "/casa/falsa" && teste["KEEP_IA_ESTADO"] == "/casa/falsa/.keep-ia-estado",
        "ambiente: app de teste: o núcleo lê e grava só na casa falsa", "\(teste)")
confere(teste["KEEP_IA_SECURITY"] == "/casa/falsa/.keep-ia-estado/sem-chaveiro",
        "ambiente: app de teste: o Chaveiro é um que não tem nada")
confere(teste["KEEP_IA_PERFIL_URL"] == KeepCLI.nowhere && teste["KEEP_IA_CLAUDE_URL"] == KeepCLI.nowhere
        && teste["KEEP_IA_CODEX_URL"] == KeepCLI.nowhere && teste["KEEP_IA_OPENROUTER_URL"] == KeepCLI.nowhere
        && teste["KEEP_IA_TOKEN_URL"] == KeepCLI.nowhere,
        "ambiente: app de teste: os serviços apontam para onde nada escuta")
let comSubstitutos = KeepCLI.environment(["KEEP_AI_USAGE_HOME": "/c", "KEEP_IA_SECURITY": "/meu/security",
                                          "KEEP_AI_USAGE_CLAUDE_URL": "http://127.0.0.1:5/claude",
                                          "KEEP_IA_CODEX_URL": "http://127.0.0.1:5/codex", "CLAUDE_KIT_PERFIL_URL": "http://127.0.0.1:5/p"])
confere(comSubstitutos["KEEP_IA_SECURITY"] == "/meu/security" && comSubstitutos["KEEP_IA_CLAUDE_URL"] == nil
        && comSubstitutos["KEEP_IA_CODEX_URL"] == "http://127.0.0.1:5/codex" && comSubstitutos["KEEP_IA_PERFIL_URL"] == nil,
        "ambiente: os substitutos que o teste nomeou (por qualquer dos nomes) ficam", "\(comSubstitutos)")
confere(KeepCLI.environment(["KEEP_AI_USAGE_HOME": ""])["KEEP_IA_HOME"] == nil, "ambiente: casa falsa vazia não é casa")
let estadoDoTeste = casa.appendingPathComponent(".keep-ia-estado").path
let preparado = KeepCLI.environment(ambiente)
KeepCLI.prepare(preparado)
let semChaveiro = estadoDoTeste + "/" + KeepCLI.noKeychainName
confere(preparado["KEEP_IA_SECURITY"] == semChaveiro && fm.isExecutableFile(atPath: semChaveiro),
        "Chaveiro do app de teste: escrito na casa falsa, executável")
let pergunta = Process()
pergunta.executableURL = URL(fileURLWithPath: semChaveiro)
pergunta.arguments = ["find-generic-password", "-a", "x", "-w", "-s", "Claude Code-credentials"]
var respondeu: Int32 = -1
if (try? pergunta.run()) != nil {
    pergunta.waitUntilExit()
    respondeu = pergunta.terminationStatus
}
confere(respondeu == 44, "Chaveiro do app de teste: não tem item nenhum (44)", "saiu \(respondeu)")
KeepCLI.prepare(["KEEP_IA_ESTADO": "/var/empty/x", "KEEP_IA_SECURITY": "/outro/lugar"])
confere(!fm.fileExists(atPath: "/outro/lugar"), "Chaveiro do app de teste: só onde o próprio ambiente pôs")
confere(AIHelper.isValidKey("claude:reserva") && AIHelper.isValidKey("gpt:principal") && AIHelper.isValidKey(AIHelper.followOrder),
        "chave: formatos válidos")
for ruim in ["claude:", "gpt:a b", "claude:a/b", "claude:a:b", "--para=x", "outro:x", "claude:x\n"] {
    confere(!AIHelper.isValidKey(ruim), "chave recusada: \(ruim.debugDescription)")
}

// --- perguntas ao núcleo, contra o falso que registra o que recebe
func registro(_ arquivo: String) -> [[String: Any]] {
    ((try? String(contentsOfFile: falsoDir + "/" + arquivo, encoding: .utf8)) ?? "").split(separator: "\n")
        .compactMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
}
func chamadas() -> [[String]] { registro("calls.jsonl").compactMap { $0["argv"] as? [String] } }
func leituras() -> [[String]] { registro("reads.jsonl").compactMap { $0["argv"] as? [String] } }
func limpa() { try? fm.removeItem(atPath: falsoDir + "/calls.jsonl"); try? fm.removeItem(atPath: falsoDir + "/reads.jsonl") }
setenv("KEEP_IA_BIN", falso, 1)
limpa()
if case .success(let nova) = AIHelper.moveOrder("claude:assinaturas", up: false) {
    confere(nova.contains("claude:assinaturas"), "mover: devolve a ordem nova", "\(nova)")
} else { confere(false, "mover: ok") }
confere(chamadas().last == ["ordem", "mover", "claude:assinaturas", "baixo", "--json"], "mover: argumentos", "\(chamadas())")
let ultima = registro("calls.jsonl").last
confere(ultima?["grupo"] as? String == "ia", "mover: chamado como o keep do app, com ia na frente")
confere(ultima?["socket"] as? String == Daemon.socketPath, "mover: com o socket do app")
confere(ultima?["casa"] as? String == casaFalsa, "mover: com a casa falsa do app de teste")
limpa()
if case .failure(let p) = AIHelper.moveOrder("claude:a b", up: true) { confere(p.reason == "uso" && chamadas().isEmpty, "mover: chave inválida nem chega ao núcleo") } else { confere(false, "mover: chave inválida") }
if case .failure = AIHelper.moveOrder(AIHelper.followOrder, up: true) { confere(chamadas().isEmpty, "mover: claude:ordem não é conta da fila") } else { confere(false, "mover: claude:ordem") }
limpa()
if case .success(let r) = AIHelper.switchAccount(workspace: "-ws estranho", tab: 7, to: "gpt:trabalho", interrupt: false) {
    confere(r.contains("gpt:trabalho"), "trocar: ok com o feito", r)
} else { confere(false, "trocar: ok") }
confere(chamadas().last == ["trocar", "--ws=-ws estranho", "--aba=7", "--para=gpt:trabalho", "--json"], "trocar: argumentos com =, workspace com traço e espaço", "\(chamadas())")
try! "".write(toFile: falsoDir + "/ocupada", atomically: true, encoding: .utf8)
if case .failure(let p) = AIHelper.switchAccount(workspace: "w", tab: 2, to: "claude:reserva", interrupt: false) {
    confere(p.reason == "ocupada" && p.detail == "A aba está no meio de uma resposta.", "trocar: ocupada chega com motivo e detalhe", "\(p)")
} else { confere(false, "trocar: ocupada") }
if case .success = AIHelper.switchAccount(workspace: "w", tab: 2, to: AIHelper.followOrder, interrupt: true) {
    confere(chamadas().last == ["trocar", "--ws=w", "--aba=2", "--para=claude:ordem", "--interromper", "--json"], "trocar: seguir a ordem, com --interromper", "\(chamadas())")
} else { confere(false, "trocar: interromper") }
try? fm.removeItem(atPath: falsoDir + "/ocupada")
limpa()
if case .failure(let p) = AIHelper.switchAccount(workspace: "w", tab: 2, to: "claude:x/y", interrupt: false) {
    confere(p.reason == "uso" && chamadas().isEmpty, "trocar: chave inválida recusada antes")
} else { confere(false, "trocar: chave inválida") }
// entrar: o falso abre a aba pelo cliente dado; aqui um cliente falso que só diz o número
let clienteFalso = falsoDir + "/keep-falso.sh"
try! "#!/bin/sh\necho \"$2: opened tab 7\"\n".write(toFile: clienteFalso, atomically: true, encoding: .utf8)
chmod(clienteFalso, 0o755)
setenv("FAKE_IA_KEEP", clienteFalso, 1)
if case .success(let aberta) = AIHelper.signIn(.gpt, workspace: "casa") {
    confere(aberta.workspace == "casa" && aberta.tab == 7, "entrar: devolve workspace e aba", "\(aberta)")
} else { confere(false, "entrar: ok") }
confere(chamadas().last == ["entrar", "gpt", "--ws=casa", "--json"], "entrar: argumentos", "\(chamadas())")
if case .success(let aberta) = AIHelper.signIn(.openrouter, workspace: "casa") {
    confere(aberta.tab == 7 && chamadas().last == ["entrar", "openrouter", "--ws=casa", "--json"],
            "conectar o Jev: o núcleo abre o login do OpenRouter numa aba", "\(chamadas())")
} else { confere(false, "conectar o Jev: ok") }
confere(fm.fileExists(atPath: casa.appendingPathComponent(".codex-contas/nova/auth.json").path), "entrar (falso): a conta nova aparece na casa falsa")
// conta fixa sem o login próprio: o núcleo abre o login numa aba e diz qual
try! "".write(toFile: falsoDir + "/precisa-login", atomically: true, encoding: .utf8)
if case .failure(let p) = AIHelper.switchAccount(workspace: "casa", tab: 2, to: "claude:reserva", interrupt: false) {
    confere(p.reason == "precisa-login" && p.login?.tab == 7 && p.login?.workspace == "casa",
            "trocar: precisa-login traz a aba do login e o workspace dela", "\(p)")
} else { confere(false, "trocar: precisa-login") }
try? fm.removeItem(atPath: falsoDir + "/precisa-login")
try! "".write(toFile: falsoDir + "/ocupada", atomically: true, encoding: .utf8)
if case .failure(let p) = AIHelper.switchAccount(workspace: "casa", tab: 2, to: "claude:reserva", interrupt: false) {
    confere(p.login == nil, "trocar: recusa sem aba de login não inventa uma", "\(p)")
} else { confere(false, "trocar: ocupada sem login") }
try? fm.removeItem(atPath: falsoDir + "/ocupada")
// uso, abas e sincronizar: leituras, que o app faz sozinho
limpa()
try! JSONSerialization.data(withJSONObject: try! JSONSerialization.jsonObject(with: respostaDoNucleo))
    .write(to: URL(fileURLWithPath: falsoDir + "/uso.json"))
for (pedido, argumentos) in [(AIHelper.Measure.due, ["uso", "--json"]), (.now, ["uso", "--agora", "--json"]),
                             (.cached, ["uso", "--cache", "--json"]), (.only("gpt:trabalho"), ["uso", "--conta=gpt:trabalho", "--json"])] {
    if case .success(let r) = AIHelper.usage(pedido) {
        confere(r.linhas.count == 3 && leituras().last == argumentos, "uso \(pedido.label): argumentos e linhas", "\(leituras())")
    } else { confere(false, "uso \(pedido.label): ok") }
}
for (pedido, argumentos) in [(AIHelper.Measure.due, ["jev", "--json"]), (.now, ["jev", "--agora", "--json"]),
                             (.cached, ["jev", "--cache", "--json"])] {
    if case .success(let r) = AIHelper.jev(pedido) {
        confere(r.jev == nil && leituras().last == argumentos, "jev \(pedido.label): argumentos, e sem chave não há jev", "\(leituras())")
    } else { confere(false, "jev \(pedido.label): ok") }
}
try! JSONSerialization.data(withJSONObject: try! JSONSerialization.jsonObject(with: respostaDoJev))
    .write(to: URL(fileURLWithPath: falsoDir + "/jev.json"))
if case .success(let r) = AIHelper.jev(.due) {
    confere(r.jev?.credit?.available != nil, "jev: o crédito atravessa o ajudante")
} else { confere(false, "jev: crédito pelo ajudante") }
confere(chamadas().isEmpty, "jev: leitura não conta como pedido de mudança")
try! "{\"versao\":1,\"ok\":true,\"jev\":\"texto\"}".write(toFile: falsoDir + "/jev.json", atomically: true, encoding: .utf8)
if case .failure(let p) = AIHelper.jev(.due) { confere(p.detail.contains("não dá para ler"), "jev: resposta estranha dita", p.detail) } else { confere(false, "jev: resposta estranha") }
try? fm.removeItem(atPath: falsoDir + "/jev.json")
if case .failure(let p) = AIHelper.usage(.only("claude:ordem")) { confere(p.reason == "uso", "uso: claude:ordem não é uma conta para medir") } else { confere(false, "uso: claude:ordem") }
confere(chamadas().isEmpty, "uso: leitura não conta como pedido de mudança")
try! "{\"versao\":1,\"ok\":true}".write(toFile: falsoDir + "/uso.json", atomically: true, encoding: .utf8)
if case .failure(let p) = AIHelper.usage(.due) { confere(p.detail.contains("não dá para ler"), "uso: resposta sem linhas dita", p.detail) } else { confere(false, "uso: resposta sem linhas") }
try? fm.removeItem(atPath: falsoDir + "/uso.json")
try! JSONSerialization.data(withJSONObject: ["ia": entradas]).write(to: URL(fileURLWithPath: falsoDir + "/abas.json"))
setenv("FAKE_IA_INICIO_MS", String(inicio), 1)
if case .success(let dados) = AIHelper.tabs() {
    let lidas = AITabAccounts.parse(dados, daemonPid: 0, daemonStartedMs: nil)
    confere(lidas?.count == 4 && leituras().last == ["abas", "--json"], "abas: argumentos e entradas", "\(String(describing: lidas))")
} else { confere(false, "abas: ok") }
unsetenv("FAKE_IA_INICIO_MS")
if case .success(let rodada) = AIHelper.sync() {
    confere(rodada["estado"] as? String == "feito" && leituras().last == ["sincronizar", "--json"], "sincronizar: argumentos e rodada")
} else { confere(false, "sincronizar: ok") }
if case .success(let rodada) = AIHelper.renew() {
    confere(rodada["estado"] as? String == "feito" && leituras().last == ["renovar", "--json"], "renovar: argumentos e rodada")
} else { confere(false, "renovar: ok") }
// prazo: o lento é morto no prazo
AIHelper.timeScale = 0.2
try! "5".write(toFile: falsoDir + "/lento", atomically: true, encoding: .utf8)
let t1 = Date()
if case .failure(let p) = AIHelper.moveOrder("claude:reserva", up: true) {
    confere(Date().timeIntervalSince(t1) < 4 && p.detail.contains("passou de 2 s"), "prazo: o lento é cortado em ~2 s", p.detail)
} else { confere(false, "prazo: devia falhar") }
try? fm.removeItem(atPath: falsoDir + "/lento")
AIHelper.timeScale = 1
// resposta que não é do contrato
let quebrado = falsoDir + "/quebrado.sh"
try! "#!/bin/sh\necho 'isto não é json'\nexit 1\n".write(toFile: quebrado, atomically: true, encoding: .utf8)
chmod(quebrado, 0o755)
setenv("KEEP_IA_BIN", quebrado, 1)
if case .failure(let p) = AIHelper.switchAccount(workspace: "w", tab: 1, to: "claude:reserva", interrupt: false) {
    confere(p.detail.contains("não dá para ler"), "resposta ilegível: dita", p.detail)
} else { confere(false, "resposta ilegível") }
setenv("KEEP_IA_BIN", "/nao/existe", 1)
if case .failure(let p) = AIHelper.moveOrder("claude:reserva", up: true) {
    confere(p.reason == "sem-ajudante", "sem keep: nada roda")
} else { confere(false, "sem keep") }

// --- o núcleo de verdade (crates/keep-ia), o que o app vai mesmo ler
if let nucleo = ambiente["NUCLEO"], fm.isExecutableFile(atPath: nucleo) {
    setenv("KEEP_IA_BIN", nucleo, 1)
    // Uma casa só dele: um slot do cofre do kit e o login do Codex, ambos
    // medidos pelo substituto dos serviços nesta máquina.
    let outra = URL(fileURLWithPath: ambiente["CASA_NUCLEO"]!)
    setenv("KEEP_AI_USAGE_HOME", outra.path, 1)
    if case .success(let r) = AIHelper.usage(.cached) {
        confere(r.linhas.map(\.account.order) == ["claude:principal", "gpt:principal"],
                "núcleo de verdade, sem rede: as contas da casa falsa, na ordem", "\(r.linhas.map(\.account.order))")
        confere(r.linhas.allSatisfy { $0.reading == nil }, "núcleo de verdade, sem rede: nada medido ainda")
    } else { confere(false, "núcleo de verdade: --cache respondeu") }
    if case .success(let r) = AIHelper.usage(.due) {
        let claude = r.linhas.first { $0.account.engine == .claude }
        let gpt = r.linhas.first { $0.account.engine == .codex }
        confere(claude?.reading?.windows.map(\.label) == ["5h", "7d"] && claude?.reading?.windows.first?.percent == 17,
                "núcleo de verdade: a leitura do Claude chega ao app", "\(String(describing: claude))")
        confere(claude?.reading?.windows.first?.resetsAt == Date(timeIntervalSince1970: 1_791_100_000),
                "núcleo de verdade: o reinício chega como data", "\(String(describing: claude?.reading?.windows.first?.resetsAt?.timeIntervalSince1970))")
        confere(claude?.measuredAt.map { abs($0.timeIntervalSinceNow) < 60 } == true, "núcleo de verdade: medido agora")
        confere(gpt?.reading?.limitReached == true && gpt?.isAvailable == false,
                "núcleo de verdade: o GPT no limite sai da fila", "\(String(describing: gpt))")
        confere(claude?.account.email == "um@exemplo.com" && claude?.account.plan == "Max 20x",
                "núcleo de verdade: e-mail e plano da conta")
    } else { confere(false, "núcleo de verdade: a rodada respondeu") }
    // O Jev: sem chave no Chaveiro do app de teste não há Jev; com a chave
    // numa "security" de teste, o crédito vem do substituto do OpenRouter.
    if case .success(let r) = AIHelper.jev(.now) {
        confere(r.jev == nil, "núcleo de verdade: sem chave do Jev no Chaveiro, não há jev", "\(String(describing: r.jev))")
    } else { confere(false, "núcleo de verdade: jev sem chave respondeu") }
    if let comChave = ambiente["SEGURANCA_COM_CHAVE"] {
        setenv("KEEP_IA_SECURITY", comChave, 1)
        if case .success(let r) = AIHelper.jev(.now) {
            let credito = r.jev?.credit
            confere(credito?.total == 10 && credito?.keyLimit == 5 && abs((credito?.available ?? 0) - 4.9958) < 1e-9,
                    "núcleo de verdade: o crédito do Jev chega ao app", "\(String(describing: r.jev))")
            confere(r.jev?.measuredAt.map { abs($0.timeIntervalSinceNow) < 60 } == true && r.jev?.problem == nil,
                    "núcleo de verdade: o crédito do Jev medido agora")
        } else { confere(false, "núcleo de verdade: jev com chave respondeu") }
        let guardado = (try? String(contentsOf: outra.appendingPathComponent(".keep-ia-estado/jev.json"), encoding: .utf8)) ?? ""
        confere(!guardado.isEmpty && !guardado.contains("sk-or-do-teste"), "núcleo de verdade: o cache do Jev não guarda a chave")
        unsetenv("KEEP_IA_SECURITY")
        if case .success(let r) = AIHelper.jev(.now) {
            confere(r.jev == nil, "núcleo de verdade: tirada a chave, o Jev some", "\(String(describing: r.jev))")
        } else { confere(false, "núcleo de verdade: jev sem chave de novo respondeu") }
    }
    confere(fm.isExecutableFile(atPath: outra.appendingPathComponent(".keep-ia-estado/sem-chaveiro").path),
            "núcleo de verdade: perguntado com o Chaveiro vazio do app de teste")
    if case .success(let r) = AIHelper.usage(.only("claude:principal")) {
        confere(r.linhas.count == 2, "núcleo de verdade: --conta responde todas as linhas")
    } else { confere(false, "núcleo de verdade: --conta respondeu") }
    if case .success(let nova) = AIHelper.moveOrder("gpt:principal", up: true) {
        confere(nova == ["gpt:principal", "claude:principal"], "núcleo de verdade: mover na ordem", "\(nova)")
    } else { confere(false, "núcleo de verdade: mover respondeu") }
    if case .success(let r) = AIHelper.usage(.cached) {
        confere(r.linhas.map(\.account.order) == ["gpt:principal", "claude:principal"] && r.linhas.last?.reading != nil,
                "núcleo de verdade: a ordem nova e as leituras guardadas", "\(r.linhas.map(\.account.order))")
    } else { confere(false, "núcleo de verdade: --cache depois de mover") }
} else {
    print("(pulado: sem NUCLEO, o keep de verdade)")
}

try? fm.removeItem(at: casa.appendingPathComponent(".codex-contas"))
print("\(casos - falhas)/\(casos) ok")
exit(falhas == 0 ? 0 : 1)
