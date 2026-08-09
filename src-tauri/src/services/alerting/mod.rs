pub(crate) mod desktop_notification;

pub(crate) use desktop_notification::{
    DesktopNotificationAdapter, DesktopNotificationError, DesktopNotificationPayload,
    DesktopNotificationPermission, TauriDesktopNotificationAdapter,
};
