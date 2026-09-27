import Foundation
// The model test of AIUsage.swift (the usage footer's facts), run by
// tools/usage-test.sh. Foundation only: no app, no network, no real logins.

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
confere(gpt?.alias == "jose" && gpt?.plan == "Pro" && gpt?.accountHeader == "ACC", "codex: apelido, plano e conta", "\(String(describing: gpt))")
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

try? fm.removeItem(at: home)
print("\(casos - falhas)/\(casos) ok")
exit(falhas == 0 ? 0 : 1)
