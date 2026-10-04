import Foundation
// The app-side test of Worktrees.swift; run by tools/worktrees-test.sh, which
// compiles this with the real file and stubs the two app types it touches.
enum Trace { static func log(_ k: String, _ d: @autoclosure () -> String) {} }
enum Daemon { static var socketPath: String { "/tmp/sock-de-teste" } }

var falhas = 0, casos = 0
func confere(_ ok: Bool, _ nome: String, _ det: @autoclosure () -> String = "") {
    casos += 1; if ok { print("ok   \(nome)") } else { falhas += 1; print("FALHA \(nome) \(det())") }
}
let fm = FileManager.default
let dir = ProcessInfo.processInfo.environment["FALSO_DIR"]!
func escreve(_ nome: String, _ obj: Any) { try! JSONSerialization.data(withJSONObject: obj, options: .fragmentsAllowed).write(to: URL(fileURLWithPath: dir + "/" + nome)) }
func chamadas() -> [[String: Any]] {
    ((try? String(contentsOfFile: dir + "/chamadas.log", encoding: .utf8)) ?? "").split(separator: "\n")
        .compactMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
}
func limpaChamadas() { try? fm.removeItem(atPath: dir + "/chamadas.log") }

// sem um keep para perguntar: o nomeado em KEEP_WORKTREES_BIN é o único
// procurado (inexistente ou pasta = nenhum), e sem ele o de dentro do app —
// que um binário de teste não tem
setenv("KEEP_WORKTREES_BIN", "/nao/existe", 1)
if case .off = Worktrees.list([.init(workspace: "w", tabs: [1])]) { confere(true, "sem ajudante: nada muda") } else { confere(false, "sem ajudante: nada muda") }
setenv("KEEP_WORKTREES_BIN", "/var/empty", 1)
confere(Worktrees.helper == nil, "KEEP_WORKTREES_BIN pasta: nenhum (sem cair no de dentro do app)")
unsetenv("KEEP_WORKTREES_BIN")
confere(Worktrees.helper == nil, "sem KEEP_WORKTREES_BIN: o keep de dentro do app, que aqui não há")
confere(Worktrees.index() == nil && !fm.fileExists(atPath: dir + "/chamadas.log"), "indexar sem ajudante: nada roda")
setenv("KEEP_WORKTREES_BIN", dir + "/fake-helper.py", 1)

// listar: argumentos e texto
let wt = dir + "/repo-wt-lixeira-\(getpid())"
escreve("listar.json", ["versao": 1,
    "abas": [["alvo": "Brokerfy:4,7", "pids": [999999]]],
    "lixeira": [["caminho": wt, "ramo": "fix/x", "head": "abc1234", "alteracoes": 3, "commits_so_aqui": 1, "processos": [["pid": 1, "nome": "php"]]],
                ["caminho": NSHomeDirectory() + "/projetos/wt-solta", "ramo": NSNull(), "head": "f00ba12", "alteracoes": 0, "commits_so_aqui": 0]],
    "mantidas": [["caminho": NSHomeDirectory() + "/projetos/wt-g", "motivo": "em uso pela aba “Relatório”"]],
    "avisos": ["não identifiquei a conversa da aba “zsh”"]])
