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

// The coloured screen, as the daemon sends it (`\r\n` lines). The box of a new
// conversation, read off Claude Code 2.1.305 on 20/10/2026.
func coloured(_ prompt: String) -> String {
    ["\u{1b}[38;2;215;119;87malgum texto\u{1b}[0m", rule, prompt, rule, "  ⏵⏵ bypass permissions on"]
        .joined(separator: "\r\n")
}
let suggestion = "❯\u{a0}\u{1b}[0m\u{1b}[2mTry \"fix lint errors\"\u{1b}[0m"
check("the plain screen takes the dim suggestion for a draft",
      ClaudeStatusReader.promptIsEmpty(onScreen: screen(prompt: "❯\u{a0}Try \"fix lint errors\"")), false)
check("a dim suggestion is an empty prompt",
      ClaudeStatusReader.promptIsEmpty(onColouredScreen: coloured(suggestion)), true)
check("a coloured draft is not",
      ClaudeStatusReader.promptIsEmpty(onColouredScreen: coloured("❯\u{a0}\u{1b}[38;2;2;2;2mescrevendo\u{1b}[0m")), false)
check("dim ended by 22 shows what follows",
      ClaudeStatusReader.promptIsEmpty(onColouredScreen: coloured("❯\u{a0}\u{1b}[2mTry\u{1b}[22mx")), false)
check("an empty coloured box",
      ClaudeStatusReader.promptIsEmpty(onColouredScreen: coloured("❯\u{a0}\u{1b}[0m")), true)

let trust = """
 Accessing workspace:

 /Users/thiago/projetos/novo

 Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your
 team). If not, take a moment to review what's in this folder first.

 ❯ No, exit
   Yes, I trust this folder

 Enter to confirm · Esc to cancel
"""
check("the trust question, the pointer on the no", ClaudeTrustDialog.pointer(onScreen: trust) == .onNo, true)
check("the trust question, the pointer on the yes",
      ClaudeTrustDialog.pointer(onScreen: trust.replacingOccurrences(of: "❯ No, exit", with: "  No, exit")
          .replacingOccurrences(of: "  Yes, I trust", with: "❯ Yes, I trust")) == .onYes, true)
check("the words quoted in a conversation",
      ClaudeTrustDialog.pointer(onScreen: "ele disse: Yes, I trust this folder e No, exit\n" + screen()) == nil, true)
check("an ordinary screen", ClaudeTrustDialog.pointer(onScreen: screen()) == nil, true)

// Codex 0.162.1, as it was on screen (12/10).
let codexIdle = """
  >_ OpenAI Codex (v0.162.1)
     ~/Developer/keep

• Model changed to gpt-6-astra max

› Ask Codex to do anything

  GPT-6-Astra max · ~/Developer/keep
  ← for agents · ? for shortcuts
"""
let codexSaid = CodexStatusReader.read(onScreen: codexIdle)
check("Codex: the model under the prompt", codexSaid?.model, "GPT-6-Astra")
check("Codex: the effort under the prompt", codexSaid?.effort, "max")
check("Codex: a hint is an empty prompt", CodexStatusReader.promptIsEmpty(onScreen: codexIdle), true)
check("Codex: a draft is not",
      CodexStatusReader.promptIsEmpty(onScreen: codexIdle.replacingOccurrences(of: "Ask Codex to do anything", with: "conserte o teste")), false)
check("Codex: with the footer out of sight, the last change above",
      CodexStatusReader.read(onScreen: "• Model changed to gpt-6-sol high\n• algo\n")?.effort, "high")
check("Codex: a plain sentence is no model",
      CodexStatusReader.read(onScreen: "› \n  esc to interrupt · 1 background terminal\n") == nil, true)

let codexModels = """
  Select Model and Effort

  1. GPT-6.1-Sol            Latest workhorse model for coding and everyday work.
› 2. GPT-6-Astra (current)  Frontier intelligence for the most demanding work.
  3. GPT-6-Sol              Previous generation workhorse model.
  4. GPT-6-Luna             Fast and affordable model for easier tasks.
  5. GPT-5.6-Sol            Older generation workhorse model.

  enter select · esc back
"""
check("picker: GPT-6-Astra is 2", CodexPicker.number(of: "GPT-6-Astra", onScreen: codexModels), 2)
check("picker: GPT-6-Sol is 3", CodexPicker.number(of: "GPT-6-Sol", onScreen: codexModels), 3)
check("picker: GPT-6.1-Sol is 1", CodexPicker.number(of: "GPT-6.1-Sol", onScreen: codexModels), 1)
check("picker: a name must end where the label does", CodexPicker.number(of: "GPT-6", onScreen: codexModels), nil)
let codexLevels = """
  Select Reasoning Level for GPT-6-Astra

  1. Low                        Fast responses with lighter reasoning
  2. Medium (default)           Balances speed and reasoning depth for everyday tasks
  3. High                       Greater reasoning depth for complex problems
  4. Extra high                 Extra high reasoning depth for complex problems
› 5. More reasoning… (current)  Max and Ultra consume usage limits faster

  enter select · esc back
"""
check("picker: High is 3", CodexPicker.number(of: CodexPicker.label(ofEffort: "high"), onScreen: codexLevels), 3)
check("picker: xhigh is Extra high, 4", CodexPicker.number(of: CodexPicker.label(ofEffort: "xhigh"), onScreen: codexLevels), 4)
check("picker: Max is not on the first screen", CodexPicker.number(of: CodexPicker.label(ofEffort: "max"), onScreen: codexLevels), nil)
check("picker: More reasoning is 5", CodexPicker.number(of: CodexPicker.moreReasoning, onScreen: codexLevels), 5)
let codexAdvanced = """
  Advanced Reasoning
  ⚠ Consumes usage limits faster

› 1. Max (current)  For difficult problems when quality matters more than speed · higher usage
  2. Ultra          For demanding work using multiple agents · highest usage

  enter default · s session · esc back
"""
check("picker: Ultra is 2", CodexPicker.number(of: CodexPicker.label(ofEffort: "ultra"), onScreen: codexAdvanced), 2)

