use crate::index::{Change, ChangeRef};
use crate::rewrites;
use crate::rewrites::tracker::ChangeKind;
use crate::tree::visit::Relation;
use bstr::BStr;
use gix_object::tree;
use std::borrow::Cow;

impl ChangeRef<'_, '_> {
    /// Copy everything into an owned version of this instance.
    pub fn into_owned(self) -> Change {
        match self {
            ChangeRef::Addition {
                location,
                index,
                entry_mode,
                id,
            } => ChangeRef::Addition {
                location: Cow::Owned(location.into_owned()),
                index,
                entry_mode,
                id: Cow::Owned(id.into_owned()),
            },
            ChangeRef::Deletion {
                location,
                index,
                entry_mode,
                id,
            } => ChangeRef::Deletion {
                location: Cow::Owned(location.into_owned()),
                index,
                entry_mode,
                id: Cow::Owned(id.into_owned()),
            },
            ChangeRef::Modification {
                location,
                previous_index,
                previous_entry_mode,
                previous_id,
                index,
                entry_mode,
                id,
            } => ChangeRef::Modification {
                location: Cow::Owned(location.into_owned()),
                previous_index,
                previous_entry_mode,
                previous_id: Cow::Owned(previous_id.into_owned()),
                index,
                entry_mode,
                id: Cow::Owned(id.into_owned()),
            },
            ChangeRef::Rewrite {
                source_location,
                source_index,
                source_entry_mode,
                source_id,
                location,
                index,
                entry_mode,
                id,
                copy,
            } => ChangeRef::Rewrite {
                source_location: Cow::Owned(source_location.into_owned()),
                source_index,
                source_entry_mode,
                source_id: Cow::Owned(source_id.into_owned()),
                location: Cow::Owned(location.into_owned()),
                index,
                entry_mode,
                id: Cow::Owned(id.into_owned()),
                copy,
            },
        }
    }
}

impl ChangeRef<'_, '_> {
    /// Return all shared fields among all variants: `(location, index, entry_mode, id)`
    ///
    /// In case of rewrites, the fields return to the current change.
    pub fn fields(&self) -> (&BStr, usize, gix_index::entry::Mode, &gix_hash::oid) {
        match self {
            ChangeRef::Addition {
                location,
                index,
                entry_mode,
                id,
                ..
            }
            | ChangeRef::Deletion {
                location,
                index,
                entry_mode,
                id,
                ..
            }
            | ChangeRef::Modification {
                location,
                index,
                entry_mode,
                id,
                ..
            }
            | ChangeRef::Rewrite {
                location,
                index,
                entry_mode,
                id,
                ..
            } => (location.as_ref(), *index, *entry_mode, id),
        }
    }
}

impl rewrites::tracker::Change for ChangeRef<'_, '_> {
    fn id(&self) -> &gix_hash::oid {
        match self {
            ChangeRef::Addition { id, .. } | ChangeRef::Deletion { id, .. } | ChangeRef::Modification { id, .. } => {
                id.as_ref()
            }
            ChangeRef::Rewrite { .. } => {
                unreachable!("BUG")
            }
        }
    }

    fn relation(&self) -> Option<Relation> {
        None
    }

    fn kind(&self) -> ChangeKind {
        match self {
            ChangeRef::Addition { .. } => ChangeKind::Addition,
            ChangeRef::Deletion { .. } => ChangeKind::Deletion,
            ChangeRef::Modification { .. } => ChangeKind::Modification,
            ChangeRef::Rewrite { .. } => {
                unreachable!("BUG: rewrites can't be determined ahead of time")
            }
        }
    }

    fn entry_mode(&self) -> tree::EntryMode {
        match self {
            ChangeRef::Addition { entry_mode, .. }
            | ChangeRef::Deletion { entry_mode, .. }
            | ChangeRef::Modification { entry_mode, .. }
            | ChangeRef::Rewrite { entry_mode, .. } => {
                entry_mode
                    .to_tree_entry_mode()
                    // Default is for the impossible case - just don't let it participate in rename tracking.
                    .unwrap_or(tree::EntryKind::Tree.into())
            }
        }
    }

    fn id_and_entry_mode(&self) -> (&gix_hash::oid, tree::EntryMode) {
        match self {
            ChangeRef::Addition { id, entry_mode, .. }
            | ChangeRef::Deletion { id, entry_mode, .. }
            | ChangeRef::Modification { id, entry_mode, .. }
            | ChangeRef::Rewrite { id, entry_mode, .. } => {
                (
                    id,
                    entry_mode
                        .to_tree_entry_mode()
                        // Default is for the impossible case - just don't let it participate in rename tracking.
                        .unwrap_or(tree::EntryKind::Tree.into()),
                )
            }
        }
    }
}
