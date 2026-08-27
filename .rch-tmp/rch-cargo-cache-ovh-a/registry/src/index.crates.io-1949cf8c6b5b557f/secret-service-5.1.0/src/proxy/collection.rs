//! A dbus proxy for speaking with secret service's `Collection` Interface.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Type, Value};

use super::SecretStruct;

/// A dbus proxy for speaking with secret service's `Collection` Interface.
///
/// This will derive CollectionProxy
///
/// Note that `Value` in the method signatures corresponds to `VARIANT` dbus type.
#[zbus::proxy(
    interface = "org.freedesktop.Secret.Collection",
    default_service = "org.freedesktop.Secret.Collection"
)]
pub trait Collection {
    /// Returns prompt: ObjectPath
    fn delete(&self) -> zbus::Result<OwnedObjectPath>;

    fn search_items(&self, attributes: HashMap<&str, &str>) -> zbus::Result<Vec<OwnedObjectPath>>;

    fn create_item(
        &self,
        properties: HashMap<&str, Value<'_>>,
        secret: SecretStruct,
        replace: bool,
    ) -> zbus::Result<CreateItemResult>;

    #[zbus(property)]
    fn items(&self) -> zbus::fdo::Result<Vec<ObjectPath<'_>>>;

    #[zbus(property)]
    fn label(&self) -> zbus::fdo::Result<String>;

    #[zbus(property)]
    fn set_label(&self, new_label: &str) -> zbus::fdo::Result<()>;

    #[zbus(property)]
    fn locked(&self) -> zbus::fdo::Result<bool>;

    #[zbus(property)]
    fn created(&self) -> zbus::fdo::Result<u64>;

    #[zbus(property)]
    fn modified(&self) -> zbus::fdo::Result<u64>;
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct CreateItemResult {
    pub(crate) item: OwnedObjectPath,
    pub(crate) prompt: OwnedObjectPath,
}
