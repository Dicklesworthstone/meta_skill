//! A dbus proxy for speaking with secret service's `Item` Interface.

use std::collections::HashMap;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use super::SecretStruct;

/// A dbus proxy for speaking with secret service's `Item` Interface.
///
/// This will derive ItemProxy
#[zbus::proxy(
    interface = "org.freedesktop.Secret.Item",
    default_service = "org.freedesktop.Secret.Item"
)]
pub trait Item {
    fn delete(&self) -> zbus::Result<OwnedObjectPath>;

    /// returns `Secret`
    fn get_secret(&self, session: &ObjectPath<'_>) -> zbus::Result<SecretStruct>;

    fn set_secret(&self, secret: SecretStruct) -> zbus::Result<()>;

    #[zbus(property)]
    fn locked(&self) -> zbus::fdo::Result<bool>;

    #[zbus(property)]
    fn attributes(&self) -> zbus::fdo::Result<HashMap<String, String>>;

    #[zbus(property)]
    fn set_attributes(&self, attributes: HashMap<&str, &str>) -> zbus::fdo::Result<()>;

    #[zbus(property)]
    fn label(&self) -> zbus::fdo::Result<String>;

    #[zbus(property)]
    fn set_label(&self, new_label: &str) -> zbus::fdo::Result<()>;

    #[zbus(property)]
    fn created(&self) -> zbus::fdo::Result<u64>;

    #[zbus(property)]
    fn modified(&self) -> zbus::fdo::Result<u64>;
}
