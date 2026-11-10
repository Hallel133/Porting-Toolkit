// Porting Toolkit HTML5 wrapper: a minimal macOS app that plays a web game.
//
// The game's files live in Contents/Resources/app and are served to WebKit
// through the app:// scheme (with byte ranges, so audio and video seek),
// because many engines load data with XHR/fetch, which file:// URLs block.
// Contents/Resources/ptk-app.json holds the entry page and window size.
//
// PTK_WRAPPER_SELFTEST=1 prints "PTK_WRAPPER_LOADED <document.title>" once
// the page has loaded, then quits (Porting Toolkit's tests use it).

import Cocoa
import WebKit

struct AppConfig: Decodable {
    var entry: String
    var title: String?
    var width: Double?
    var height: Double?
}

let mimeTypes: [String: String] = [
    "html": "text/html", "htm": "text/html", "js": "text/javascript", "mjs": "text/javascript",
    "css": "text/css", "json": "application/json", "wasm": "application/wasm", "txt": "text/plain",
    "png": "image/png", "jpg": "image/jpeg", "jpeg": "image/jpeg", "gif": "image/gif", "webp": "image/webp",
    "svg": "image/svg+xml", "ico": "image/x-icon", "bmp": "image/bmp",
    "ogg": "audio/ogg", "oga": "audio/ogg", "m4a": "audio/mp4", "mp3": "audio/mpeg", "wav": "audio/wav",
    "mp4": "video/mp4", "webm": "video/webm", "ogv": "video/ogg",
    "ttf": "font/ttf", "otf": "font/otf", "woff": "font/woff", "woff2": "font/woff2",
    "xml": "application/xml",
]

final class SchemeHandler: NSObject, WKURLSchemeHandler {
    let root: URL

    init(root: URL) {
        self.root = root.standardizedFileURL
    }

    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url else { return }
        var path = url.path
        if path.isEmpty || path == "/" { path = "/index.html" }
        let file = root.appendingPathComponent(String(path.dropFirst())).standardizedFileURL
        guard file.path.hasPrefix(root.path + "/"),
              let data = try? Data(contentsOf: file, options: .mappedIfSafe)
        else {
            let response = HTTPURLResponse(url: url, statusCode: 404, httpVersion: "HTTP/1.1",
                                           headerFields: ["Content-Type": "text/plain"])!
            task.didReceive(response)
            task.didReceive(Data("Not found".utf8))
            task.didFinish()
            return
        }
        let ext = file.pathExtension.lowercased()
        var headers = [
            "Content-Type": mimeTypes[ext] ?? "application/octet-stream",
            "Accept-Ranges": "bytes",
            "Cache-Control": "no-cache",
            "Access-Control-Allow-Origin": "*",
        ]
        var status = 200
        var body = data
        if let header = task.request.value(forHTTPHeaderField: "Range"),
           let range = parseRange(header, count: data.count) {
            body = data.subdata(in: range)
            status = 206
            headers["Content-Range"] = "bytes \(range.lowerBound)-\(range.upperBound - 1)/\(data.count)"
        }
        headers["Content-Length"] = String(body.count)
        let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)!
        task.didReceive(response)
        task.didReceive(body)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}

    func parseRange(_ header: String, count: Int) -> Range<Int>? {
        guard header.hasPrefix("bytes="), count > 0 else { return nil }
        let spec = header.dropFirst(6).split(separator: ",").first ?? ""
        let parts = spec.split(separator: "-", omittingEmptySubsequences: false)
        guard parts.count == 2 else { return nil }
        if parts[0].isEmpty, let suffix = Int(parts[1]), suffix > 0 {
            return max(0, count - suffix)..<count
        }
        guard let start = Int(parts[0]), start < count else { return nil }
        let end = Int(parts[1]).map { min($0, count - 1) } ?? count - 1
        return end >= start ? start..<(end + 1) : nil
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, WKNavigationDelegate, WKUIDelegate {
    var window: NSWindow!
    var webView: WKWebView!
    let selfTest = ProcessInfo.processInfo.environment["PTK_WRAPPER_SELFTEST"] == "1"

    func applicationDidFinishLaunching(_ notification: Notification) {
        let resources = Bundle.main.resourceURL!
        let root = resources.appendingPathComponent("app")
        var config = AppConfig(entry: "index.html")
        if let data = try? Data(contentsOf: resources.appendingPathComponent("ptk-app.json")),
           let parsed = try? JSONDecoder().decode(AppConfig.self, from: data) {
            config = parsed
        }
        let name = config.title ?? (Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Game")

        let webConfig = WKWebViewConfiguration()
        webConfig.setURLSchemeHandler(SchemeHandler(root: root), forURLScheme: "app")
        webConfig.mediaTypesRequiringUserActionForPlayback = []
        if #available(macOS 12.3, *) {
            webConfig.preferences.isElementFullscreenEnabled = true
        }
        webView = WKWebView(frame: .zero, configuration: webConfig)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        if #available(macOS 13.3, *) {
            webView.isInspectable = true
        }

        let size = NSSize(width: config.width ?? 1280, height: config.height ?? 720)
        window = NSWindow(contentRect: NSRect(origin: .zero, size: size),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        window.title = name
        window.contentView = webView
        window.collectionBehavior = [.fullScreenPrimary]
        window.center()
        window.setFrameAutosaveName("GameWindow")
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(webView)
        buildMenu(name: name)
        NSApp.activate(ignoringOtherApps: true)

        let entry = config.entry.split(separator: "/").map {
            $0.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? String($0)
        }.joined(separator: "/")
        webView.load(URLRequest(url: URL(string: "app://game/\(entry)")!))
    }

    func buildMenu(name: String) {
        let main = NSMenu()
        let appItem = NSMenuItem()
        main.addItem(appItem)
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "About \(name)", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Hide \(name)", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Quit \(name)", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu

        let viewItem = NSMenuItem()
        main.addItem(viewItem)
        let viewMenu = NSMenu(title: "View")
        viewMenu.addItem(withTitle: "Reload", action: #selector(reload), keyEquivalent: "r")
        let full = viewMenu.addItem(withTitle: "Enter Full Screen", action: #selector(NSWindow.toggleFullScreen(_:)), keyEquivalent: "f")
        full.keyEquivalentModifierMask = [.control, .command]
        viewItem.submenu = viewMenu

        let windowItem = NSMenuItem()
        main.addItem(windowItem)
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        windowItem.submenu = windowMenu
        NSApp.windowsMenu = windowMenu
        NSApp.mainMenu = main
    }

    @objc func reload() {
        webView.reload()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        guard selfTest else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
            webView.evaluateJavaScript("document.title") { result, _ in
                print("PTK_WRAPPER_LOADED \(result as? String ?? "")")
                fflush(stdout)
                NSApp.terminate(nil)
            }
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        if selfTest {
            print("PTK_WRAPPER_FAILED \(error.localizedDescription)")
            NSApp.terminate(nil)
        }
    }

    func webViewDidClose(_ webView: WKWebView) {
        NSApp.terminate(nil)
    }

    func webView(_ webView: WKWebView, runJavaScriptAlertPanelWithMessage message: String,
                 initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping () -> Void) {
        let alert = NSAlert()
        alert.messageText = message
        alert.runModal()
        completionHandler()
    }

    func webView(_ webView: WKWebView, runJavaScriptConfirmPanelWithMessage message: String,
                 initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping (Bool) -> Void) {
        let alert = NSAlert()
        alert.messageText = message
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Cancel")
        completionHandler(alert.runModal() == .alertFirstButtonReturn)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