limpaChamadas()
let r = Worktrees.list([.init(workspace: "Brokerfy", tabs: [4, 7]), .init(workspace: "Clinica", tabs: [2])], deadline: 2)
guard case .listing(let l) = r else { print("FALHA listar não leu"); exit(1) }
let c = chamadas().first
confere((c?["argv"] as? [String]) == ["listar", "--prazo", "2.0", "Brokerfy:4,7", "Clinica:2"], "listar: argumentos", "\(String(describing: c?["argv"]))")
confere((c?["grupo"] as? String) == "worktrees", "listar: chamado como o keep do app, com worktrees na frente", "\(String(describing: c?["grupo"]))")
confere((c?["socket"] as? String) == "/tmp/sock-de-teste", "listar: passa o socket do app")
confere(l.pids == [999999], "listar: pids das abas")
let nota = Worktrees.note(for: r)
print("--- nota:\n\(nota)\n---")
confere(nota.contains("As 2 worktrees desta aba vão para a lixeira:"), "nota: cabeçalho no plural")
confere(nota.contains("(ramo fix/x; 3 arquivos alterados; 1 commit só nela; ainda rodando dentro: php)"), "nota: fatos do item")
confere(nota.contains("• ~/projetos/wt-solta (sem ramo, em f00ba12)"), "nota: HEAD solta e ~")
confere(nota.contains("Fica onde está:\n• ~/projetos/wt-g — em uso pela aba “Relatório”"), "nota: mantida com motivo")
confere(nota.contains("não identifiquei a conversa"), "nota: aviso")
confere(nota.contains("Colocar de volta") && nota.contains("fora do git"), "nota: como recuperar, sem prometer a worktree")
confere(Worktrees.note(for: r, about: "deste workspace").contains("As 2 worktrees deste workspace vão para a lixeira:"), "nota: fala do que se fecha")
escreve("listar.json", ["versao": 1, "abas": [], "lixeira": [], "mantidas": [], "avisos": []])
confere(Worktrees.note(for: Worktrees.list([.init(workspace: "w", tabs: [1])])) == "", "nota: nada a dizer quando não há worktree")
escreve("listar.json", ["versao": 2])
if case .failed = Worktrees.list([.init(workspace: "w", tabs: [1])]) { confere(true, "versão desconhecida: falha dita") } else { confere(false, "versão desconhecida: falha dita") }
escreve("listar.json", ["versao": 1, "ok": false, "motivo": "o git não respondeu a tempo"])
if case .failed(let why) = Worktrees.list([.init(workspace: "w", tabs: [1])]) {
    confere(why == "o git não respondeu a tempo", "recusa do listar: falha dita com o motivo do núcleo", why)
} else { confere(false, "recusa do listar: falha dita, não lista vazia") }
escreve("listar.json", ["versao": 1]); escreve("demora-listar.json", 5)
let t0 = Date()
let lento = Worktrees.list([.init(workspace: "w", tabs: [1])], deadline: 1)
if case .failed(let why) = lento { confere(Date().timeIntervalSince(t0) < 4 && why.contains("passou de"), "prazo estourado: falha em ~2,5 s", why) } else { confere(false, "prazo estourado") }
confere(Worktrees.note(for: lento).contains("nenhuma será movida"), "prazo estourado: diálogo diz que nada move")
try? fm.removeItem(atPath: dir + "/demora-listar.json")

