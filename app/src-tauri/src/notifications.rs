//! Desktop notifications. On macOS through Apple's User Notifications, which
//! asks for permission once and lists the app in System Settings →
//! Notifications (the older way used by the notification plugin no longer
//! shows anything on recent macOS). Elsewhere through the Tauri plugin.

/// Error text when the system does not allow notifications; the UI offers a shortcut.
pub const DENIED: &str = "Notifications are turned off for Pigeonhole in System Settings.";

#[cfg(target_os = "macos")]
mod mac {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread};
    use objc2_foundation::{NSBundle, NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions,
        UNNotificationRequest, UNNotificationSettings, UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use std::ptr::NonNull;
    use std::sync::{mpsc, Once};

    define_class!(
        // Shows the banner also while Pigeonhole is the app in front.
        #[unsafe(super(NSObject))]
        #[name = "PigeonholeNotificationDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                handler.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound,));
            }
        }
    );

    /// Only an app bundle can use the notification center (not `tauri dev`).
    pub fn available() -> bool {
        NSBundle::mainBundle().bundleIdentifier().is_some()
    }

    fn center() -> Retained<UNUserNotificationCenter> {
        static DELEGATE: Once = Once::new();
        let center = UNUserNotificationCenter::currentNotificationCenter();
        DELEGATE.call_once(|| {
            let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
            center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            // The center keeps a weak reference: the delegate lives as long as the app.
            std::mem::forget(delegate);
        });
        center
    }

    fn status(center: &UNUserNotificationCenter) -> UNAuthorizationStatus {
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            let _ = tx.send(unsafe { settings.as_ref() }.authorizationStatus());
        });
        center.getNotificationSettingsWithCompletionHandler(&block);
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or(UNAuthorizationStatus::NotDetermined)
    }

    /// Asks once (macOS shows its dialog); true when allowed. May wait for the answer.
    pub fn allowed() -> bool {
        let center = center();
        match status(&center) {
            UNAuthorizationStatus::NotDetermined => {
                let (tx, rx) = mpsc::channel();
                let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
                    let _ = tx.send(granted.as_bool());
                });
                center.requestAuthorizationWithOptions_completionHandler(
                    UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge,
                    &block,
                );
                rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or(false)
            }
            UNAuthorizationStatus::Denied => false,
            _ => true,
        }
    }

    pub fn show(title: &str, body: &str) -> Result<(), String> {
        if !allowed() {
            return Err(super::DENIED.into());
        }
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let id = format!("pigeonhole-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&id), &content, None);
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |error: *mut NSError| {
            let message = unsafe { error.as_ref() }.map(|e| e.localizedDescription().to_string());
            let _ = tx.send(message);
        });
        center().addNotificationRequest_withCompletionHandler(&request, Some(&block));
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Some(error)) => Err(error),
            _ => Ok(()),
        }
    }
}

/// Asks for permission now, so that the first new email is not the moment
/// macOS asks. True when notifications can be shown.
pub fn prepare() -> bool {
    #[cfg(target_os = "macos")]
    if mac::available() {
        return mac::allowed();
    }
    true
}

pub fn show<R: tauri::Runtime>(app: &tauri::AppHandle<R>, title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if mac::available() {
        return mac::show(title, body);
    }
    use tauri_plugin_notification::NotificationExt;
    app.notification().builder().title(title).body(body).show().map_err(|e| e.to_string())
}
