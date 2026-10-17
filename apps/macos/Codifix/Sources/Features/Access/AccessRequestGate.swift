import Foundation

struct AccessRequestTicket: Equatable, Sendable {
    let id: UUID
    let revision: Int
}

/// Tracks the latest Access read so connection changes and later refreshes
/// cannot allow stale list data to replace the current selection.
struct AccessRequestGate: Sendable {
    private(set) var current: AccessRequestTicket?

    mutating func begin(revision: Int) -> AccessRequestTicket {
        let ticket = AccessRequestTicket(id: UUID(), revision: revision)
        current = ticket
        return ticket
    }

    mutating func invalidate() { current = nil }

    func accepts(_ ticket: AccessRequestTicket, revision: Int) -> Bool {
        current == ticket && ticket.revision == revision
    }
}
