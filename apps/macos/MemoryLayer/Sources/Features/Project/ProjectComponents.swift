import SwiftUI

struct ProjectPageHeader<Action: View>: View {
    let title: String
    let subtitle: String
    @ViewBuilder var action: Action

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.title2.weight(.semibold))
                Text(subtitle).font(.callout).foregroundStyle(.secondary)
            }
            Spacer(minLength: 12)
            action
        }
    }
}

struct ProjectCard<Content: View>: View {
    let title: String?
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let title { Text(title).font(.headline) }
            content
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color(nsColor: .separatorColor).opacity(0.65), lineWidth: 0.7)
        }
    }
}

struct ProjectMetricCard: View {
    let value: String
    let title: String
    let note: String

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(value).font(.system(size: 27, weight: .semibold, design: .rounded)).lineLimit(1).minimumScaleFactor(0.75)
            Text(title).font(.subheadline.weight(.semibold))
            Text(note).font(.caption).foregroundStyle(.secondary).lineLimit(2)
        }
        .frame(maxWidth: .infinity, minHeight: 76, alignment: .leading)
        .padding(12)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
        .overlay { RoundedRectangle(cornerRadius: 8).stroke(Color(nsColor: .separatorColor).opacity(0.6), lineWidth: 0.7) }
    }
}

struct ProjectBanner: View {
    enum Tone { case info, warning, success, failure }
    let title: String
    let message: String
    var tone: Tone = .info
    var actionTitle: String? = nil
    var action: (() -> Void)? = nil

    var body: some View {
        HStack(alignment: .top, spacing: 11) {
            Image(systemName: symbol).foregroundStyle(color).font(.title3)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.subheadline.weight(.semibold))
                Text(message).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                if let actionTitle, let action {
                    Button(actionTitle, action: action).buttonStyle(.bordered).controlSize(.small).padding(.top, 3)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(background, in: RoundedRectangle(cornerRadius: 8))
        .overlay { RoundedRectangle(cornerRadius: 8).stroke(color.opacity(0.15), lineWidth: 0.7) }
    }

    private var color: Color {
        switch tone {
        case .info: .accentColor
        case .warning: .orange
        case .success: .green
        case .failure: .red
        }
    }
    private var background: Color {
        switch tone {
        case .info: Color.accentColor.opacity(0.08)
        case .warning: Color.orange.opacity(0.10)
        case .success: Color.green.opacity(0.09)
        case .failure: Color.red.opacity(0.08)
        }
    }
    private var symbol: String {
        switch tone {
        case .info: "info.circle.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .success: "checkmark.circle.fill"
        case .failure: "xmark.octagon.fill"
        }
    }
}

struct ProjectLoadingState: View {
    let context: FeatureContext
    let title: String
    let message: String
    let isLoading: Bool
    var error: String? = nil
    var retry: (() -> Void)? = nil

    var body: some View {
        ContentUnavailableView {
            Label(title, systemImage: error == nil ? "arrow.triangle.2.circlepath" : "exclamationmark.triangle")
        } description: {
            VStack(spacing: 8) {
                Text(message)
                if let error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
                Text("\(context.profile.baseURL.host ?? context.profile.baseURL.absoluteString) · \(context.profile.projectSlug)")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .frame(maxWidth: 440)
        } actions: {
            if !isLoading, let retry { Button("Try again", action: retry).buttonStyle(.bordered) }
            if error != nil {
                Button("Review connection settings…") {
                    NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
                }
                .buttonStyle(.bordered)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

func projectFormat(_ date: Date?) -> String {
    guard let date else { return "Not recorded" }
    return date.formatted(date: .abbreviated, time: .shortened)
}

func projectAuthorizationState(_ error: Error) -> (title: String, message: String)? {
    if case MemoryServiceError.unauthorized = error { return ("Authentication required", "Review the credentials for this service connection.") }
    if case MemoryServiceError.forbidden = error { return ("Access restricted", "This identity does not have permission to read this project.") }
    if case let MemoryServiceError.httpStatus(status, _) = error, status == 401 {
        return ("Authentication required", "Review the credentials for this service connection.")
    }
    if case let MemoryServiceError.httpStatus(status, _) = error, status == 403 {
        return ("Access restricted", "This identity does not have permission to read this project.")
    }
    return nil
}

func projectIsUnavailable(_ error: Error) -> Bool {
    let value = error as NSError
    return value.domain == NSURLErrorDomain && [-1001, -1003, -1004, -1005, -1009].contains(value.code)
}
