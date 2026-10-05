import AppKit
import SwiftUI

/// Every signed-in account's usage, for every window's footer — as the core
/// measures it (`keep ia uso`).
///
/// One for the app, not one per window: the answer is the same in all of
/// them. The core keeps the cadence and the pauses — five minutes between
/// readings of an account, sooner after a failure, a "Medir agora" at most
/// every 15 s, a service's "too many requests" waited out even by a click —
/// so asking it every minute costs the services nothing it would not have
/// spent anyway, and keeps a failure retried sooner than a success repeated.
///
/// Which accounts there are, and in what order, is watched apart from that:
/// a glance at the files every two seconds (`AIAccounts.signature`), which
/// reads nothing over the network, so that a login just made or an order
/// just changed shows at once — the core is then asked what it has without
/// asking any service, and only an account that is new is measured on the
/// spot.
///
/// The work is the poller's shape: the core runs off the main thread, one
/// question at a time, and only its answer is handed back to the main actor.
@MainActor
final class UsageMonitor: ObservableObject {
    static let shared = UsageMonitor()

    @Published private(set) var lines: [AccountUsage] = []
    /// A round somebody asked for is running.
    @Published private(set) var measuring = false
    /// Whether there is a `keep` to ask (the one inside the app). Without it
    /// there is no order to change and no login to open, and the footer
    /// offers neither.
    @Published private(set) var helperAvailable = false
    /// What the footer has to say for a moment: a change the core refused.
    @Published private(set) var notice: String?
    /// What the Jev can still spend, when this Mac has a key of its own for
    /// OpenRouter; nil when it has none, and the footer says nothing of it.
    @Published private(set) var jev: JevLine?

    private var timer: Timer?
    private var glance: Timer?
    private var inFlight = false
    private var lastClick = Date.distantPast
    /// A click that came while a question was out, owed a round of its own.
    private var clickPending = false
    /// A round of the due accounts owed when the question out lands.
    private var tickPending = false
    /// Accounts found while a question was out, owed a reading as soon as it
    /// lands.
    private var owed: Set<String> = []
    /// When each line first showed, and when the answer its figures came
    /// from was asked for. Answers overtake each other — a round of readings
    /// takes seconds, a look at what the core has takes a moment — and one
    /// asked for earlier is laid only over what is older than it: it neither
    /// takes away an account that showed after it was asked for, nor puts
    /// back figures a later answer replaced.
    private var arrived: [String: Date] = [:]
    private var figuresAsOf: [String: Date] = [:]

    /// The files the logins live in, as last read.
    private var signature: [String]?
    private var looking = false
    private var lookAgain = false
    /// The next look asks the core whether or not the files seem to have
    /// changed.
    private var rereading = false

    /// Moves of the order asked of the core and not yet answered.
    private var movesPending = 0
    /// The order being shown while the core catches up with the arrows — by
    /// line, first to last — and until when it holds against its answers.
    private var heldOrder: [String]?
    private var heldUntil = Date.distantPast
    /// One move at a time, in the order they were clicked: each is a step
    /// from wherever the one before left the order.
    private let orderQueue = DispatchQueue(label: "keep.usage.order", qos: .userInitiated)
    private var noticeTicket = 0

    nonisolated static let tick: TimeInterval = 60
    nonisolated static let glanceEvery: TimeInterval = 2
    /// How long an order shown ahead of the core is held against what it
    /// answers, after the last arrow. The core writes within a second; what
    /// this covers is an answer asked for just before it did.
    nonisolated static let holdFor: TimeInterval = 10

    private let environment = ProcessInfo.processInfo.environment

    /// Where the logins are watched. A test points this at a stand-in home,
    /// and the core is told the same (`KeepCLI.environment`), so the real
    /// logins are never opened.
    private var home: URL {
        KeepCLI.madeUpHome(environment).map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser
    }

