//! Native delivery under the signed app's bundle identity, never Script Editor.

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use block2::{DynBlock, RcBlock};
use objc2::{
    AnyThread, define_class, msg_send,
    rc::Retained,
    runtime::{Bool, ProtocolObject},
};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSound,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use super::DesktopNotification;

static READY: AtomicBool = AtomicBool::new(false);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

define_class!(
    // SAFETY: NSObject has no subclass requirements; this delegate has no ivars
    // or mutable state and its callback can run on any notification queue.
    #[unsafe(super = NSObject)]
    #[thread_kind = AnyThread]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            // Eligibility already suppresses the conversation being read.
            // Other conversations must still notify while the app is focused.
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
    }
);

// SAFETY: There is no state to mutate, and NSObject's reference counting and
// this delegate's only callback are safe on notification worker queues.
unsafe impl Send for NotificationDelegate {}
unsafe impl Sync for NotificationDelegate {}

pub(super) fn initialize() -> Result<(), String> {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return Ok(());
    }
    // Calling currentNotificationCenter from a bare Cargo executable raises an
    // Objective-C exception. Require our real bundle before touching the API.
    let bundle = NSBundle::mainBundle();
    if bundle
        .bundleIdentifier()
        .as_deref()
        .map(NSString::to_string)
        .as_deref()
        != Some("com.echonet.superplatinum")
    {
        return Err(
            "macOS notifications require the app bundle; launch scripts/macos-app.sh --run".into(),
        );
    }
    static DELEGATE: OnceLock<Retained<NotificationDelegate>> = OnceLock::new();
    let delegate = DELEGATE.get_or_init(|| {
        // SAFETY: NSObject init has no arguments or additional requirements.
        unsafe { msg_send![NotificationDelegate::alloc(), init] }
    });
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.setDelegate(Some(ProtocolObject::from_ref(&**delegate)));
    READY.store(true, Ordering::Release);
    let completion = RcBlock::new(|granted: Bool, error: *mut NSError| {
        report_error(error);
        if !granted.as_bool() {
            eprintln!(
                "super-platinum: macOS notifications are disabled; allow Super Platinum in System Settings > Notifications"
            );
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &completion,
    );
    Ok(())
}

pub(super) fn show(notification: &DesktopNotification) {
    if !READY.load(Ordering::Acquire) {
        return;
    }
    objc2::rc::autoreleasepool(|_| {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notification.title));
        content.setBody(&NSString::from_str(&notification.body));
        if let Some(name) = notification
            .sound
            .as_deref()
            .and_then(super::sounds::native_name)
        {
            let sound = UNNotificationSound::soundNamed(&NSString::from_str(&name));
            content.setSound(Some(&sound));
        }
        let identifier = NSString::from_str(&format!(
            "super-platinum-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );
        let completion = RcBlock::new(report_error);
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, Some(&completion));
    });
}

fn report_error(error: *mut NSError) {
    // SAFETY: Apple's completion handlers pass either null or an NSError valid
    // for the duration of the callback; we don't retain the borrowed pointer.
    if let Some(error) = unsafe { error.as_ref() } {
        eprintln!(
            "super-platinum: native notification failed: {}",
            error.localizedDescription()
        );
    }
}
