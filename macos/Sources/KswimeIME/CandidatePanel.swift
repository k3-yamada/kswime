// A small borderless, non-activating candidate window.  IMKCandidates is
// avoided on purpose: its selection API misbehaves on recent macOS, and a
// custom NSPanel can be positioned at the caret and styled freely.
import Cocoa

final class CandidatePanel {
    private let panel: NSPanel
    private let stack = NSStackView()
    private var labels: [NSTextField] = []
    private(set) var isVisible = false

    init() {
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 200, height: 100),
                        styleMask: [.borderless, .nonactivatingPanel],
                        backing: .buffered, defer: true)
        panel.level = .popUpMenu
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.setAccessibilityElement(false)

        let effect = NSVisualEffectView()
        effect.material = .popover
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = 8
        effect.layer?.masksToBounds = true
        effect.translatesAutoresizingMaskIntoConstraints = false

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 2
        stack.edgeInsets = NSEdgeInsets(top: 6, left: 8, bottom: 6, right: 8)
        stack.translatesAutoresizingMaskIntoConstraints = false
        effect.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            stack.topAnchor.constraint(equalTo: effect.topAnchor),
            stack.bottomAnchor.constraint(equalTo: effect.bottomAnchor),
        ])
        panel.contentView = effect
    }

    /// Show `candidates` (at most 9 shown, numbered) with `selected`
    /// highlighted, placed under the caret rectangle `caret` (screen
    /// coordinates).  `level` is the client window level + 1.
    func show(candidates: [String], selected: Int, caret: NSRect, level: NSWindow.Level, title: String? = nil) {
        for l in labels { stack.removeArrangedSubview(l); l.removeFromSuperview() }
        labels.removeAll()
        // Some hosts (Warp) draw neither the selected range nor attributes of
        // the marked text, so the window itself names the word being edited.
        if let title {
            let header = NSTextField(labelWithString: title)
            header.font = NSFont.systemFont(ofSize: 11)
            header.textColor = .secondaryLabelColor
            stack.addArrangedSubview(header)
            labels.append(header)
        }
        let page = max(0, selected / 9)
        let start = page * 9
        let slice = Array(candidates[start..<min(candidates.count, start + 9)])
        for (i, c) in slice.enumerated() {
            let index = start + i
            let label = NSTextField(labelWithString: "\(i + 1)  \(c)")
            label.font = NSFont.systemFont(ofSize: 15)
            label.textColor = index == selected ? .selectedMenuItemTextColor : .labelColor
            label.drawsBackground = index == selected
            label.backgroundColor = index == selected ? .selectedContentBackgroundColor : .clear
            label.wantsLayer = true
            label.layer?.cornerRadius = 4
            stack.addArrangedSubview(label)
            labels.append(label)
        }
        panel.level = level
        panel.contentView?.layoutSubtreeIfNeeded()
        let size = stack.fittingSize
        var origin = NSPoint(x: caret.minX, y: caret.minY - size.height - 4)
        if let screen = NSScreen.screens.first(where: { $0.frame.contains(caret.origin) }) ?? NSScreen.main {
            let f = screen.visibleFrame
            if origin.y < f.minY { origin.y = caret.maxY + 4 }
            if origin.x + size.width > f.maxX { origin.x = f.maxX - size.width }
            if origin.x < f.minX { origin.x = f.minX }
        }
        panel.setFrame(NSRect(origin: origin, size: size), display: true)
        panel.orderFrontRegardless()
        isVisible = true
    }

    func hide() {
        if isVisible {
            panel.orderOut(nil)
            isVisible = false
        }
    }
}