    func start() {
        guard timer == nil else { return }
        restore()
        helperAvailable = AIHelper.path != nil
        // What the core has first, then a round over it.
        look(thenMeasure: true)
        let timer = Timer(timeInterval: Self.tick, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.measure(clicked: false) }
        }
        // In the common modes as well, so a drag or an open menu does not
        // hold the next reading back.
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
        let glance = Timer(timeInterval: Self.glanceEvery, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.look() }
        }
        RunLoop.main.add(glance, forMode: .common)
        self.glance = glance
    }

    /// The footer's refresh button: every account, now — within reason.
    func measureNow() {
        guard Date().timeIntervalSince(lastClick) > 15 else { return }
        guard !inFlight else {
            // Not dropped: run as soon as the question out lands.
            clickPending = true
            measuring = true
            return
        }
        lastClick = Date()
        measure(clicked: true)
    }

    // MARK: - which accounts, in what order

    /// A glance at the files the logins live in. Changed since the last one
    /// — a login added or renewed, the account the tabs run on switched,
    /// the order rewritten — the core is asked what it has, without asking
    /// any service, and whatever is new among the accounts measured now
    /// rather than at the next round.
    private func look(force: Bool = false, thenMeasure: Bool = false) {
        if force { rereading = true }
        guard !looking else {
            lookAgain = true
            return
        }
        looking = true
        let home = self.home
        let known = rereading ? nil : signature
        rereading = false
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let now = AIAccounts.signature(home: home)
            let helper = AIHelper.path != nil
            // Asked once per change of the files, answered or not: a core
            // that fails is asked again at the next change or the next tick,
            // never every two seconds. A change of the Jev's files alone — a
            // decision it just made — asks only about the Jev.
            let asks = helper && now != known
            let isJev = { (part: String) in part.hasPrefix("jev ") }
            let accountsChanged = known.map { $0.filter { !isJev($0) } } != now.filter { !isJev($0) }
            let jevChanged = known.map { $0.filter(isJev) } != now.filter(isJev)
            let askedAt = Date()
            let answer = asks && accountsChanged ? try? AIHelper.usage(.cached).get() : nil
            // The Jev as the core has it: a connection just made, a key gone,
            // or what a decision just spent, from the book.
            let credit = asks && jevChanged ? try? AIHelper.jev(.cached).get() : nil
            DispatchQueue.main.async {
                guard let self else { return }
                self.looking = false
                if self.helperAvailable != helper { self.helperAvailable = helper }
                if asks { self.signature = now }
                if let credit { self.adoptJev(credit.jev) }
                if let answer {
                    let fresh = self.adopt(answer.linhas, askedAt: askedAt)
                    if !thenMeasure, !fresh.isEmpty {
                        Trace.log("usage", "found \(fresh.sorted().joined(separator: " ")); measuring now")
                        self.measure(clicked: false, only: fresh)
                    }
                }
                if thenMeasure { self.measure(clicked: false) }
                if self.lookAgain {
                    self.lookAgain = false
                    self.look()
                }
            }
        }
    }

    /// The accounts as the core had them when it was asked, at `askedAt`,
    /// each with what it last said — laid over what is shown only where the
    /// answer is the newer news. The ones that were not there before, or have
    /// just been given a token, are handed back, by the key the core knows
    /// them by: they are worth asking about now.
    @discardableResult
    private func adopt(_ answer: [AccountUsage], askedAt: Date) -> Set<String> {
        let previous = Dictionary(lines.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let answered = Set(answer.map(\.id))
        var merged = answer.map { line -> AccountUsage in
            guard let shown = previous[line.id], let asOf = figuresAsOf[line.id], asOf > askedAt
            else { return line }
            return shown
        }
        merged += lines.filter { !answered.contains($0.id) && (arrived[$0.id] ?? .distantPast) > askedAt }
        let now = Date()
        for line in merged where arrived[line.id] == nil { arrived[line.id] = now }
        for id in answered where (figuresAsOf[id] ?? .distantPast) <= askedAt { figuresAsOf[id] = askedAt }
        let kept = Set(merged.map(\.id))
        arrived = arrived.filter { kept.contains($0.key) }
        figuresAsOf = figuresAsOf.filter { kept.contains($0.key) }
        let next = holding(merged)
        if next != lines {
            lines = next
            remember()
        }
        // Before the first answer there is nothing to tell new from old:
        // the round that follows it measures whatever is due.
        guard !previous.isEmpty else { return [] }
        return Set(next.compactMap { line in
            guard let before = previous[line.id] else { return line.account.order }
            return before.account.hasToken != true && line.account.hasToken == true
                ? line.account.order : nil
        })
    }

    /// The order being held, laid over lines the core answered: an order
    /// changed by the arrows is shown before the core has written it, and an
    /// answer asked for in between must not put it back as it was.
    private func holding(_ next: [AccountUsage]) -> [AccountUsage] {
        guard let held = heldOrder else { return next }
        var rank: [String: Int] = [:]
        for (place, id) in held.enumerated() where rank[id] == nil { rank[id] = place }
        let placed = next.filter { rank[$0.id] != nil }.sorted { rank[$0.id]! < rank[$1.id]! }
        return placed + next.filter { rank[$0.id] == nil }
    }

    /// One place up (`-1`) or down (`1`) the order of priority.
    ///
    /// Shown at once and asked of the core behind it, one move at a time: it
    /// writes the order, and the tabs that follow it move on its next round.
    /// What it refuses is taken back — the order is asked again as the core
    /// has it — and said.
    func move(_ line: AccountUsage, by step: Int) {
        guard helperAvailable, step != 0,
              let at = lines.firstIndex(where: { $0.id == line.id }),
              lines.indices.contains(at + step)
        else { return }
        let key = lines[at].account.order
        let up = step < 0
        var next = lines
        next.insert(next.remove(at: at), at: at + step)
        lines = next
        heldOrder = next.map(\.id)
        heldUntil = Date().addingTimeInterval(Self.holdFor)
        movesPending += 1
        quiet()
        Trace.log("ia", "order: \(key) \(up ? "up" : "down"), shown now as "
            + next.map(\.account.order).joined(separator: " "))
        orderQueue.async { [weak self] in
            let answer = AIHelper.moveOrder(key, up: up)
            DispatchQueue.main.async { self?.moved(key, answer) }
        }
        releaseLater()
    }

    private func moved(_ key: String, _ answer: Result<[String], AIHelper.Problem>) {
        movesPending -= 1
        switch answer {
        case .success(let order):
            Trace.log("ia", "order: the core has \(order.joined(separator: " "))")
        case .failure(let problem):
            Trace.log("ia", "order: \(key) refused (\(problem.reason ?? "?")): \(problem.detail)")
            // Taken back by asking the order as the core has it — which also
            // keeps whatever an earlier move did get through.
            heldOrder = nil
            say("Não deu para mudar a ordem: \(problem.detail)")
            look(force: true)
        }
        if movesPending == 0 { releaseLater() }
    }

    /// Stop holding the order once the last move is answered and the hold
    /// has run out, and ask what the core says: its word is the last.
    private func releaseLater() {
        let until = heldUntil
        let wait = max(0, until.timeIntervalSinceNow) + 0.05
        DispatchQueue.main.asyncAfter(deadline: .now() + wait) { [weak self] in
            guard let self, self.heldUntil == until, self.movesPending == 0, self.heldOrder != nil
            else { return }
            self.heldOrder = nil
            self.look(force: true)
        }
    }

    /// Something said in the footer for a few seconds.
    private func say(_ text: String) {
        noticeTicket += 1
        let ticket = noticeTicket
        notice = text
        DispatchQueue.main.asyncAfter(deadline: .now() + 12) { [weak self] in
            guard let self, self.noticeTicket == ticket else { return }
            self.notice = nil
        }
    }

    private func quiet() {
        noticeTicket += 1
        if notice != nil { notice = nil }
    }

    // MARK: - how much each has spent

    /// One question to the core: the accounts due (`clicked`: all of them,
    /// as "Medir agora"), or `only` these, by their keys, one after another.
    /// Its answer is laid onto the footer whole — the core keeps every
    /// reading, every failure and every pause.
    private func measure(clicked: Bool, only: Set<String>? = nil) {
        guard AIHelper.path != nil else { return }
        guard !inFlight else {
            if let only { owed.formUnion(only) } else if !clicked { tickPending = true }
            return
        }
        inFlight = true
        if clicked { measuring = true }
        let questions: [AIHelper.Measure] = only.map { $0.sorted().map { .only($0) } }
            ?? [clicked ? .now : .due]
        DispatchQueue.global(qos: .utility).async { [weak self] in
            var last: Result<UsageAnswer, AIHelper.Problem>?
            var credit: Result<JevAnswer, AIHelper.Problem>?
            var askedAt = Date()
            for question in questions {
                let started = Date()
                askedAt = started
                let answer = AIHelper.usage(question)
                let took = Int(Date().timeIntervalSince(started) * 1000)
                switch answer {
                case .success(let answer):
                    Trace.log("usage", "uso \(question.label): \(took)ms, "
                        + "\(answer.linhas.filter { $0.reading != nil }.count)/\(answer.linhas.count) lidas")
                case .failure(let problem):
                    Trace.log("usage", "uso \(question.label) falhou em \(took)ms: \(problem.detail)")
                }
                last = answer
            }
            // The Jev's credit is asked in the rounds of everybody's usage
            // and not in the ones for a login just made: the core keeps its
            // own cadence, as it does for the accounts.
            if only == nil {
                let started = Date()
                credit = AIHelper.jev(clicked ? .now : .due)
                let took = Int(Date().timeIntervalSince(started) * 1000)
                switch credit {
                case .success(let answer)?:
                    Trace.log("usage", "jev: \(took)ms, " + (answer.jev == nil ? "sem chave" : "lido"))
                case .failure(let problem)?:
                    Trace.log("usage", "jev falhou em \(took)ms: \(problem.detail)")
                case nil: break
                }
            }
            DispatchQueue.main.async {
                guard let self else { return }
                self.inFlight = false
                self.measuring = false
                if case .success(let answer)? = last { self.adopt(answer.linhas, askedAt: askedAt) }
                if case .success(let answer)? = credit { self.adoptJev(answer.jev) }
                if !self.owed.isEmpty {
                    let owed = self.owed
                    self.owed = []
                    self.measure(clicked: false, only: owed)
                } else if self.clickPending {
                    self.clickPending = false
                    self.measureNow()
                } else if self.tickPending {
                    self.tickPending = false
                    self.measure(clicked: false)
                }
            }
        }
    }

    /// The Jev's line as the core had it: none when there is no key, which
    /// takes the line off the footer.
    private func adoptJev(_ line: JevLine?) {
        guard line != jev else { return }
        jev = line
        rememberJev()
    }

    // MARK: - across relaunches

    /// The lines kept in the app's defaults — no token, only figures — so a
    /// relaunch shows the last figures at once, before the core has said
    /// anything. Not when a test points the monitor at a home of its own.
    ///
    /// In Foundation's own encoding, as it always was: the kit's priority
    /// script reads the footer from here (`usageFooterLines`).
    private var persists: Bool { KeepCLI.madeUpHome(environment) == nil }
    private static let linesKey = "usageFooterLines"
    private static let jevKey = "usageFooterJev"

    private func restore() {
        guard persists else { return }
        let defaults = UserDefaults.standard
        if let data = defaults.data(forKey: Self.linesKey),
           let saved = try? JSONDecoder().decode([AccountUsage].self, from: data) {
            lines = saved
        }
        if let data = defaults.data(forKey: Self.jevKey),
           let saved = try? JSONDecoder().decode(JevLine.self, from: data) {
            jev = saved
        }
    }

    private func rememberJev() {
        guard persists else { return }
        if let jev, let data = try? JSONEncoder().encode(jev) {
            UserDefaults.standard.set(data, forKey: Self.jevKey)
        } else {
            UserDefaults.standard.removeObject(forKey: Self.jevKey)
        }
    }

    private func remember() {
        guard persists else { return }
        if let data = try? JSONEncoder().encode(lines) {
            UserDefaults.standard.set(data, forKey: Self.linesKey)
        }
    }
}

