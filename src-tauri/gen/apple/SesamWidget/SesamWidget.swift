// Widget «Zuletzt verwendet» für den Home-Bildschirm des iPads. Noch in keinem Ziel des Xcode-Projekts:
// Es braucht die App Group «group.ch.sesam.remote», die nur das bezahlte Entwicklerprogramm ausstellt.
// Einschalten: docs/plattformen.md, Abschnitt «Widget einschalten». Die App schreibt die Liste selbst
// (src-tauri/src/jumps.rs, widget.json im Ordner der App Group); ohne App Group bleibt sie leer.
// Nur ID, Name und Link, keine Zugangsdaten. Ein Tipp öffnet sesam://open?id=…

import SwiftUI
import WidgetKit

struct RecentConnection: Decodable, Identifiable {
    let id: String
    let name: String
    let url: String
}

struct RecentEntry: TimelineEntry {
    let date: Date
    let items: [RecentConnection]
}

struct RecentProvider: TimelineProvider {
    static let group = "group.ch.sesam.remote"

    static func load() -> [RecentConnection] {
        guard let dir = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group),
              let data = try? Data(contentsOf: dir.appendingPathComponent("widget.json")),
              let list = try? JSONDecoder().decode([RecentConnection].self, from: data)
        else { return [] }
        return list
    }

    func placeholder(in context: Context) -> RecentEntry {
        RecentEntry(date: Date(), items: [RecentConnection(id: "1", name: "Büro-PC", url: "sesam://open?id=1")])
    }

    func getSnapshot(in context: Context, completion: @escaping (RecentEntry) -> Void) {
        completion(RecentEntry(date: Date(), items: Self.load()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<RecentEntry>) -> Void) {
        // Die App schreibt widget.json bei jeder Änderung; stündlich neu lesen genügt.
        completion(Timeline(entries: [RecentEntry(date: Date(), items: Self.load())], policy: .after(Date().addingTimeInterval(3600))))
    }
}

struct RecentView: View {
    @Environment(\.widgetFamily) var family
    let entry: RecentEntry

    var body: some View {
        let max = family == .systemSmall ? 3 : family == .systemMedium ? 4 : 8
        VStack(alignment: .leading, spacing: 6) {
            Text("Sesam").font(.caption).foregroundStyle(.secondary)
            if entry.items.isEmpty {
                Text("Noch keine Verbindung geöffnet").font(.footnote).foregroundStyle(.secondary)
            }
            ForEach(entry.items.prefix(max)) { c in
                Link(destination: URL(string: c.url)!) {
                    Label(c.name, systemImage: "display").font(.subheadline).lineLimit(1)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

@main
struct SesamWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "ch.sesam.remote.recent", provider: RecentProvider()) { entry in
            if #available(iOS 17.0, *) {
                RecentView(entry: entry).containerBackground(.fill.tertiary, for: .widget)
            } else {
                RecentView(entry: entry).padding()
            }
        }
        .configurationDisplayName("Zuletzt verwendet")
        .description("Die zuletzt geöffneten Verbindungen, ein Tipp öffnet sie in Sesam.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}
