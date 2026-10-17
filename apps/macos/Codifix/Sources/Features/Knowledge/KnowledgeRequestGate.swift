import Foundation

struct KnowledgeRequestTicket: Equatable, Sendable {
    let token: UUID
    let revision: Int
    let resourceID: String?
}

/// Tracks the most recent request for a feature resource so obsolete network
/// responses cannot replace the currently selected connection or record.
struct KnowledgeRequestGate: Sendable {
    private(set) var current: KnowledgeRequestTicket?

    mutating func begin(revision: Int, resourceID: String? = nil) -> KnowledgeRequestTicket {
        let ticket = KnowledgeRequestTicket(token: UUID(), revision: revision, resourceID: resourceID)
        current = ticket
        return ticket
    }

    mutating func invalidate() { current = nil }

    func accepts(_ ticket: KnowledgeRequestTicket, revision: Int, resourceID: String? = nil) -> Bool {
        current == ticket && ticket.revision == revision && ticket.resourceID == resourceID
    }
}
