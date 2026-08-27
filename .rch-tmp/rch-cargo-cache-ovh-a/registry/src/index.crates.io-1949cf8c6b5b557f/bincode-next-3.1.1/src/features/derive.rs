#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use bincode_derive_next::BitPacked;
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use bincode_derive_next::BorrowDecode;
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use bincode_derive_next::Decode;
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use bincode_derive_next::Encode;
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use bincode_derive_next::Fingerprint;

#[cfg(feature = "static-size")]
#[cfg_attr(docsrs, doc(cfg(all(feature = "static-size", feature = "derive"))))]
pub use bincode_derive_next::StaticSize;

#[cfg(feature = "zero-copy")]
#[cfg_attr(docsrs, doc(cfg(all(feature = "zero-copy", feature = "derive"))))]
pub use bincode_derive_next::ZeroCopy;
