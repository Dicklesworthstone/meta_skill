//! Definitions for secret service interactions

// DBus Name
pub const SS_DBUS_NAME: &str = "org.freedesktop.secrets";

// Item Properties
pub const SS_ITEM_LABEL: &str = "org.freedesktop.Secret.Item.Label";
pub const SS_ITEM_ATTRIBUTES: &str = "org.freedesktop.Secret.Item.Attributes";

// Algorithm Names
pub const ALGORITHM_PLAIN: &str = "plain";
pub const ALGORITHM_DH: &str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";

// Collection properties
pub const SS_COLLECTION_LABEL: &str = "org.freedesktop.Secret.Collection.Label";
