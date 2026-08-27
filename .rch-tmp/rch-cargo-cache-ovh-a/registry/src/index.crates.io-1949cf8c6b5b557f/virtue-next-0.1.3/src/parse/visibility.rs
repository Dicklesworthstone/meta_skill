use super::utils::assume_group;
use super::utils::assume_ident;
use super::utils::ident_eq;
use crate::Result;
use crate::prelude::Delimiter;
use crate::prelude::TokenTree;
use std::iter::Peekable;

/// The visibility of a struct, enum, field, etc
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Visibility {
    /// Default visibility. Most items are private by default.
    Default,

    /// Public visibility
    Pub,
}

impl Visibility {
    /// Try to take visibility from the input
    ///
    /// # Panics
    ///
    /// Panics if an internal invariant is violated.
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn try_take(input: &mut Peekable<impl Iterator<Item = TokenTree>>) -> Result<Self> {
        match input.peek() {
            | Some(TokenTree::Ident(ident)) if ident_eq(ident, "pub") => {
                // Consume this token
                assume_ident(input.next());

                // check if the next token is `pub(...)`
                if let Some(TokenTree::Group(g)) = input.peek()
                    && g.delimiter() == Delimiter::Parenthesis
                {
                    // check if this is one of:
                    // - pub ( crate )
                    // - pub ( self )
                    // - pub ( super )
                    // - pub ( in ... )
                    if let Some(TokenTree::Ident(i)) = g.stream().into_iter().next()
                        && matches!(i.to_string().as_str(), "crate" | "self" | "super" | "in")
                    {
                        // it is, ignore this token
                        assume_group(input.next());
                    }
                }

                Ok(Self::Pub)
            },
            | Some(TokenTree::Group(group)) => {
                // sometimes this is a group instead of an ident
                // e.g. when used in `bitflags! {}`
                let mut iter = group.stream().into_iter();
                let tokens = (iter.next(), iter.next());
                match tokens {
                    | (Some(TokenTree::Ident(ident)), None) if ident_eq(&ident, "pub") => {
                        // Consume this token
                        assume_group(input.next());

                        // check if the next token is `pub(...)`
                        if let Some(TokenTree::Group(_)) = input.peek() {
                            // we just consume the visibility, we're not actually using it for generation
                            assume_group(input.next());
                        }
                        Ok(Self::Pub)
                    },
                    | _ => Ok(Self::Default),
                }
            },
            | _ => Ok(Self::Default),
        }
    }
}

#[test]
fn test_visibility_try_take() {
    use crate::token_stream;

    assert_eq!(
        Visibility::Default,
        Visibility::try_take(&mut token_stream("")).unwrap()
    );
    assert_eq!(
        Visibility::Pub,
        Visibility::try_take(&mut token_stream("pub")).unwrap()
    );
    assert_eq!(
        Visibility::Pub,
        Visibility::try_take(&mut token_stream(" pub ")).unwrap(),
    );
    assert_eq!(
        Visibility::Pub,
        Visibility::try_take(&mut token_stream("\tpub\t")).unwrap()
    );
    assert_eq!(
        Visibility::Pub,
        Visibility::try_take(&mut token_stream("pub(crate)")).unwrap()
    );
    assert_eq!(
        Visibility::Pub,
        Visibility::try_take(&mut token_stream(" pub ( crate ) ")).unwrap()
    );
    assert_eq!(
        Visibility::Pub,
        Visibility::try_take(&mut token_stream("\tpub\t(\tcrate\t)\t")).unwrap()
    );

    assert_eq!(
        Visibility::Default,
        Visibility::try_take(&mut token_stream("pb")).unwrap()
    );
}