let cachePath = NSTemporaryDirectory() + "keep-status-test-cache-\(getpid()).json"
try! """
{"models": [
 {"slug": "gpt-6-astra", "display_name": "GPT-6-Astra", "visibility": "list", "priority": 2,
  "default_reasoning_level": "medium", "supported_reasoning_levels": [{"effort": "low"}, {"effort": "max"}]},
 {"slug": "gpt-reserve", "display_name": "GPT-Reserve", "visibility": "hide", "priority": 1},
 {"slug": "gpt-6.1-sol", "display_name": "GPT-6.1-Sol", "visibility": "list", "priority": 1,
  "default_reasoning_level": "low", "supported_reasoning_levels": [{"effort": "low"}]}
]}
""".write(toFile: cachePath, atomically: true, encoding: .utf8)
let cached = CodexStatusReader.models(cache: cachePath)
try? FileManager.default.removeItem(atPath: cachePath)
check("cache: listed models in Codex's order", cached.map(\.slug).joined(separator: ","), "gpt-6.1-sol,gpt-6-astra")
check("cache: a model's efforts", cached.last?.efforts.joined(separator: ","), "low,max")
check("cache: found by its name on screen", CodexStatusReader.model(named: "GPT-6-Astra", in: cached)?.slug, "gpt-6-astra")

// Claude Code 2.1.296's pickers, as they were on screen (12/10).
let claudeModels = """
   Select model
   Switch between Claude models. Your pick becomes the default for new sessions. For other/previous model names,
   specify with --model.
     1.  Default (recommended)  Fable 5.1
     2.  Opus 5.5               For complex work and everyday tasks
     3.  Fable 5.1              For your toughest challenges
   ❯ 4.  Sonnet 5.5 ✔           Most efficient for simpler tasks
     5.  Haiku 5.5              Fastest for quick answers
     6.  Haiku 4.5              Fastest for quick answers
   ↓ 10. Opus 4.8               Best for everyday, complex tasks
   ○ Low effort ←/→ to adjust
   Enter to set as default · s to use this session only · Esc to cancel
"""
check("Claude's model picker: the pointed item", ClaudePicker.items(onScreen: claudeModels).first(where: \.pointed)?.label, "Sonnet 5.5")
check("Claude's model picker: up two to Opus 5.5", ClaudePicker.steps(to: "Opus 5.5", onScreen: claudeModels).map(String.init), "-2")
check("Claude's model picker: down one to Haiku 5.5", ClaudePicker.steps(to: "Haiku 5.5", onScreen: claudeModels).map(String.init), "1")
check("Claude's model picker: Fable 5.1 is its own item, not the default's", ClaudePicker.steps(to: "Fable 5.1", onScreen: claudeModels).map(String.init), "-1")
check("Claude's model picker: the 1M Opus has no item", ClaudePicker.label(forModel: "claude-opus-5-5[1m]"), nil)
let claudeEffort = """
   Effort
                             Faster                             Smarter
                             ─▲────────────────────────────────────────      Ultracode  off
                             low     medium     high     xhigh      max      Tab to toggle
   ←/→ to adjust · Enter to confirm · s for this session only · Esc to cancel
"""
check("Claude's effort picker: on low, four right to max", ClaudePicker.effortSteps(to: "max", onScreen: claudeEffort).map(String.init), "4")
check("Claude's effort picker: on low, none to low", ClaudePicker.effortSteps(to: "low", onScreen: claudeEffort).map(String.init), "0")
let onHigh = claudeEffort.replacingOccurrences(of: "─▲──────────────────", with: "───────────────────▲")
check("Claude's effort picker: on high, one left to medium", ClaudePicker.effortSteps(to: "medium", onScreen: onHigh).map(String.init), "-1")
check("the switch question", ClaudePicker.asksToSwitch(onScreen: "Switch model?\n  the full history gets re-read\n ❯ 1. Yes, switch to Sonnet 5.5\n   2. No"), true)
exit(failures == 0 ? 0 : 1)