// MARK: - the footer

/// The usage footer at the bottom of the sidebar: every signed-in account,
/// its windows as bars, and when each starts over.
///
/// Set apart by a rule and pinned under the list (DESIGN.md: "the footer is
/// set apart by a rule"), quiet until something is near its limit: the bars
/// are the ink of resting text, and only a window past 75% takes a colour —
/// amber, then red from 90, the thresholds of the Clínica's panel this
/// mirrors. The figure is always written beside the bar, so the colour is
/// never the only thing saying it.
///
/// The accounts are listed in the order of priority, and — when there is a
/// `keep` to ask — each with its place in it, arrows to move one up or
/// down, and a "+" in the heading to sign in to another.
struct UsageFooter: View {
    @ObservedObject var monitor: UsageMonitor
    /// This window's channel to the session: a login opened from here opens
    /// in the workspace this window is in.
    let dispatch: (Intent) -> Void
    var selectedAccount: String? = nil
    /// Quiet at rest, legible under the pointer (PRODUCT.md).
    @State private var hoverFold = false
    @State private var hoverMeasure = false
    @State private var hoverArrow: String?
    /// Folded, it is one line per account. A convenience of this Mac's, so it
    /// lives in the app's defaults rather than in any window's record.
    @AppStorage("usageFooterFolded") private var folded = false

