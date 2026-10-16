import SwiftUI

struct WorkMemorySelection: Identifiable {
    let id: String
}

struct WorkBadge: View {
    enum Tone { case blue, green, amber, secondary }
    let title: String
    var tone: Tone = .secondary

    var body: some View {
        Text(title)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundStyle(foreground)
            .background(background, in: RoundedRectangle(cornerRadius: 7))
    }

    private var foreground: Color {
        switch tone {
        case .blue: .blue
        case .green: .green
        case .amber: Color(nsColor: .systemOrange)
        case .secondary: .secondary
        }
    }

    private var background: Color {
        switch tone {
        case .blue: .blue.opacity(0.10)
        case .green: .green.opacity(0.10)
        case .amber: Color(nsColor: .systemOrange).opacity(0.12)
        case .secondary: Color(nsColor: .quaternarySystemFill)
        }
    }
}

struct WorkNotice: View {
    enum Tone { case info, warning, error }
    let title: String
    let message: String
    var tone: Tone = .info

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(titleColor)
            Text(message).font(.callout).foregroundStyle(.primary).fixedSize(horizontal: false, vertical: true)
        }
        .padding(13)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(fill, in: RoundedRectangle(cornerRadius: 9))
        .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(Color(nsColor: .separatorColor).opacity(0.55), lineWidth: 0.7))
        .accessibilityElement(children: .combine)
    }

    private var titleColor: Color {
        switch tone {
        case .info: .blue
        case .warning: Color(nsColor: .systemOrange)
        case .error: .red
        }
    }

    private var fill: Color {
        switch tone {
        case .info: .blue.opacity(0.08)
        case .warning: Color(nsColor: .systemOrange).opacity(0.12)
        case .error: .red.opacity(0.09)
        }
    }
}

struct WorkLoadingOrError: View {
    let context: FeatureContext
    let loading: Bool
    let error: Error?
    let hasContent: Bool
    let emptyTitle: String
    let emptyMessage: String
    var retry: (() -> Void)?

    var body: some View {
        if loading && !hasContent {
            ProgressView("Loading from the connected service…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let error, !hasContent {
            ContentUnavailableView {
                Label(errorTitle, systemImage: errorSymbol)
            } description: {
                VStack(spacing: 8) {
                    Text(errorMessage)
                    Text("Service: \(context.profile.baseURL.host ?? context.profile.baseURL.absoluteString) · Project: \(context.profile.projectSlug)")
                        .font(.caption).foregroundStyle(.secondary)
                }.frame(maxWidth: 470)
            } actions: {
                if isAuthorizationError(error, status: 401) || isAuthorizationError(error, status: 403) {
                    Button("Review connection settings…") {
                        NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
                    }
                }
                if let retry { Button("Try again", action: retry).buttonStyle(.bordered) }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if !hasContent {
            ContentUnavailableView(emptyTitle, systemImage: "tray", description: Text(emptyMessage))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var errorTitle: String {
        if isAuthorizationError(error!, status: 401) { return "Authentication required" }
        if isAuthorizationError(error!, status: 403) { return "Restricted access" }
        if workErrorIsOffline(error!) { return "Service is offline" }
        if let serviceError = error as? MemoryServiceError {
            if case .httpStatus = serviceError { return "Service unavailable" }
        }
        return "Could not load this view"
    }

    private var errorSymbol: String {
        if isAuthorizationError(error!, status: 401) { return "person.crop.circle.badge.exclamationmark" }
        if isAuthorizationError(error!, status: 403) { return "lock" }
        return "wifi.slash"
    }

    private var errorMessage: String {
        if isAuthorizationError(error!, status: 401) { return "The service did not accept this connection’s credentials." }
        if isAuthorizationError(error!, status: 403) { return "This account cannot read the selected project." }
        if workErrorIsOffline(error!) { return "Could not reach the selected service. Check the connection and try again." }
        return error!.localizedDescription
    }
}

func workDate(_ value: String?) -> String {
    guard let value, !value.isEmpty else { return "Time not reported" }
    if let date = ISO8601DateFormatter().date(from: value) {
        return date.formatted(date: .abbreviated, time: .shortened)
    }
    let fractional = ISO8601DateFormatter()
    fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = fractional.date(from: value) { return date.formatted(date: .abbreviated, time: .shortened) }
    return value
}

func workRelativeDate(_ value: Date?) -> String {
    guard let value else { return "Time not reported" }
    return value.formatted(.relative(presentation: .named))
}

func workErrorIsOffline(_ error: Error) -> Bool {
    if let urlError = error as? URLError { return [.notConnectedToInternet, .cannotConnectToHost, .timedOut, .networkConnectionLost].contains(urlError.code) }
    return false
}
