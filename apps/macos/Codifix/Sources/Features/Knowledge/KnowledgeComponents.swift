import SwiftUI

struct KnowledgeCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        content
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color(nsColor: .separatorColor).opacity(0.65), lineWidth: 0.7)
            }
    }
}

struct KnowledgePageHeader: View {
    let title: String
    let subtitle: String
    var actionTitle: String? = nil
    var action: (() -> Void)? = nil

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.title2.weight(.semibold))
                Text(subtitle).font(.callout).foregroundStyle(.secondary)
            }
            Spacer(minLength: 12)
            if let actionTitle, let action {
                Button(actionTitle, action: action)
                    .buttonStyle(.bordered)
            }
        }
    }
}

struct KnowledgeConnectionHint: View {
    let context: FeatureContext

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Existing Codifix service", systemImage: "externaldrive.connected.to.line.below")
                .font(.headline)
            LabeledContent("Service", value: context.profile.baseURL.host ?? context.profile.baseURL.absoluteString)
            LabeledContent("Project", value: context.profile.projectSlug)
            Button("Review connection settings…") {
                NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
            }
            .buttonStyle(.bordered)
            .padding(.top, 3)
        }
        .font(.callout)
    }
}

struct KnowledgeStateView: View {
    enum Kind {
        case loading
        case empty
        case noMatches
        case insufficientEvidence
        case authRequired
        case restricted
        case unavailable
        case failed(String)
    }

    let kind: Kind
    let context: FeatureContext
    var retry: (() -> Void)?
    var clearFilters: (() -> Void)?

    var body: some View {
        ContentUnavailableView {
            Label(title, systemImage: symbol)
        } description: {
            VStack(spacing: 8) {
                Text(message)
                if case .empty = kind {
                    KnowledgeConnectionHint(context: context)
                }
            }
            .frame(maxWidth: 460)
        } actions: {
            if case .noMatches = kind, let clearFilters {
                Button("Clear filters", action: clearFilters).buttonStyle(.bordered)
            } else if let retry {
                Button("Try again", action: retry).buttonStyle(.bordered)
            } else if case .authRequired = kind {
                Button("Review connection settings…") {
                    NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
                }
                .buttonStyle(.bordered)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var title: String {
        switch kind {
        case .loading: "Loading Codifix…"
        case .empty: "No memories in this project"
        case .noMatches: "No memories match these filters"
        case .insufficientEvidence: "More evidence is needed"
        case .authRequired: "Authentication required"
        case .restricted: "Restricted access"
        case .unavailable: "Service unavailable"
        case .failed: "Could not load this view"
        }
    }

    private var message: String {
        switch kind {
        case .loading: "Fetching current records from the selected service."
        case .empty: "This connected project currently has no readable canonical memories."
        case .noMatches: "Try a broader search or remove a type, status, or tag filter."
        case .insufficientEvidence: "The selected project did not return enough evidence to support an answer."
        case .authRequired: "The service did not accept the credentials for this connection."
        case .restricted: "This account does not have permission to read this project."
        case .unavailable: "Check the selected service and project connection."
        case let .failed(message): message
        }
    }

    private var symbol: String {
        switch kind {
        case .loading: "arrow.triangle.2.circlepath"
        case .empty: "square.stack.3d.up"
        case .noMatches: "line.3.horizontal.decrease.circle"
        case .insufficientEvidence: "text.magnifyingglass"
        case .authRequired: "person.crop.circle.badge.exclamationmark"
        case .restricted: "lock"
        case .unavailable: "wifi.slash"
        case .failed: "exclamationmark.triangle"
        }
    }
}

func knowledgeDate(_ date: Date) -> String {
    date.formatted(date: .abbreviated, time: .shortened)
}

func isAuthorizationError(_ error: Error, status: Int) -> Bool {
    if case let MemoryServiceError.httpStatus(code, _) = error { return code == status }
    if status == 401, case MemoryServiceError.unauthorized = error { return true }
    if status == 403, case MemoryServiceError.forbidden = error { return true }
    return false
}

func knowledgeErrorKind(_ error: Error, fallbackMessage: String) -> KnowledgeStateView.Kind {
    switch error {
    case MemoryServiceError.unauthorized:
        return .authRequired
    case MemoryServiceError.forbidden:
        return .restricted
    case let MemoryServiceError.httpStatus(status, _) where status == 401:
        return .authRequired
    case let MemoryServiceError.httpStatus(status, _) where status == 403:
        return .restricted
    default:
        let nsError = error as NSError
        if nsError.domain == NSURLErrorDomain,
           [-1001, -1003, -1004, -1005, -1009].contains(nsError.code) {
            return .unavailable
        }
        if fallbackMessage.localizedCaseInsensitiveContains("offline") || fallbackMessage.localizedCaseInsensitiveContains("connect") {
            return .unavailable
        }
        return .failed(fallbackMessage)
    }
}