// mover de verdade para a Lixeira
try! fm.createDirectory(atPath: wt, withIntermediateDirectories: true)
try! "x".write(toFile: wt + "/arquivo.txt", atomically: true, encoding: .utf8)
let recusa = dir + "/repo-wt-recusa-\(getpid())"
try! fm.createDirectory(atPath: recusa, withIntermediateDirectories: true)
let espera = dir + "/repo-wt-espera-\(getpid())"
try! fm.createDirectory(atPath: espera, withIntermediateDirectories: true)
escreve("preparar-" + (recusa as NSString).lastPathComponent + ".json", [["versao":1,"ok":false,"motivo":"travada: teste"]])
// a primeira recusa sai com 1, como o contrato manda dizer um "não": é uma
// resposta, lida como a de saída 0, e não uma falha
escreve("preparar-" + (espera as NSString).lastPathComponent + ".json", [["versao":1,"ok":false,"motivo":"processo vivo","processos":[["pid":1]],"_saida":1], ["versao":1,"ok":true]])
let dorminhoco = Process(); dorminhoco.executableURL = URL(fileURLWithPath: "/bin/sleep"); dorminhoco.arguments = ["1.5"]; try! dorminhoco.run()
let pid = dorminhoco.processIdentifier
setenv("FALSO_PID", String(pid), 1)
let item = { (p: String) -> [String: Any] in ["caminho": p] }
escreve("listar.json", ["versao": 1, "abas": [["alvo": "w:1", "pids": [pid]]], "lixeira": [item(wt), item(recusa), item(espera)]])
guard case .listing(let l2) = Worktrees.list([.init(workspace: "w", tabs: [1])]) else { print("FALHA"); exit(1) }
limpaChamadas()
let t1 = Date()
let problemas = Worktrees.trash(l2)
let dt = Date().timeIntervalSince(t1)
let cs = chamadas().map { ($0["argv"] as? [String]) ?? [] }
let primeiroPreparar = chamadas().first { ($0["argv"] as? [String])?.first == "preparar" }
confere(dt >= 1.3 && primeiroPreparar?["pid_vivo"] as? Bool == false, "espera o pid da aba morrer antes de mover", "\(dt) \(String(describing: primeiroPreparar))")
confere(!fm.fileExists(atPath: wt), "a worktree saiu do lugar")
let naLixeira = NSHomeDirectory() + "/.Trash/" + (wt as NSString).lastPathComponent
confere(fm.fileExists(atPath: naLixeira + "/arquivo.txt"), "e está na Lixeira com o conteúdo")
confere(cs.contains(["concluir", wt, naLixeira]) || cs.contains { $0.first == "concluir" && $0.count == 3 && $0[1] == wt && $0[2].hasPrefix(NSHomeDirectory() + "/.Trash/") }, "concluir recebe o destino real", "\(cs)")
confere(fm.fileExists(atPath: recusa), "recusada no preparar: fica")
confere(problemas.contains { $0.contains("travada: teste") }, "recusa vira problema dito", "\(problemas)")
confere(!cs.contains(["concluir", recusa]) && !cs.contains { $0.first == "concluir" && $0[1] == recusa }, "recusada: sem concluir")
confere(!fm.fileExists(atPath: espera), "processo vivo: tentou de novo e moveu")
confere(cs.filter { $0 == ["preparar", espera] }.count == 2, "processo vivo: preparou duas vezes", "\(cs)")
confere(problemas.count == 1, "só um problema", "\(problemas)")
print(problemas)
// pid que não morre: nada se move
let teimoso = Process(); teimoso.executableURL = URL(fileURLWithPath: "/bin/sleep"); teimoso.arguments = ["60"]; try! teimoso.run()
let fica = dir + "/repo-wt-fica-\(getpid())"
try! fm.createDirectory(atPath: fica, withIntermediateDirectories: true)
escreve("listar.json", ["versao": 1, "abas": [["alvo": "w:1", "pids": [teimoso.processIdentifier]]], "lixeira": [item(fica)]])
guard case .listing(let l3) = Worktrees.list([.init(workspace: "w", tabs: [1])]) else { print("FALHA"); exit(1) }
limpaChamadas()
let t3 = Date()
let p3 = Worktrees.trash(l3)
confere(fm.fileExists(atPath: fica) && p3.first?.contains("ainda está rodando") == true && !chamadas().contains { ($0["argv"] as? [String])?.first == "preparar" }, "conversa viva: nada move e diz por quê", "\(p3)")
confere(Date().timeIntervalSince(t3) >= 19, "conversa viva: esperou os 20 s")
teimoso.terminate(); try? fm.removeItem(atPath: fica)
// zumbi conta como morto
var z: pid_t = 0
let argv: [UnsafeMutablePointer<CChar>?] = [strdup("/usr/bin/true"), nil]
posix_spawn(&z, "/usr/bin/true", nil, nil, argv, nil)
Thread.sleep(forTimeInterval: 0.5)
confere(kill(z, 0) == 0, "(o filho virou zumbi: kill(pid, 0) ainda responde)")
confere(Worktrees.waitForExit([z], upTo: 1).isEmpty, "zumbi conta como morto")
var st: Int32 = 0; waitpid(z, &st, 0)
// nasceu_ns vai para o preparar
let comId = dir + "/repo-wt-id-\(getpid())"
try! fm.createDirectory(atPath: comId, withIntermediateDirectories: true)
escreve("listar.json", ["versao": 1, "abas": [], "lixeira": [["caminho": comId, "nasceu_ns": 1790000000123456789]]])
guard case .listing(let l4) = Worktrees.list([.init(workspace: "w", tabs: [1])]) else { print("FALHA"); exit(1) }
limpaChamadas()
_ = Worktrees.trash(l4)
confere(chamadas().contains { ($0["argv"] as? [String]) == ["preparar", comId, "--nasceu", "1790000000123456789"] }, "preparar recebe o nascimento listado", "\(chamadas())")
try? fm.removeItem(atPath: NSHomeDirectory() + "/.Trash/" + (comId as NSString).lastPathComponent)
for p in [wt, espera] { try? fm.removeItem(atPath: NSHomeDirectory() + "/.Trash/" + (p as NSString).lastPathComponent) }
try? fm.removeItem(atPath: recusa)
// indexar: o que as conversas criam, anotado pelo núcleo
limpaChamadas()
confere(Worktrees.index() == nil, "indexar: respondido")
confere(chamadas().last.map { ($0["argv"] as? [String]) == ["indexar", "--json"] && ($0["grupo"] as? String) == "worktrees" } == true,
        "indexar: keep worktrees indexar --json", "\(chamadas())")
print("\(casos - falhas)/\(casos) ok"); exit(falhas == 0 ? 0 : 1)
