import Foundation
// The model test of AIUsage.swift (the usage footer's facts and the kit's
// order) and the `keep-ia` half of Daemon/KitHelper.swift, run by
// tools/usage-test.sh.
// Foundation only: no app, no network, no real logins, no real kit.

// The two app types KitHelper.swift reaches for, stubbed.
enum Trace { static func log(_ kind: String, _ detail: @autoclosure () -> String) {} }
enum Daemon { static var socketPath: String { "/tmp/sock-de-teste-uso" } }

var falhas = 0
var casos = 0
func confere(_ ok: Bool, _ nome: String, _ detalhe: @autoclosure () -> String = "") {
    casos += 1
    if ok { print("ok   \(nome)") } else { falhas += 1; print("FALHA \(nome) \(detalhe())") }
}

// --- respostas reais (formato de 26/09/2026, contas-mac.md), sem segredos
let claudeReal = """
{"five_hour":{"utilization":17.0,"resets_at":"2026-09-27T04:50:00.275240+00:00","limit_dollars":null},
 "seven_day":{"utilization":88.0,"resets_at":"2026-09-28T10:00:00.275265+00:00"},
 "seven_day_opus":null,"seven_day_sonnet":null,
 "nimbus_quill":{"utilization":0.0,"resets_at":null},
 "extra_usage":{"is_enabled":false,"monthly_limit":null,"used_credits":null,"utilization":null},
 "limits":[
  {"kind":"session","group":"session","percent":17,"severity":"normal","resets_at":"2026-09-27T04:50:00.275240+00:00","scope":null},
  {"kind":"weekly_all","group":"weekly","percent":88,"severity":"warning","resets_at":"2026-09-28T10:00:00.275265+00:00","scope":null},
  {"kind":"weekly_scoped","group":"weekly","percent":32,"severity":"normal","resets_at":"2026-09-28T10:00:00.275265+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null}}
 ]}
""".data(using: .utf8)!

let r = UsageParse.claude(claudeReal)
confere(r?.windows.map(\.label) == ["5h", "7d", "Fable"], "claude: janelas 5h, 7d e Fable", "\(String(describing: r?.windows.map(\.label)))")
confere(r?.windows.map(\.percent) == [17, 88, 32], "claude: percentuais", "\(String(describing: r?.windows.map(\.percent)))")
confere(r?.windows.first?.resetsAt == Date(timeIntervalSince1970: 1790484600), "claude: reset com 6 casas e +00:00", "\(String(describing: r?.windows.first?.resetsAt?.timeIntervalSince1970))")
confere(r?.limitReached == false, "claude: fora do limite")
confere(r?.windows.contains { $0.label == "Extra" } == false, "claude: extra desligado não aparece")

