import Foundation

var failures = 0
func check<T: Equatable>(_ name: String, _ got: T, _ want: T) {
    if got == want { print("ok   \(name)") } else {
        failures += 1
        print("FAIL \(name): got \(got), wanted \(want)")
    }
}

let rule = String(repeating: "─", count: 40)
func screen(prompt: String = "❯ ", footer: [String] = []) -> String {
    (["algum texto", rule, prompt, rule, "  ⏵⏵ bypass permissions on"] + footer).joined(separator: "\n")
}

let json = #"{"model":{"id":"claude-opus-5-5[1m]","display_name":"Opus 5.5 (1M context)"},"effort":{"level":"max"}}"#
let parsed = ClaudeStatusReader.parse(Data(json.utf8))
check("model id", parsed?.modelID, "claude-opus-5-5[1m]")
check("model name, shortened", parsed?.modelName, "Opus 5.5 (1M)")
check("effort", parsed?.effort, "max")
let plain = ClaudeStatusReader.parse(Data(#"{"model":{"id":"claude-sonnet-5-5","display_name":"Sonnet 5.5"}}"#.utf8))
check("no effort", plain?.effort, nil)
check("not a status", ClaudeStatusReader.parse(Data("{}".utf8)) == nil, true)
check("not json", ClaudeStatusReader.parse(Data("x".utf8)) == nil, true)

check("no agents", ClaudeStatusReader.agents(onScreen: screen()), 0)
check("agents listed", ClaudeStatusReader.agents(onScreen: screen(footer: [
    "", "  ⏺ main", "  ◯ general-purpose  Reading", "  ◯ Explore  Scanning"])), 2)
check("agents counted when folded", ClaudeStatusReader.agents(onScreen: screen(footer: [
    "  ⏵⏵ bypass · 3 local agents · ↓ to manage"])), 3)
check("a ◯ above the prompt is not an agent", ClaudeStatusReader.agents(onScreen: "◯ old\n" + screen()), 0)

check("empty prompt", ClaudeStatusReader.promptIsEmpty(onScreen: screen()), true)
check("a draft", ClaudeStatusReader.promptIsEmpty(onScreen: screen(prompt: "❯ escrevendo")), false)
check("no box", ClaudeStatusReader.promptIsEmpty(onScreen: "só texto"), false)

let trust = """
 Accessing workspace:

 /Users/thiago/projetos/novo

 Quick safety check: is this a project you created or one you trust?

 ❯ 1. Yes, I trust this folder
   2. No, exit

 Enter to confirm · Esc to cancel
"""
check("the trust question, yes chosen", ClaudeTrustDialog.isAsking(onScreen: trust), true)
check("the trust question, no chosen",
      ClaudeTrustDialog.isAsking(onScreen: trust.replacingOccurrences(of: "❯ 1.", with: "  1.").replacingOccurrences(of: "  2. No", with: "❯ 2. No")), false)
check("the words quoted in a conversation",
      ClaudeTrustDialog.isAsking(onScreen: "ele disse: Yes, I trust this folder\n" + screen()), false)
check("an ordinary screen", ClaudeTrustDialog.isAsking(onScreen: screen()), false)
exit(failures == 0 ? 0 : 1)
