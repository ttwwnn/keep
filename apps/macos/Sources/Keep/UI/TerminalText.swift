import AppKit

/// Terminal output, drawn outside a terminal.
///
/// A preview of a tab is a picture of a screen, and a screen is coloured: the
/// error is red, the branch is blue, the prompt is whatever the person made
/// it. Shown as plain text it is not just duller, it is harder to read — the
/// colour is carrying meaning, and dropping it drops the meaning with it.
///
/// So the daemon hands over the screen as VT sequences — the same bytes it
/// repaints an attaching client with — and this turns them into something
/// AppKit can draw. Only what a screen holds is understood: colours, bold,
/// italic, underline, dim. Everything else an escape can say is about *where*
/// to put things, and where is already decided by the time a snapshot is
/// taken, so it is read past and dropped.
enum TerminalText {
    /// The escape a snapshot is made of, as an attributed string.
    static func attributed(
        _ vt: String, font: NSFont, palette: [NSColor], foreground: NSColor
    ) -> NSAttributedString {
        let out = NSMutableAttributedString()
        var style = Style(foreground: foreground)
        var run = ""

        func flush() {
            guard !run.isEmpty else { return }
            out.append(NSAttributedString(string: run, attributes: style.attributes(font: font)))
            run = ""
        }

        var rest = Substring(vt)
        while let next = rest.first {
            guard next == "\u{1b}" else {
                // Carriage returns are the snapshot's line endings paired with
                // a newline; kept, the text would draw one line on top of
                // another in a view that honours them.
                if next != "\r" { run.append(next) }
                rest = rest.dropFirst()
                continue
            }
            flush()
            rest = rest.dropFirst()
            switch rest.first {
            case "[":
                let (parameters, final, remainder) = readControl(rest.dropFirst())
                rest = remainder
                if final == "m" { style.apply(parameters, palette: palette, base: foreground) }
            case "]":
                // An operating-system command: a title, a working directory, a
                // hyperlink. It ends at a bell or at a string terminator.
                rest = rest.dropFirst()
                while let c = rest.first, c != "\u{07}" {
                    if c == "\u{1b}" { rest = rest.dropFirst(); break }
                    rest = rest.dropFirst()
                }
                rest = rest.dropFirst()
            default:
                rest = rest.dropFirst()
            }
        }
        flush()
        return out
    }

    /// The parameters of a control sequence, its final byte, and what is left.
    private static func readControl(
        _ input: Substring
    ) -> (parameters: [Int], final: Character, rest: Substring) {
        var rest = input
        var digits = ""
        var parameters: [Int] = []
        while let c = rest.first {
            rest = rest.dropFirst()
            if c.isNumber {
                digits.append(c)
            } else if c == ";" || c == ":" {
                parameters.append(Int(digits) ?? 0)
                digits = ""
            } else if c == "?" || c == ">" || c == "<" || c == "=" {
                continue  // a private marker; the parameters read the same
            } else {
                if !digits.isEmpty || !parameters.isEmpty { parameters.append(Int(digits) ?? 0) }
                return (parameters, c, rest)
            }
        }
        return (parameters, " ", rest)
    }

    /// What the sequences seen so far say the next characters look like.
    private struct Style {
        var foreground: NSColor
        var bold = false
        var italic = false
        var underline = false
        var dim = false

        func attributes(font: NSFont) -> [NSAttributedString.Key: Any] {
            var face = font
            if bold || italic {
                var traits: NSFontDescriptor.SymbolicTraits = []
                if bold { traits.insert(.bold) }
                if italic { traits.insert(.italic) }
                let descriptor = font.fontDescriptor.withSymbolicTraits(traits)
                face = NSFont(descriptor: descriptor, size: font.pointSize) ?? font
            }
            var attributes: [NSAttributedString.Key: Any] = [
                .font: face,
                .foregroundColor: dim ? foreground.withAlphaComponent(0.6) : foreground,
            ]
            if underline { attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue }
            return attributes
        }

        /// One SGR sequence. Anything not about the colour or the face of the
        /// text — where the cursor goes, what the terminal is called — has
        /// already been read past by the time this is called.
        mutating func apply(_ parameters: [Int], palette: [NSColor], base: NSColor) {
            var index = 0
            let parameters = parameters.isEmpty ? [0] : parameters
            while index < parameters.count {
                let code = parameters[index]
                switch code {
                case 0:
                    self = Style(foreground: base)
                case 1: bold = true
                case 2: dim = true
                case 3: italic = true
                case 4: underline = true
                case 22: bold = false; dim = false
                case 23: italic = false
                case 24: underline = false
                case 30...37: foreground = palette[code - 30]
                case 90...97: foreground = palette[code - 90 + 8]
                case 39: foreground = base
                case 38:
                    // 38;5;n is one of the 256; 38;2;r;g;b is exact.
                    if index + 2 < parameters.count, parameters[index + 1] == 5 {
                        foreground = palette[min(255, max(0, parameters[index + 2]))]
                        index += 2
                    } else if index + 4 < parameters.count, parameters[index + 1] == 2 {
                        foreground = NSColor(
                            srgbRed: CGFloat(parameters[index + 2]) / 255,
                            green: CGFloat(parameters[index + 3]) / 255,
                            blue: CGFloat(parameters[index + 4]) / 255,
                            alpha: 1)
                        index += 4
                    }
                default:
                    // Backgrounds among them: a preview is drawn on glass, and
                    // painting the terminal's own background back over it
                    // would put a second, squarer window inside the card.
                    break
                }
                index += 1
            }
        }
    }
}
