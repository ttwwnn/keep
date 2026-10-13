import AppKit
import IOKit.ps

/// The battery, as the menu bar draws it: the percentage, the cell, and its
/// charge in a colour — green while there is plenty or the Mac is charging,
/// amber from 20% down, red from 10%.
///
/// It stands in the titlebar row of a full-screen window (`CenteringHost`),
/// where the menu bar that carries the system's own is out of sight. A Mac
/// without a battery has nothing to show, and `isPresent` says so.
@MainActor
final class BatteryIndicator: NSView {
    struct Reading: Equatable {
        var percent: Int
        var charging: Bool
        var onPower: Bool
    }

    private(set) var reading: Reading?
    var isPresent: Bool { reading != nil }

    private var source: CFRunLoopSource?
    private var timer: Timer?
    private var groundObserver: NSObjectProtocol?
    private var zoomObserver: NSObjectProtocol?

    private static let bodySize = NSSize(width: 22, height: 11)
    private static let tipWidth: CGFloat = 1.5
    private static let gap: CGFloat = 5
    private static var font: NSFont { GhosttyApp.shared.tabNameFont }

    init() {
        super.init(frame: .zero)
        setAccessibilityRole(.staticText)
        refresh()
        // The system says when the charge or the power source changes; the
        // timer is for the percentage drifting between those.
        let context = Unmanaged.passUnretained(self).toOpaque()
        if let loop = IOPSNotificationCreateRunLoopSource({ context in
            guard let context else { return }
            let view = Unmanaged<BatteryIndicator>.fromOpaque(context).takeUnretainedValue()
            MainActor.assumeIsolated { view.refresh() }
        }, context)?.takeRetainedValue() {
            CFRunLoopAddSource(CFRunLoopGetMain(), loop, .commonModes)
            source = loop
        }
        // The ink follows the terminal's ground.
        groundObserver = NotificationCenter.default.addObserver(
            forName: GhosttyApp.backgroundDidChange, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.needsDisplay = true }
        }
        // The figure is a tab name's size, so the zoom moves the room it needs.
        zoomObserver = NotificationCenter.default.addObserver(
            forName: GhosttyApp.textSizeDidChange, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.needsDisplay = true
                self?.superview?.needsLayout = true
            }
        }
        timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    isolated deinit {
        if let source { CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes) }
        timer?.invalidate()
        if let groundObserver { NotificationCenter.default.removeObserver(groundObserver) }
        if let zoomObserver { NotificationCenter.default.removeObserver(zoomObserver) }
    }

    /// nil on a Mac with no battery.
    static func read() -> Reading? {
        guard let info = IOPSCopyPowerSourcesInfo()?.takeRetainedValue(),
              let list = IOPSCopyPowerSourcesList(info)?.takeRetainedValue() as? [CFTypeRef]
        else { return nil }
        for item in list {
            guard let description = IOPSGetPowerSourceDescription(info, item)?
                    .takeUnretainedValue() as? [String: Any],
                  description[kIOPSTypeKey] as? String == kIOPSInternalBatteryType,
                  let current = description[kIOPSCurrentCapacityKey] as? Int,
                  let maximum = description[kIOPSMaxCapacityKey] as? Int, maximum > 0
            else { continue }
            let percent = min(100, max(0, Int((Double(current) / Double(maximum) * 100).rounded())))
            let charging = description[kIOPSIsChargingKey] as? Bool ?? false
            let onPower = description[kIOPSPowerSourceStateKey] as? String == kIOPSACPowerValue
            return Reading(percent: percent, charging: charging, onPower: onPower)
        }
        return nil
    }

    func refresh() {
        let next = Self.read()
        guard next != reading else { return }
        let appeared = (next == nil) != (reading == nil)
        reading = next
        toolTip = next.map { Self.sentence($0) }
        setAccessibilityLabel(next.map { Self.sentence($0) })
        needsDisplay = true
        // Appearing or going changes whether the host has anything to place.
        if appeared { superview?.needsLayout = true }
    }

    private static func sentence(_ reading: Reading) -> String {
        let state = reading.charging ? "carregando"
            : reading.onPower ? "carregada, na tomada" : "na bateria"
        return "Bateria em \(reading.percent)%, \(state)"
    }

    /// The colour of the charge.
    static func fill(_ reading: Reading) -> NSColor {
        func state(_ lightness: Double, _ chroma: Double, _ hue: Double) -> NSColor {
            NSColor(name: nil) { appearance in
                let dark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                return oklch(dark ? lightness : lightness - 0.22, chroma, hue)
            }
        }
        if reading.charging || reading.onPower || reading.percent > 20 { return state(0.76, 0.14, 150) }
        if reading.percent > 10 { return state(0.82, 0.15, 85) }
        return state(0.70, 0.19, 27)
    }

    /// OKLCH to sRGB, as the sidebar's palette does it.
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

    private var text: NSAttributedString {
        let palette = TabStripView.Palette.current
        return NSAttributedString(string: "\(reading?.percent ?? 0)%", attributes: [
            .font: Self.font,
            .foregroundColor: palette.dimText,
        ])
    }

    /// How wide the indicator is: the figure, a gap, the cell and its tip.
    var fittingWidth: CGFloat {
        // Room for "100%", so the cell does not shuffle as the figure grows.
        let widest = NSAttributedString(string: "100%", attributes: [.font: Self.font]).size().width
        return ceil(widest) + Self.gap + Self.bodySize.width + Self.tipWidth
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let reading else { return }
        let palette = TabStripView.Palette.current
        let figure = text
        let body = NSRect(
            x: bounds.maxX - Self.tipWidth - Self.bodySize.width,
            y: ((bounds.height - Self.bodySize.height) / 2).rounded(),
            width: Self.bodySize.width, height: Self.bodySize.height)

        // The percentage, flush against the cell.
        let size = figure.size()
        figure.draw(at: NSPoint(
            x: body.minX - Self.gap - size.width,
            y: ((bounds.height - size.height) / 2).rounded()))

        // The cell and its tip.
        let outline = NSBezierPath(roundedRect: body.insetBy(dx: 0.5, dy: 0.5), xRadius: 3, yRadius: 3)
        outline.lineWidth = 1
        palette.dimText.withAlphaComponent(0.7).setStroke()
        outline.stroke()
        let tip = NSBezierPath(roundedRect: NSRect(
            x: body.maxX + 0.5, y: body.midY - 2, width: Self.tipWidth, height: 4),
            xRadius: 0.75, yRadius: 0.75)
        palette.dimText.withAlphaComponent(0.7).setFill()
        tip.fill()

        // The charge, in its colour.
        let inner = body.insetBy(dx: 2, dy: 2)
        let width = max(2, inner.width * CGFloat(reading.percent) / 100)
        Self.fill(reading).setFill()
        NSBezierPath(
            roundedRect: NSRect(x: inner.minX, y: inner.minY, width: width, height: inner.height),
            xRadius: 1.5, yRadius: 1.5).fill()

        // A bolt over the cell while it charges.
        if reading.charging,
           let bolt = NSImage(systemSymbolName: "bolt.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 8, weight: .bold))
        {
            let mark = bolt.size
            let spot = NSRect(
                x: body.midX - mark.width / 2, y: body.midY - mark.height / 2,
                width: mark.width, height: mark.height)
            // Dark ink over the colour, drawn into an image of its own so
            // the tint reaches the glyph and nothing else.
            let dark = NSImage(size: mark, flipped: false) { rect in
                bolt.draw(in: rect)
                NSColor.black.withAlphaComponent(0.85).set()
                rect.fill(using: .sourceIn)
                return true
            }
            dark.draw(in: spot)
        }
    }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