let claudeAntigo = """
{"five_hour":{"utilization":1.0,"resets_at":"2026-09-27T04:50:00Z"},"seven_day":{"utilization":100,"resets_at":null},
 "seven_day_opus":{"utilization":40,"resets_at":"2026-09-28T10:00:00Z"},"seven_day_sonnet":null,
 "extra_usage":{"is_enabled":true,"utilization":12.5,"used_credits":1,"monthly_limit":8}}
""".data(using: .utf8)!
let a = UsageParse.claude(claudeAntigo)
confere(a?.windows.map(\.label) == ["5h", "7d", "Opus", "Extra"], "claude sem limits: Opus e Extra", "\(String(describing: a?.windows.map(\.label)))")
confere(a?.windows.first?.percent == 1.0, "claude: utilization 1.0 é 1%, não 100%")
confere(a?.limitReached == true, "claude: 100% é limite")
let zero = UsageParse.claude(#"{"five_hour":{"utilization":0,"resets_at":null},"seven_day":{"utilization":0.0},"extra_usage":{"is_enabled":true,"utilization":true}}"#.data(using: .utf8)!)
confere(zero?.windows.map(\.label) == ["5h", "7d"] && zero?.windows.map(\.percent) == [0, 0], "0% aparece; booleano não vira número", "\(String(describing: zero?.windows))")
confere(UsageParse.claude("{}".data(using: .utf8)!) == nil, "claude: resposta sem janelas é nil")
confere(UsageParse.claude("não é json".data(using: .utf8)!) == nil, "claude: lixo é nil")

let codexReal = """
{"plan_type":"pro","rate_limit":{"allowed":true,"limit_reached":false,
 "primary_window":{"used_percent":2,"limit_window_seconds":604800,"reset_after_seconds":578124,"reset_at":1791047982},
 "secondary_window":null},"additional_rate_limits":null}
""".data(using: .utf8)!
let c = UsageParse.codex(codexReal)
confere(c?.windows.map(\.label) == ["7d"], "codex: uma janela semanal", "\(String(describing: c?.windows.map(\.label)))")
confere(c?.windows.first?.percent == 2 && c?.windows.first?.resetsAt == Date(timeIntervalSince1970: 1791047982), "codex: 2% e reset em epoch")
let codexDois = """
{"rate_limit":{"limit_reached":true,"primary_window":{"used_percent":100,"limit_window_seconds":18000,"reset_at":1790000000},
 "secondary_window":{"used_percent":55.5,"limit_window_seconds":604800,"reset_at":1790500000}},
 "additional_rate_limits":[{"limit_name":"codex-mini","rate_limit":{"primary_window":{"used_percent":10,"limit_window_seconds":86400,"reset_at":1790100000}}}]}
""".data(using: .utf8)!
let c2 = UsageParse.codex(codexDois)
confere(c2?.windows.map(\.label) == ["5h", "7d", "codex-mini 1d"], "codex: 5h, 7d e adicional", "\(String(describing: c2?.windows.map(\.label)))")
confere(c2?.limitReached == true, "codex: limit_reached")

let porModelo = UsageParse.claude(#"{"five_hour":{"utilization":10},"seven_day":{"utilization":50},"limits":[{"kind":"weekly_scoped","percent":100,"scope":{"model":null,"surface":{"display_name":"Claude Code"}}}],"extra_usage":{"is_enabled":true,"utilization":100}}"#.data(using: .utf8)!)
confere(porModelo?.windows.map(\.label) == ["5h", "7d", "Claude Code", "Extra"], "janela por superfície leva o nome dela", "\(String(describing: porModelo?.windows.map(\.label)))")
confere(porModelo?.limitReached == false, "janela por modelo/extra a 100% não bloqueia a conta")
let adicional = UsageParse.codex(#"{"rate_limit":{"limit_reached":false,"primary_window":{"used_percent":20,"limit_window_seconds":604800}},"additional_rate_limits":[{"limit_name":"GPT-X","rate_limit":{"limit_reached":true,"primary_window":{"used_percent":100,"limit_window_seconds":18000}}}]}"#.data(using: .utf8)!)
confere(adicional?.limitReached == false && adicional?.windows.last?.label == "GPT-X 5h", "limite adicional do Codex a 100% não bloqueia a conta", "\(String(describing: adicional))")

// --- descoberta num home falso
let fm = FileManager.default
let home = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("keep-uso-\(getpid())")
try? fm.removeItem(at: home)
let cofre = home.appendingPathComponent(".claude/contas")
try! fm.createDirectory(at: cofre, withIntermediateDirectories: true)
func slot(_ nome: String, _ corpo: [String: Any]) {
    try! JSONSerialization.data(withJSONObject: corpo).write(to: cofre.appendingPathComponent("\(nome).json"))
}
let agora = Date()
func oauth(_ token: String, expira: Date, tier: String = "default_claude_max_20x", assinatura: String = "max") -> [String: Any] {
    ["claudeAiOauth": ["accessToken": token, "refreshToken": "r", "expiresAt": expira.timeIntervalSince1970 * 1000,
                       "subscriptionType": assinatura, "rateLimitTier": tier]]
}
slot("principal", ["apelido": "principal", "email": "a@x", "accountUuid": "U1", "credenciais": oauth("t-velho", expira: agora.addingTimeInterval(3600))])
slot("assinaturas", ["apelido": "assinaturas", "email": "a@x", "accountUuid": "U1", "credenciais": oauth("t-novo", expira: agora.addingTimeInterval(7200))])
slot("reserva", ["apelido": "reserva", "email": "b@y", "accountUuid": "U2", "refreshMorto": "abc123", "credenciais": oauth("t2", expira: agora.addingTimeInterval(60), tier: "default_claude_pro", assinatura: "pro")])
try! "assinaturas\n".write(to: cofre.appendingPathComponent(".ativa"), atomically: true, encoding: .utf8)
try! "reserva".write(to: cofre.appendingPathComponent(".preferida"), atomically: true, encoding: .utf8)
try! "{}".write(to: cofre.appendingPathComponent(".oculto.json"), atomically: true, encoding: .utf8)

func b64url(_ o: [String: Any]) -> String {
    try! JSONSerialization.data(withJSONObject: o).base64EncodedString()
        .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
}
let exp = agora.addingTimeInterval(86400).timeIntervalSince1970.rounded()
let jwt = "h." + b64url(["exp": exp, "https://api.openai.com/profile": ["email": "jose@z.com"],
                           "https://api.openai.com/auth": ["chatgpt_plan_type": "pro", "chatgpt_account_id": "ACC"]]) + ".s"
try! fm.createDirectory(at: home.appendingPathComponent(".codex"), withIntermediateDirectories: true)
try! JSONSerialization.data(withJSONObject: ["auth_mode": "chatgpt", "tokens": ["access_token": jwt, "account_id": "ACC", "refresh_token": "r"]])
    .write(to: home.appendingPathComponent(".codex/auth.json"))

let contas = AIAccounts.discover(home: home)
confere(contas.count == 3, "descoberta: 2 Claude (1 duplicada fundida) + 1 Codex", "\(contas.map(\.alias))")
confere(contas.first?.alias == "reserva" && contas.first?.isPreferred == true, "ordem: preferida primeiro", "\(contas.map(\.alias))")
let u1 = contas.first { $0.key == "U1" }
confere(u1?.alias == "assinaturas" && u1?.isActive == true, "duplicada: mostrada pelo apelido ativo", "\(String(describing: u1?.alias))")
confere(u1?.aliases.sorted() == ["assinaturas", "principal"], "duplicada: guarda os dois apelidos")
confere(u1?.token == "t-novo", "duplicada: fica o token mais novo")
confere(u1?.plan == "Max 20x", "plano Max 20x")
let res = contas.first { $0.key == "U2" }
confere(res?.warning != nil && res?.plan == "Pro", "refreshMorto vira aviso; tier desconhecido cai na assinatura", "\(String(describing: res?.plan))")
let gpt = contas.first { $0.engine == .codex }
confere(gpt?.alias == "principal" && gpt?.email == "jose@z.com" && gpt?.plan == "Pro" && gpt?.accountHeader == "ACC", "codex: a única conta é a principal, com e-mail, plano e conta", "\(String(describing: gpt))")
confere(gpt?.expiresAt == Date(timeIntervalSince1970: exp), "codex: validade do JWT")
confere(AIAccounts.discover(home: URL(fileURLWithPath: "/nao/existe")).isEmpty, "sem cofre nem codex: nenhuma conta")

// --- requisições: URL de teste só em 127.0.0.1
let v = (claude: "2.1.283", codex: "0.157.0")
let rq = UsageEndpoint.request(for: u1!, environment: [:], clientVersions: v)
confere(rq?.url == UsageEndpoint.claude, "claude: URL real")
confere(rq?.value(forHTTPHeaderField: "Authorization") == "Bearer t-novo", "claude: Bearer")
confere(rq?.value(forHTTPHeaderField: "User-Agent") == "claude-cli/2.1.283 (external, cli)", "claude: User-Agent do CLI")
confere(rq?.httpMethod == "GET", "claude: GET")
let local = UsageEndpoint.request(for: u1!, environment: ["KEEP_AI_USAGE_CLAUDE_URL": "http://127.0.0.1:9999/u"], clientVersions: v)
confere(local?.url?.absoluteString == "http://127.0.0.1:9999/u", "stand-in local aceito")
for fora in ["https://evil.example/u", "http://127.0.0.1.evil.example/u", "http://localhost:9999/u", "https://127.0.0.1:9999/u"] {
    let x = UsageEndpoint.request(for: u1!, environment: ["KEEP_AI_USAGE_CLAUDE_URL": fora], clientVersions: v)
    confere(x?.url == UsageEndpoint.claude, "stand-in recusado: \(fora)", "\(String(describing: x?.url))")
}
let rqc = UsageEndpoint.request(for: gpt!, environment: [:], clientVersions: v)
confere(rqc?.url == UsageEndpoint.codex && rqc?.value(forHTTPHeaderField: "ChatGPT-Account-Id") == "ACC", "codex: URL e ChatGPT-Account-Id")
let semToken = AIAccount(engine: .claude, key: "k", alias: "a", aliases: ["a"], email: nil, plan: nil, isActive: false,
                         isPreferred: false, token: nil, accountHeader: nil, expiresAt: nil, warning: nil)
confere(UsageEndpoint.request(for: semToken, environment: [:], clientVersions: v) == nil, "sem token: sem requisição")
confere(!u1!.summary.id.isEmpty && !"\(u1!.summary)".contains("t-novo"), "resumo publicado não leva o token")

// --- versões instaladas
let bin = home.appendingPathComponent(".local/bin")
try! fm.createDirectory(at: bin, withIntermediateDirectories: true)
let vers = home.appendingPathComponent(".local/share/claude/versions")
try! fm.createDirectory(at: vers, withIntermediateDirectories: true)
fm.createFile(atPath: vers.appendingPathComponent("9.8.7").path, contents: Data())
try! fm.createSymbolicLink(at: bin.appendingPathComponent("claude"), withDestinationURL: vers.appendingPathComponent("9.8.7"))
let rel = home.appendingPathComponent(".codex/packages/standalone/releases/1.2.3-aarch64-apple-darwin/bin")
try! fm.createDirectory(at: rel, withIntermediateDirectories: true)
fm.createFile(atPath: rel.appendingPathComponent("codex").path, contents: Data())
let cur = home.appendingPathComponent(".codex/packages/standalone/current")
try! fm.createSymbolicLink(at: cur, withDestinationURL: home.appendingPathComponent(".codex/packages/standalone/releases/1.2.3-aarch64-apple-darwin"))
try! fm.createSymbolicLink(at: bin.appendingPathComponent("codex"), withDestinationURL: cur.appendingPathComponent("bin/codex"))
let iv = UsageEndpoint.installedVersions(home: home)
confere(iv.claude == "9.8.7" && iv.codex == "1.2.3", "versões lidas dos links", "\(iv)")
let iv0 = UsageEndpoint.installedVersions(home: URL(fileURLWithPath: "/nao/existe"))
confere(iv0.claude == "2.1.283" && iv0.codex == "0.157.0", "versões: padrão sem ferramentas")

// --- texto
let t0 = Date(timeIntervalSince1970: 1_790_000_000)
confere(UsageText.until(t0.addingTimeInterval(41 * 60), now: t0) == "41min", "falta 41min")
confere(UsageText.until(t0.addingTimeInterval(4 * 3600 + 10 * 60), now: t0) == "4h10", "falta 4h10")
confere(UsageText.until(t0.addingTimeInterval(3 * 3600), now: t0) == "3h", "falta 3h")
confere(UsageText.until(t0.addingTimeInterval(37 * 3600 + 5), now: t0) == "1d13h", "falta 1d13h", "\(String(describing: UsageText.until(t0.addingTimeInterval(37 * 3600 + 5), now: t0)))")
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

// --- a ordem de prioridade (.ordem)
// Sem .ordem: a ordem de hoje (preferida, depois por nome; Claude antes de GPT).
let semOrdem = AIAccounts.discover(home: home)
confere(semOrdem.map(\.summary.order) == ["claude:reserva", "claude:assinaturas", "gpt:principal"],
        "sem .ordem: a ordem de hoje, com a chave de cada uma", "\(semOrdem.map(\.summary.order))")
let arquivoOrdem = cofre.appendingPathComponent(".ordem")
func ordem(_ texto: String) { try! texto.write(to: arquivoOrdem, atomically: true, encoding: .utf8) }
// Cruza motores, comentário e linha vazia, chave repetida, chave sem conta.
ordem("# prioridade\n\ngpt:principal\nclaude:sumida\nclaude:principal\ngpt:principal\n")
let comOrdem = AIAccounts.discover(home: home)
confere(comOrdem.map(\.alias) == ["principal", "assinaturas", "reserva"]
        && comOrdem.map(\.engine) == [.codex, .claude, .claude],
        "ordem: GPT na frente, fundida no lugar de um dos apelidos, a não listada no fim",
        "\(comOrdem.map { "\($0.engine.orderPrefix):\($0.alias)" })")
let fundida = comOrdem.first { $0.key == "U1" }
confere(fundida?.orderKey == "claude:principal", "fundida: a chave é a do apelido listado", "\(String(describing: fundida?.orderKey))")
confere(comOrdem.last?.orderKey == "claude:reserva", "não listada: a chave é a do apelido mostrado")
// Fundida pela MENOR posição entre os apelidos.
ordem("claude:reserva\nclaude:principal\ngpt:principal\nclaude:assinaturas\n")
let menor = AIAccounts.discover(home: home)
confere(menor.map(\.summary.order) == ["claude:reserva", "claude:principal", "gpt:principal"],
        "fundida: vale a menor posição entre os apelidos", "\(menor.map(\.summary.order))")
ordem("claude:assinaturas\ngpt:principal\nclaude:principal\n")
let menor2 = AIAccounts.discover(home: home)
confere(menor2.first?.orderKey == "claude:assinaturas" && menor2.first?.alias == "assinaturas",
        "fundida: a chave é a do apelido da menor posição", "\(menor2.map(\.summary.order))")
confere(AIOrder.read(URL(fileURLWithPath: "/nao/existe")).isEmpty, "ordem ausente: nada listado")
try? fm.removeItem(at: arquivoOrdem)

// --- contas GPT extras (.codex-contas/<apelido>/auth.json)
let extras = home.appendingPathComponent(".codex-contas")
func extra(_ nome: String, email: String, conta: String) {
    let pasta = extras.appendingPathComponent(nome)
    try! fm.createDirectory(at: pasta, withIntermediateDirectories: true)
    let token = "h." + b64url(["exp": exp, "https://api.openai.com/profile": ["email": email],
                               "https://api.openai.com/auth": ["chatgpt_plan_type": "plus", "chatgpt_account_id": conta]]) + ".s"
    try! JSONSerialization.data(withJSONObject: ["tokens": ["access_token": token, "account_id": conta]])
        .write(to: pasta.appendingPathComponent("auth.json"))
}
extra("trabalho", email: "t@z.com", conta: "ACC-T")
extra("aberta", email: "a@z.com", conta: "ACC-A")
extra("copia", email: "jose@z.com", conta: "ACC")          // o mesmo login do principal
try! fm.createDirectory(at: extras.appendingPathComponent(".escondida"), withIntermediateDirectories: true)
try! fm.createDirectory(at: extras.appendingPathComponent("vazia"), withIntermediateDirectories: true)
let gpts = AIAccounts.codexAccounts(home: home)
confere(gpts.map(\.alias) == ["principal", "aberta", "trabalho"], "GPT: principal, depois as extras por nome; a cópia funde; pasta com ponto ou vazia fica de fora", "\(gpts.map(\.alias))")
confere(gpts.first?.aliases == ["principal", "copia"], "GPT: a cópia do principal é o principal", "\(String(describing: gpts.first?.aliases))")
confere(gpts.first?.isActive == true && gpts.dropFirst().allSatisfy { !$0.isActive }, "GPT: só o principal está em uso")
let trabalho = gpts.first { $0.alias == "trabalho" }
confere(trabalho?.email == "t@z.com" && trabalho?.accountHeader == "ACC-T" && trabalho?.summary.order == "gpt:trabalho",
        "GPT extra: e-mail, conta e chave gpt:<apelido>", "\(String(describing: trabalho?.summary))")
let todas = AIAccounts.discover(home: home)
confere(todas.map(\.summary.order) == ["claude:reserva", "claude:assinaturas", "gpt:principal", "gpt:aberta", "gpt:trabalho"],
        "descoberta: Claude, depois GPT principal e extras", "\(todas.map(\.summary.order))")
confere(todas.first { $0.alias == "principal" && $0.engine == .codex }?.summary.keys == ["gpt:principal", "gpt:copia"],
        "chaves de uma conta: uma por apelido")

// --- a assinatura dos arquivos (o vigia de 2 s)
let assinatura0 = AIAccounts.signature(home: home)
confere(assinatura0 == AIAccounts.signature(home: home), "assinatura: igual quando nada muda")
Thread.sleep(forTimeInterval: 0.01)
ordem("gpt:trabalho\n")
let assinatura1 = AIAccounts.signature(home: home)
confere(assinatura1 != assinatura0, "assinatura: muda com a .ordem")
extra("nova", email: "n@z.com", conta: "ACC-N")
confere(AIAccounts.signature(home: home) != assinatura1, "assinatura: muda com uma conta GPT nova")
let antesDoOculto = AIAccounts.signature(home: home)
try! "{}".write(to: cofre.appendingPathComponent(".oculto.json"), atomically: true, encoding: .utf8)
confere(AIAccounts.signature(home: home) == antesDoOculto, "assinatura: arquivo oculto do cofre não conta")
try? fm.removeItem(at: arquivoOrdem)

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
confere(linha(conta(.claude, "a"), [("5h", 94.9), ("7d", 50), ("Fable", 100)]).isAvailable, "disponível: 5h/7d abaixo de 95 (janela por modelo não conta)")
confere(!linha(conta(.claude, "a"), [("5h", 95)]).isAvailable, "indisponível: 5h em 95%")
confere(!linha(conta(.codex, "a"), [("7d", 99)]).isAvailable, "indisponível: 7d em 99%")
confere(!linha(conta(.claude, "a"), [("5h", 10)], limite: true).isAvailable, "indisponível: limitReached")
confere(!linha(conta(.claude, "a", aviso: "login recusado"), lida: false).isAvailable, "indisponível: aviso da conta")

// --- o ajudante keep-ia, contra o falso que registra o que recebe
let estado = home.appendingPathComponent("estado")
try! fm.createDirectory(at: estado, withIntermediateDirectories: true)
let falso = ProcessInfo.processInfo.environment["FALSO_IA"]!
let falsoDir = ProcessInfo.processInfo.environment["FAKE_IA_DIR"]!
func chamadas() -> [[String]] {
    ((try? String(contentsOfFile: falsoDir + "/calls.jsonl", encoding: .utf8)) ?? "").split(separator: "\n")
        .compactMap { (try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any])?["argv"] as? [String] }
}
func limpa() { try? fm.removeItem(atPath: falsoDir + "/calls.jsonl") }
confere(AIHelper.path(environment: ["KEEP_IA_BIN": "/nao/existe"], installed: falso) == nil, "ajudante: KEEP_IA_BIN inexistente = nenhum (sem cair no do kit)")
confere(AIHelper.path(environment: ["KEEP_IA_BIN": "/var/empty"], installed: falso) == nil, "ajudante: KEEP_IA_BIN pasta = nenhum")
confere(AIHelper.path(environment: ["KEEP_IA_BIN": falso], installed: "/nao/existe") == falso, "ajudante: KEEP_IA_BIN executável = ele")
confere(AIHelper.path(environment: [:], installed: falso) == falso, "ajudante: sem KEEP_IA_BIN, o instalado pelo kit")
confere(AIHelper.path(environment: [:], installed: "/nao/existe") == nil, "ajudante: sem nenhum, nenhum")
confere(AIHelper.path(environment: [:], installed: falsoDir) == nil, "ajudante: instalado que é pasta não conta")
confere(AIHelper.isValidKey("claude:reserva") && AIHelper.isValidKey("gpt:principal") && AIHelper.isValidKey(AIHelper.followOrder),
        "chave: formatos válidos")
for ruim in ["claude:", "gpt:a b", "claude:a/b", "claude:a:b", "--para=x", "outro:x", "claude:x\n"] {
    confere(!AIHelper.isValidKey(ruim), "chave recusada: \(ruim.debugDescription)")
}
setenv("KEEP_IA_BIN", falso, 1)
// o falso lê a casa de teste
setenv("KEEP_AI_USAGE_HOME", home.path, 1)
setenv("KIT_KEEP_ESTADO", estado.path, 1)
limpa()
if case .success(let nova) = AIHelper.moveOrder("claude:assinaturas", up: false) {
    confere(nova.first == "claude:reserva", "mover: devolve a ordem nova", "\(nova)")
} else { confere(false, "mover: ok") }
confere(chamadas().last == ["ordem", "mover", "claude:assinaturas", "baixo", "--json"], "mover: argumentos", "\(chamadas())")
limpa()
if case .failure(let p) = AIHelper.moveOrder("claude:a b", up: true) { confere(p.reason == "uso" && chamadas().isEmpty, "mover: chave inválida nem chega ao ajudante") } else { confere(false, "mover: chave inválida") }
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
if case .success = AIHelper.switchAccount(workspace: "w", tab: 2, to: "claude:reserva", interrupt: true) {
    confere(chamadas().last == ["trocar", "--ws=w", "--aba=2", "--para=claude:reserva", "--interromper", "--json"], "trocar: --interromper", "\(chamadas())")
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
confere(fm.fileExists(atPath: home.appendingPathComponent(".codex-contas/nova/auth.json").path), "entrar (falso): a conta nova aparece no disco")
// prazo: o ajudante lento é morto no prazo
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
    confere(p.reason == "sem-ajudante", "sem ajudante: nada roda")
} else { confere(false, "sem ajudante") }

try? fm.removeItem(at: home)
print("\(casos - falhas)/\(casos) ok")
exit(falhas == 0 ? 0 : 1)
