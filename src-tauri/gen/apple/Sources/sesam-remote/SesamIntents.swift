// Kurzbefehle für Sesam (iPadOS 16+): «Verbindung öffnen» in der App «Kurzbefehle», in Spotlight, bei Siri und
// als Symbol auf dem Home-Bildschirm. Ersatz für die Verknüpfungen auf dem Schreibtisch (docs/plattformen.md).
// Gelesen werden nur ID, Name und Dienste aus library.json; Zugangsdaten liegen im Schlüsselbund und kommen hier
// nie vor. Geöffnet wird über denselben Link wie auf dem Desktop: sesam://open?id=…&view=…

import AppIntents
import Foundation
import UIKit

/// Eine Verbindung aus der Bibliothek, so wie Kurzbefehle sie zeigen.
@available(iOS 16.0, *)
struct ConnectionEntity: AppEntity {
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Verbindung")
    static var defaultQuery = ConnectionQuery()

    let id: String
    let name: String
    let kind: String
    let symbol: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)", subtitle: "\(kind)", image: .init(systemName: symbol))
    }

    /// Bibliothek der App (Tauri: Application Support/<Bundle-ID>/library.json), nur lesend.
    static func all() -> [ConnectionEntity] {
        guard let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first,
              let id = Bundle.main.bundleIdentifier,
              let data = try? Data(contentsOf: support.appendingPathComponent(id).appendingPathComponent("library.json")),
              let list = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return list.compactMap { c in
            guard let id = c["id"] as? String, let name = c["name"] as? String else { return nil }
            let desktop = (c["rdpPort"] is NSNumber) || (c["vncPort"] is NSNumber)
            let ssh = c["sshPort"] is NSNumber
            let kind = desktop ? ((c["desktopProtocol"] as? String) == "vnc" || !(c["rdpPort"] is NSNumber) ? "VNC" : "RDP") : ssh ? "Shell" : "Dateien"
            let symbol = desktop ? "display" : ssh ? "terminal" : "folder"
            return ConnectionEntity(id: id, name: name.isEmpty ? (c["address"] as? String ?? id) : name, kind: kind, symbol: symbol)
        }
    }
}

@available(iOS 16.0, *)
struct ConnectionQuery: EntityStringQuery {
    func entities(for identifiers: [String]) async throws -> [ConnectionEntity] {
        ConnectionEntity.all().filter { identifiers.contains($0.id) }
    }
    func entities(matching string: String) async throws -> [ConnectionEntity] {
        ConnectionEntity.all().filter { $0.name.localizedCaseInsensitiveContains(string) }
    }
    func suggestedEntities() async throws -> [ConnectionEntity] {
        ConnectionEntity.all()
    }
}

/// Was beim Öffnen gezeigt wird; «Wie eingestellt» nimmt die Ansicht der Verbindung.
@available(iOS 16.0, *)
enum ConnectionView: String, AppEnum {
    case standard, desktop, shell, files
    static var typeDisplayRepresentation = TypeDisplayRepresentation(name: "Ansicht")
    static var caseDisplayRepresentations: [ConnectionView: DisplayRepresentation] = [
        .standard: "Wie eingestellt", .desktop: "Desktop", .shell: "Shell", .files: "Dateien",
    ]
}

@available(iOS 16.0, *)
struct OpenConnectionIntent: AppIntent {
    static var title: LocalizedStringResource = "Verbindung öffnen"
    static var description = IntentDescription("Öffnet eine Verbindung aus der Bibliothek von Sesam.")
    static var openAppWhenRun = true

    @Parameter(title: "Verbindung")
    var connection: ConnectionEntity

    @Parameter(title: "Ansicht", default: .standard)
    var view: ConnectionView

    static var parameterSummary: some ParameterSummary {
        Summary("\(\.$connection) öffnen") { \.$view }
    }

    @MainActor
    func perform() async throws -> some IntentResult {
        var parts = URLComponents()
        parts.scheme = "sesam"
        parts.host = "open"
        parts.queryItems = [URLQueryItem(name: "id", value: connection.id)] + (view == .standard ? [] : [URLQueryItem(name: "view", value: view.rawValue)])
        if let url = parts.url { await UIApplication.shared.open(url) }
        return .result()
    }
}

@available(iOS 16.0, *)
struct SesamShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: OpenConnectionIntent(),
            phrases: [
                "\(\.$connection) mit \(.applicationName) öffnen",
                "Mit \(.applicationName) zu \(\.$connection) verbinden",
                "Verbindung in \(.applicationName) öffnen",
            ]
        )
    }
}