    var body: some View {
        if monitor.lines.isEmpty && !monitor.helperAvailable {
            // No login found and nothing to sign in with: no footer, not an
            // empty box.
            Color.clear.frame(height: 0)
        } else {
            // Re-read the clock every half minute, so "4h10" keeps counting
            // down between readings without anybody publishing anything.
            TimelineView(.periodic(from: .now, by: 30)) { context in
                footer(now: context.date)
            }
        }
    }

    private var numbers: Font { Font(GhosttyApp.shared.terminalFont(size: 11)) }

    private func footer(now: Date) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Rectangle()
                .fill(UsageInk.wash(0.08))
                .frame(height: 1)
            header
                .padding(.horizontal, 17)
                .padding(.top, 7)
                .padding(.bottom, folded ? 4 : 5)
            if let notice = monitor.notice {
                Text(notice)
                    .font(.system(size: 10))
                    .foregroundStyle(UsageInk.attention)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 17)
                    .padding(.bottom, 6)
            }
            VStack(alignment: .leading, spacing: folded ? 3 : 8) {
                ForEach(Array(monitor.lines.enumerated()), id: \.element.id) { index, line in
                    if folded {
                        compact(line, at: index)
                    } else {
                        block(line, at: index, now: now)
                    }
                }
                if let jev = monitor.jev {
                    if folded {
                        jevCompact(jev, now: now)
                    } else {
                        jevBlock(jev, now: now)
                    }
                }
            }
            .padding(.horizontal, 17)
            // An account that moves in the order is seen to move: the
            // arrows' answer, in the footer's own time.
            .animation(.easeOut(duration: 0.18), value: monitor.lines.map(\.id))
        }
        .padding(.bottom, 10)
        .animation(.easeOut(duration: 0.15), value: folded)
    }

    private var header: some View {
        HStack(spacing: 4) {
            Button {
                folded.toggle()
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 8, weight: .semibold))
                        .rotationEffect(.degrees(folded ? 0 : 90))
                    Text("Consumo de IA")
                        .font(.system(size: 11, weight: .semibold))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(hoverFold ? UsageInk.inkResting : UsageInk.inkFaint)
            .onHover { hoverFold = $0 }
            .accessibilityLabel("Consumo de IA")
            .help(folded ? "Mostrar as janelas de cada conta" : "Uma linha por conta")
            Spacer(minLength: 4)
            if monitor.helperAvailable {
                // Signing in is a menu — either AI, and the Jev's key on
                // OpenRouter — and the login itself is the core's: it opens
                // in a tab of this window's workspace.
                MenuGlyph(
                    symbol: "plus", pointSize: 9, weight: .semibold,
                    label: "Entrar em outra conta", identifier: "usage-signin",
                    help: "Entrar em outra conta",
                    resting: UsageInk.faintColor, lit: UsageInk.restingColor,
                    menu: {
                        AccountMenu.signIn(jevConnected: monitor.jev != nil) { service in
                            dispatch(.signIn(service))
                        }
                    })
                    .frame(width: 16, height: 14)
            }
            Button {
                monitor.measureNow()
            } label: {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 9, weight: .semibold))
                    .frame(width: 16, height: 14)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(hoverMeasure ? UsageInk.inkResting : UsageInk.inkFaint)
            .onHover { hoverMeasure = $0 }
            .opacity(monitor.measuring ? 0.35 : 1)
            .help("Medir agora")
            .accessibilityLabel("Medir agora")
        }
        .foregroundStyle(UsageInk.inkFaint)
    }

    /// "Claude · principal": the service and the order's name for the account.
    private func title(_ line: AccountUsage) -> String { line.account.name }

    /// "2 · Claude · reserva": the same, after its place in the order, which
    /// is what the arrows beside it change. The place is written only when
    /// there is a `keep` to keep the order: without it the footer is
    /// as it always was.
    private func heading(_ text: String, _ line: AccountUsage, at index: Int) -> Text {
        let name = Text(text).foregroundColor(line.account.isUsed(by: selectedAccount) ? UsageInk.ink : UsageInk.inkResting)
        guard monitor.helperAvailable else { return name }
        return Text("\(index + 1) · ").foregroundColor(UsageInk.inkFaint) + name
    }

    private func block(_ line: AccountUsage, at index: Int, now: Date) -> some View {
        let stale = line.problem != nil
            || line.measuredAt.map { now.timeIntervalSince($0) > 15 * 60 } ?? true
        return VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 5) {
                HStack(spacing: 5) {
                    heading(title(line), line, at: index)
                        .font(numbers)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if line.account.isUsed(by: selectedAccount) {
                        Circle()
                            .fill(UsageInk.live)
                            .frame(width: 5, height: 5)
                            .help("Conta usada nesta aba")
                            .accessibilityLabel("Conta usada nesta aba")
                            .accessibilityIdentifier("usage-active-\(line.account.order)")
                    }
                    Spacer(minLength: 0)
                }
                .help(accountHelp(line, now: now))
                arrows(line, at: index)
            }

            // Which login this is, written out: the slot's name says which
            // one the order means, the address says which one the service does.
            if let email = line.account.email {
                Text(email)
                    .font(.system(size: 10.5))
                    .foregroundStyle(UsageInk.inkResting)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(accountHelp(line, now: now))
            }

            if let reading = line.reading {
                ForEach(Array(reading.windows.enumerated()), id: \.offset) { _, window in
                    bar(window, of: line, now: now)
                }
                .opacity(stale ? 0.55 : 1)
            }

            if let note = note(line, now: now) {
                Text(note)
                    .font(.system(size: 10))
                    .foregroundStyle(line.reading?.limitReached == true ? UsageInk.critical : UsageInk.inkFaint)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// ▲ and ▼: one place up or down the order of priority. The first
    /// has nowhere up to go and the last nowhere down, and each keeps the
    /// other's place, so the arrows stand in one column.
    ///
    /// Beside the line rather than part of it: the folded line is one element
    /// to the accessibility tree, and anything inside it could not be pressed.
    @ViewBuilder
    private func arrows(_ line: AccountUsage, at index: Int) -> some View {
        if monitor.helperAvailable {
            HStack(spacing: 1) {
                arrow(line, up: true, shown: index > 0)
                arrow(line, up: false, shown: index < monitor.lines.count - 1)
            }
        }
    }

    private func arrow(_ line: AccountUsage, up: Bool, shown: Bool) -> some View {
        let key = "\(up ? "up" : "down") \(line.id)"
        return Button {
            monitor.move(line, by: up ? -1 : 1)
        } label: {
            Image(systemName: up ? "chevron.up" : "chevron.down")
                .font(.system(size: 7.5, weight: .bold))
                .frame(width: 12, height: 12)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(hoverArrow == key ? UsageInk.inkResting : UsageInk.inkFaint)
        .onHover { inside in
            hoverArrow = inside ? key : (hoverArrow == key ? nil : hoverArrow)
        }
        .help(up ? "Subir na ordem de prioridade" : "Descer na ordem de prioridade")
        .accessibilityLabel("\(up ? "Subir" : "Descer") \(title(line))")
        .opacity(shown ? 1 : 0)
        .allowsHitTesting(shown)
        .accessibilityHidden(!shown)
    }

    private func bar(_ window: UsageWindow, of line: AccountUsage, now: Date) -> some View {
        let level = UsageText.level(window.percent)
        let until = UsageText.until(window.resetsAt, now: now)
        return HStack(spacing: 6) {
            Text(window.label)
                .font(.system(size: 10))
                .foregroundStyle(UsageInk.inkResting)
                .lineLimit(1)
                // The middle: an additional limit's label ends in the window
                // ("… 5h", "… 7d") that tells two of them apart.
                .truncationMode(.middle)
                .frame(width: 34, alignment: .leading)
            GeometryReader { space in
                ZStack(alignment: .leading) {
                    Capsule().fill(UsageInk.wash(0.12))
                    Capsule()
                        .fill(UsageInk.fill(level))
                        .frame(width: max(2, space.size.width * min(window.percent, 100) / 100))
                }
            }
            .frame(height: 4)
            Text(UsageText.percent(window.percent))
                .font(numbers)
                .monospacedDigit()
                .foregroundStyle(level == .normal ? UsageInk.inkResting : UsageInk.fill(level))
                .frame(width: 34, alignment: .trailing)
            Text(until ?? "")
                .font(.system(size: 10))
                .foregroundStyle(UsageInk.inkFaint)
                .lineLimit(1)
                .frame(width: 40, alignment: .trailing)
        }
        .frame(height: 13)
        .help(barHelp(window, now: now))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            "\(title(line)) \(window.label) \(UsageText.percent(window.percent))"
                + (until.map { " reinicia em \($0)" } ?? ""))
    }

    /// Folded: the account — by its address, which is what tells two logins
    /// of one service apart — and the figures of its first two windows, each
    /// in its own colour: one amber figure must not paint its calm neighbour.
    private func compact(_ line: AccountUsage, at index: Int) -> some View {
        let windows = Array((line.reading?.windows ?? []).prefix(2))
        let figures = windows.map { UsageText.percent($0.percent) }.joined(separator: " · ")
        // The address alone, cut at its end: its start is what tells the
        // logins apart, and the service is in the tooltip.
        let name = line.account.email ?? title(line)
        return HStack(spacing: 5) {
            HStack(spacing: 5) {
                heading(name, line, at: index)
                    .font(numbers)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 4)
                compactFigures(windows)
                    .font(numbers)
                    .monospacedDigit()
                    .lineLimit(1)
                    .layoutPriority(1)
            }
            .frame(height: 14)
            .help(([title(line)] + windows.map { "\($0.title): \(UsageText.percent($0.percent))" })
                .joined(separator: "\n"))
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("\(line.account.engine.title) \(name) \(figures)")
            arrows(line, at: index)
        }
    }

    private func compactFigures(_ windows: [UsageWindow]) -> Text {
        guard !windows.isEmpty else { return Text("—").foregroundColor(UsageInk.inkFaint) }
        var text = Text("")
        for (index, window) in windows.enumerated() {
            if index > 0 { text = text + Text(" · ").foregroundColor(UsageInk.inkFaint) }
            let level = UsageText.level(window.percent)
            text = text + Text(UsageText.percent(window.percent))
                .foregroundColor(level == .normal ? UsageInk.inkResting : UsageInk.fill(level))
        }
        return text
    }

    // MARK: - the Jev's credit

    /// "Jev · OpenRouter": the credit the Jev spends, in two counters. The
    /// account: its bar is what is spent of what was bought, and the figure
    /// what is left. The Jev's key: with a ceiling of its own, the same of the
    /// ceiling; without one — it spends from the whole credit — what it has
    /// spent ("gasto"), its bar that share of the credit. Each bar is what is
    /// spent, as the others are.
    private func jevBlock(_ jev: JevLine, now: Date) -> some View {
        let stale = jev.problem != nil
            || jev.measuredAt.map { now.timeIntervalSince($0) > 15 * 60 } ?? true
        return VStack(alignment: .leading, spacing: 3) {
            Text("Jev · OpenRouter")
                .font(numbers)
                .foregroundStyle(UsageInk.inkResting)
                .lineLimit(1)
                .help(jevHelp(jev, now: now))
            if let credit = jev.credit {
                Group {
                    creditBar("conta", spent: credit.accountPercent, figure: credit.accountLeft, says: "restam",
                              help: "Crédito da conta no OpenRouter: \(UsageText.dollars(credit.used)) "
                                  + "gastos de \(UsageText.dollars(credit.total))")
                    if let keyPercent = credit.keyPercent, let keyLeft = credit.keyLeft, let limit = credit.keyLimit {
                        creditBar("chave", spent: keyPercent, figure: keyLeft, says: "restam",
                                  help: "Teto da chave do Jev: \(UsageText.dollars(credit.keyUsed ?? 0)) "
                                      + "gastos de \(UsageText.dollars(limit))")
                    } else if let keyUsed = credit.keyUsed {
                        creditBar("gasto", spent: credit.keySharePercent, figure: keyUsed, says: "gastou",
                                  help: "Gasto da chave do Jev, que não tem teto e usa todo o crédito da conta: "
                                      + UsageText.dollars(keyUsed)
                                      + (credit.usedToday.map { " (hoje \(UsageText.dollars($0)))" } ?? ""))
                    }
                }
                .opacity(stale ? 0.55 : 1)
            }
            if let note = jevNote(jev, now: now) {
                Text(note)
                    .font(.system(size: 10))
                    .foregroundStyle(UsageInk.inkFaint)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// One counter: `spent` is the bar, `figure` the dollars beside it, and
    /// `says` how the figure reads aloud ("restam", "gastou").
    private func creditBar(_ label: String, spent: Double, figure: Double, says: String, help: String) -> some View {
        let level = UsageText.level(spent)
        return HStack(spacing: 6) {
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(UsageInk.inkResting)
                .lineLimit(1)
                .frame(width: 34, alignment: .leading)
            GeometryReader { space in
                ZStack(alignment: .leading) {
                    Capsule().fill(UsageInk.wash(0.12))
                    Capsule()
                        .fill(UsageInk.fill(level))
                        .frame(width: max(2, space.size.width * min(spent, 100) / 100))
                }
            }
            .frame(height: 4)
            Text(UsageText.dollars(figure))
                .font(numbers)
                .monospacedDigit()
                .foregroundStyle(level == .normal ? UsageInk.inkResting : UsageInk.fill(level))
                .lineLimit(1)
                // "< US$ 0,00001", the longest a figure gets, in the 11-point face.
                .frame(width: 90, alignment: .trailing)
        }
        .frame(height: 13)
        .help(help)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Jev \(label): \(says) \(UsageText.dollars(figure))")
    }

    /// Folded: the Jev on one line, with what it can still spend — the
    /// smaller of the account's credit and its key's ceiling.
    private func jevCompact(_ jev: JevLine, now: Date) -> some View {
        let figure = jev.credit.map { UsageText.dollars($0.available) } ?? "—"
        let spent = jev.credit.map { max($0.accountPercent, $0.keyPercent ?? 0) } ?? 0
        let level = UsageText.level(spent)
        return HStack(spacing: 5) {
            Text("Jev · OpenRouter")
                .font(numbers)
                .foregroundStyle(UsageInk.inkResting)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 4)
            Text(figure)
                .font(numbers)
                .monospacedDigit()
                .foregroundStyle(level == .normal ? UsageInk.inkResting : UsageInk.fill(level))
                .lineLimit(1)
                .layoutPriority(1)
        }
        .frame(height: 14)
        .help(jevHelp(jev, now: now))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Jev OpenRouter restam \(figure)")
    }

    private func jevNote(_ jev: JevLine, now: Date) -> String? {
        var parts: [String] = []
        if let problem = jev.problem {
            parts.append(problem)
        } else if jev.credit == nil {
            parts.append("medindo…")
        }
        if let at = jev.measuredAt, now.timeIntervalSince(at) > 15 * 60 {
            parts.append("medido \(UsageText.ago(at, now: now))")
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func jevHelp(_ jev: JevLine, now: Date) -> String {
        var lines = ["Crédito que o Jev gasta no OpenRouter"]
        if let credit = jev.credit {
            lines.append("Pode gastar ainda: \(UsageText.dollars(credit.available))")
            lines.append("Conta: \(UsageText.dollars(credit.accountLeft)) de \(UsageText.dollars(credit.total))")
            if let left = credit.keyLeft, let limit = credit.keyLimit {
                lines.append("Chave: \(UsageText.dollars(left)) de \(UsageText.dollars(limit))")
            } else if let used = credit.keyUsed {
                lines.append("Chave: sem teto, gastou \(UsageText.dollars(used))")
            }
            if let today = credit.usedToday { lines.append("Gasto hoje: \(UsageText.dollars(today))") }
            if let month = credit.usedThisMonth { lines.append("Gasto no mês: \(UsageText.dollars(month))") }
        }
        if let pending = jev.pending, pending > 0 {
            lines.append("Inclui \(UsageText.dollars(pending)) de decisões desta máquina que o OpenRouter ainda não contou")
        }
        if let at = jev.measuredAt { lines.append("Medido \(UsageText.ago(at, now: now))") }
        return lines.joined(separator: "\n")
    }

    private func note(_ line: AccountUsage, now: Date) -> String? {
        var parts: [String] = []
        if line.reading?.limitReached == true { parts.append("no limite") }
        if let warning = line.account.warning { parts.append(warning) }
        if let problem = line.problem {
            parts.append(problem)
        } else if line.reading == nil {
            parts.append("medindo…")
        }
        if let at = line.measuredAt, now.timeIntervalSince(at) > 15 * 60 {
            parts.append("medido \(UsageText.ago(at, now: now))")
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private func accountHelp(_ line: AccountUsage, now: Date) -> String {
        var lines: [String] = []
        if let email = line.account.email { lines.append(email) }
        if let plan = line.account.plan { lines.append("Plano \(plan)") }
        if line.account.aliases.count > 1 {
            lines.append("Também chamada: " + line.account.aliases.joined(separator: ", "))
        }
        if line.account.isUsed(by: selectedAccount) {
            lines.append("Conta usada nesta aba")
        }
        if let at = line.measuredAt { lines.append("Medido \(UsageText.ago(at, now: now))") }
        return lines.joined(separator: "\n")
    }

    private func barHelp(_ window: UsageWindow, now: Date) -> String {
        var text = "\(window.title): \(UsageText.percent(window.percent)) usados"
        if let resets = window.resetsAt {
            text += "\nReinicia \(UsageText.moment(resets, now: now))"
        }
        return text
    }
}

/// The footer's inks: the sidebar's own (`Palette` in WorkspaceSidebar.swift,
/// private to that file), and the two warning hues it lacks. Same values, so
/// the footer's text sits with the list's.
private enum UsageInk {
    private static func inkColor(dark: CGFloat, light: CGFloat) -> NSColor {
        NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                ? NSColor.white.withAlphaComponent(dark)
                : NSColor.black.withAlphaComponent(light)
        }
    }

    static let ink = Color(nsColor: inkColor(dark: 0.96, light: 0.92))
    static let inkResting = Color(nsColor: restingColor)
    static let inkFaint = Color(nsColor: faintColor)
    /// The same two, for the controls drawn by AppKit.
    static let restingColor = inkColor(dark: 0.60, light: 0.75)
    static let faintColor = inkColor(dark: 0.38, light: 0.55)

    static func wash(_ alpha: CGFloat) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                ? NSColor.white.withAlphaComponent(alpha)
                : NSColor.black.withAlphaComponent(alpha)
        })
    }

    /// A state hue a step darker on a light ground, as the sidebar's are.
    private static func state(_ lightness: Double, _ chroma: Double, _ hue: Double) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let dark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            return oklch(dark ? lightness : lightness - 0.22, chroma, hue)
        })
    }

    /// The sidebar's "attached" green: the account the tabs are on.
    static let live = state(0.76, 0.14, 150)
    /// The sidebar's "busy" amber.
    static let attention = state(0.82, 0.15, 85)
    static let critical = state(0.70, 0.19, 27)

    static func fill(_ level: UsageText.Level) -> Color {
        switch level {
        case .normal: return inkResting
        case .attention: return attention
        case .critical: return critical
        }
    }

    /// OKLCH to sRGB, the same conversion the sidebar's palette uses.
    private static func oklch(_ lightness: Double, _ chroma: Double, _ hue: Double) -> NSColor {
        let radians = hue * .pi / 180
        let a = chroma * cos(radians)
        let b = chroma * sin(radians)
        let l = pow(lightness + 0.3963377774 * a + 0.2158037573 * b, 3)
        let m = pow(lightness - 0.1055613458 * a - 0.0638541728 * b, 3)
        let s = pow(lightness - 0.0894841775 * a - 1.2914855480 * b, 3)
        func encode(_ channel: Double) -> Double {
            let value = max(0, min(1, channel))
            return value <= 0.0031308 ? value * 12.92 : 1.055 * pow(value, 1 / 2.4) - 0.055
        }
        return NSColor(
            srgbRed: encode(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
            green: encode(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
            blue: encode(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s),
            alpha: 1)
    }
}
