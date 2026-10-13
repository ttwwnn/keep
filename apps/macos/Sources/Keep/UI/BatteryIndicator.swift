import AppKit
import IOKit.ps

/// The battery: a cell with its percentage written inside it, and its charge
/// in a colour — green while there is plenty or the Mac is charging, amber
/// from 20% down, red from 10%.
///
/// It stands in the sidebar's top row whenever the menu bar, which carries
/// the system's own, is out of sight (`SidebarHost`). A Mac without a battery
/// has nothing to show, and `isPresent` says so.
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

    private static let tipWidth: CGFloat = 1.5
    /// The bolt, left of the cell while it charges, and its gap.
    private static var boltWidth: CGFloat { 8 * GhosttyApp.shared.iconScale }
    private static let boltGap: CGFloat = 3
    /// Room round the figure inside the cell.
    /// As tall as the red close button of the window (a traffic light is 12 across) at 100%, then with the zoom.
    private static var height: CGFloat { 12 * GhosttyApp.shared.iconScale }
    private static let padding: CGFloat = 2
    /// The zoom control's percentage, face and size exactly.
    private static var font: NSFont {
        NSFont.monospacedDigitSystemFont(ofSize: 11 * GhosttyApp.shared.iconScale, weight: .medium)
    }

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
            MainActor.assumeIsolated {
                self?.refresh()
                // The menu bar may have been hidden or shown by the system's setting.
                self?.superview?.needsLayout = true
            }
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
        let appeared = (next == nil) != (reading == nil) || next?.charging != reading?.charging
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

    /// The cell: as wide as "100%" with room round it, as tall as the text.
    private var bodySize: NSSize {
        let widest = NSAttributedString(string: "100%", attributes: [.font: Self.font]).size().width
        let height = Self.height
        return NSSize(width: ceil(widest) + 2 * Self.padding, height: height)
    }

    /// How tall the indicator is.
    var fittingHeight: CGFloat { bodySize.height }

    /// How wide the indicator is: the bolt while charging, the cell, its tip.
    var fittingWidth: CGFloat {
        let bolt = reading?.charging == true ? Self.boltWidth + Self.boltGap : 0
        return bolt + bodySize.width + Self.tipWidth
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let reading else { return }
        let palette = TabStripView.Palette.current
        let size = bodySize
        let body = NSRect(
            x: bounds.maxX - Self.tipWidth - size.width,
            y: ((bounds.height - size.height) / 2).rounded(),
            width: size.width, height: size.height)

        // The cell and its tip.
        let radius = min(4, size.height / 3)
        let outline = NSBezierPath(
            roundedRect: body.insetBy(dx: 0.5, dy: 0.5), xRadius: radius, yRadius: radius)
        outline.lineWidth = 1
        palette.dimText.withAlphaComponent(0.7).setStroke()
        outline.stroke()
        let tip = NSBezierPath(roundedRect: NSRect(
            x: body.maxX + 0.5, y: body.midY - 2.5, width: Self.tipWidth - 0.5, height: 5),
            xRadius: 0.75, yRadius: 0.75)
        palette.dimText.withAlphaComponent(0.7).setFill()
        tip.fill()

        // The charge, in its colour.
        let inner = body.insetBy(dx: 1.5, dy: 1.5)
        let charge = NSRect(
            x: inner.minX, y: inner.minY,
            width: max(2, inner.width * CGFloat(reading.percent) / 100), height: inner.height)
        Self.fill(reading).setFill()
        NSBezierPath(roundedRect: charge, xRadius: radius - 1.5, yRadius: radius - 1.5).fill()

        // The percentage inside the cell: dark ink over the charge, the
        // terminal's own over the empty part, so it reads at any level.
        func figure(_ color: NSColor) -> NSAttributedString {
            NSAttributedString(string: "\(reading.percent)%", attributes: [
                .font: Self.font, .foregroundColor: color,
            ])
        }
        let sample = figure(.black)
        let mark = sample.size()
        let at = NSPoint(x: body.midX - mark.width / 2, y: body.midY - mark.height / 2)
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(rect: charge).addClip()
        figure(NSColor.black.withAlphaComponent(0.85)).draw(at: at)
        NSGraphicsContext.restoreGraphicsState()
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(rect: NSRect(
            x: charge.maxX, y: body.minY, width: max(0, body.maxX - charge.maxX), height: body.height)).addClip()
        figure(palette.text).draw(at: at)
        NSGraphicsContext.restoreGraphicsState()

        // A bolt beside the cell while it charges.
        if reading.charging,
           let bolt = NSImage(systemSymbolName: "bolt.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 8 * GhosttyApp.shared.iconScale, weight: .bold))
        {
            let tinted = NSImage(size: bolt.size, flipped: false) { rect in
                bolt.draw(in: rect)
                Self.fill(reading).set()
                rect.fill(using: .sourceIn)
                return true
            }
            tinted.draw(in: NSRect(
                x: body.minX - Self.boltGap - bolt.size.width, y: body.midY - bolt.size.height / 2,
                width: bolt.size.width, height: bolt.size.height))
        }
    }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
