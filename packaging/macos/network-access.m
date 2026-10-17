// SPDX-License-Identifier: AGPL-3.0-or-later
// A visible, credential-free request for macOS Local Network permission.
#import <AppKit/AppKit.h>
#import <CommonCrypto/CommonDigest.h>
#import <signal.h>

static BOOL pinnedExecutable(NSString *executable, NSString *expected) {
    NSData *bytes = [NSData dataWithContentsOfFile:executable options:NSDataReadingMappedIfSafe error:nil];
    if (!bytes || bytes.length > 1024 * 1024 * 1024) return NO;
    unsigned char digest[CC_SHA256_DIGEST_LENGTH];
    CC_SHA256(bytes.bytes, (CC_LONG)bytes.length, digest);
    NSMutableString *actual = [NSMutableString string];
    for (unsigned i = 0; i < sizeof(digest); ++i) [actual appendFormat:@"%02x", digest[i]];
    return [actual isEqualToString:expected];
}

static int synchronize(int argc, const char *argv[]) {
    if (argc != 14 || strcmp(argv[2], "--python") || strcmp(argv[4], "--python-sha256") ||
        strcmp(argv[6], "--operator") || strcmp(argv[8], "--operator-sha256") ||
        strcmp(argv[10], "--journal") || strcmp(argv[12], "--subscription")) return 2;
    NSString *executable = @(argv[3]), *expected = @(argv[5]), *operator = @(argv[7]);
    NSString *operatorDigest = @(argv[9]), *journal = @(argv[11]), *subscription = @(argv[13]);
    if (![executable isAbsolutePath] || ![operator isAbsolutePath] || ![journal isAbsolutePath] ||
        expected.length != 64 || operatorDigest.length != 64 || ![[NSUUID alloc] initWithUUIDString:subscription]) return 2;
    if (!pinnedExecutable(executable, expected) || !pinnedExecutable(operator, operatorDigest)) return 2;
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
    __block NSTask *child = nil;
    __block BOOL stopping = NO;
    __block void (^start)(void);
    __block __weak void (^weakStart)(void);
    start = ^{
        if (stopping) return;
        if (!pinnedExecutable(executable, expected) || !pinnedExecutable(operator, operatorDigest)) {
            fputs("Pinned onboarding runtime changed; resume the reviewed upgrade.\n", stderr);
            stopping = YES;
            [NSApp stop:nil];
            return;
        }
        child = [NSTask new];
        child.executableURL = [NSURL fileURLWithPath:executable];
        // Preserve the operator's journal, profile, executable and permission
        // validation before it execs the existing authenticated Codifix path.
        child.arguments = @[@"-I", @"-S", @"-B", operator, @"run-sync", @"--journal", journal, @"--subscription", subscription];
        child.terminationHandler = ^(NSTask *finished) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (!stopping) dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC), dispatch_get_main_queue(), weakStart);
            });
        };
        NSError *error = nil;
        if (![child launchAndReturnError:&error]) {
            fputs("Codifix synchronization could not start; retrying.\n", stderr);
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC), dispatch_get_main_queue(), weakStart);
        }
    };
    weakStart = start;
    signal(SIGTERM, SIG_IGN);
    signal(SIGINT, SIG_IGN);
    dispatch_source_t termination = dispatch_source_create(DISPATCH_SOURCE_TYPE_SIGNAL, SIGTERM, 0, dispatch_get_main_queue());
    dispatch_source_set_event_handler(termination, ^{ stopping = YES; [child terminate]; [NSApp stop:nil]; });
    dispatch_resume(termination);
    dispatch_source_t interrupt = dispatch_source_create(DISPATCH_SOURCE_TYPE_SIGNAL, SIGINT, 0, dispatch_get_main_queue());
    dispatch_source_set_event_handler(interrupt, ^{ stopping = YES; [child terminate]; [NSApp stop:nil]; });
    dispatch_resume(interrupt);
    start();
    [NSApp run];
    if (child.running) {
        pid_t ownedPID = child.processIdentifier;
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC), dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^{
            if (child.running && child.processIdentifier == ownedPID) kill(ownedPID, SIGKILL);
        });
        [child waitUntilExit];
    }
    dispatch_source_cancel(termination);
    dispatch_source_cancel(interrupt);
    start = nil;
    return 0;
}

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        if (argc > 1 && !strcmp(argv[1], "--sync")) return synchronize(argc, argv);
        if (argc != 2) return 2;
        NSURLComponents *origin = [NSURLComponents componentsWithString:@(argv[1])];
        if (![origin.scheme isEqualToString:@"https"] || !origin.host.length ||
            origin.user || origin.password || origin.query || origin.fragment ||
            (origin.path.length && ![origin.path isEqualToString:@"/"])) return 2;
        origin.path = @"/healthz";
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        NSAlert *alert = [NSAlert new];
        alert.messageText = @"Enable Codifix synchronization";
        alert.informativeText = @"Allow local-network access when macOS asks. Codifix needs this to synchronize your selected projects with your company server.";
        [alert addButtonWithTitle:@"Close"];
        __block BOOL connected = NO;
        __block BOOL pending = NO;
        NSURLSessionConfiguration *configuration = [NSURLSessionConfiguration ephemeralSessionConfiguration];
        configuration.timeoutIntervalForRequest = 8;
        configuration.HTTPShouldSetCookies = NO;
        NSURLSession *session = [NSURLSession sessionWithConfiguration:configuration];
        void (^probe)(void) = ^{
            if (pending || connected) return;
            pending = YES;
            [[session dataTaskWithURL:origin.URL completionHandler:^(NSData *data, NSURLResponse *response, NSError *error) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    pending = NO;
                    if (response && !error) {
                        connected = YES;
                        alert.messageText = @"Codifix can reach your company server";
                        alert.informativeText = @"Local-network access is ready. You can close this window; synchronization runs in the background.";
                    }
                });
            }] resume];
        };
        NSTimer *timer = [NSTimer timerWithTimeInterval:5 repeats:YES block:^(NSTimer *t) { probe(); }];
        [[NSRunLoop mainRunLoop] addTimer:timer forMode:NSRunLoopCommonModes];
        probe();
        [NSApp activateIgnoringOtherApps:YES];
        [alert runModal];
        [timer invalidate];
        [session invalidateAndCancel];
        return connected ? 0 : 1;
    }
}
