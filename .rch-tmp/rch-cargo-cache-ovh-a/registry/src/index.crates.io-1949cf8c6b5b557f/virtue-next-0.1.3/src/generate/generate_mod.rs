use super::GenEnum;
use super::GenStruct;
use super::Impl;
use super::Parent;
use super::StreamBuilder;
use crate::Result;
use crate::parse::Visibility;
use crate::prelude::Delimiter;
use crate::prelude::Ident;
use crate::prelude::Span;

/// Builder for generating a module with its contents.
pub struct GenerateMod<'a, P: Parent> {
    parent: &'a mut P,
    name: Ident,
    uses: Vec<StreamBuilder>,
    vis: Visibility,
    content: StreamBuilder,
}

impl<'a, P: Parent> GenerateMod<'a, P> {
    pub(crate) fn new(
        parent: &'a mut P,
        name: impl Into<String>,
    ) -> Self {
        Self {
            parent,
            name: Ident::new(name.into().as_str(), Span::call_site()),
            uses: Vec::new(),
            vis: Visibility::Default,
            content: StreamBuilder::new(),
        }
    }

    /// Add a `use ...;` to the current mod
    ///
    /// `generator.impl_mod("foo").add_use("bar")` will generate:
    ///
    /// ```ignore
    /// mod foo {
    ///     use bar;
    /// }
    /// ```
    ///
    /// This is especially useful with `.add_use("super::*");`, which will pull all parent imports into scope
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails.
    pub fn add_use(
        &mut self,
        r#use: impl AsRef<str>,
    ) -> Result {
        let mut builder = StreamBuilder::new();
        builder.ident_str("use").push_parsed(r#use)?.punct(';');
        self.uses.push(builder);
        Ok(())
    }

    /// Generate a struct with the given name. See [`GenStruct`] for more info.
    pub fn generate_struct(
        &mut self,
        name: impl Into<String>,
    ) -> GenStruct<'_, Self> {
        GenStruct::new(self, name)
    }

    /// Generate an enum with the given name. See [`GenEnum`] for more info.
    pub fn generate_enum(
        &mut self,
        name: impl Into<String>,
    ) -> GenEnum<'_, Self> {
        GenEnum::new(self, name)
    }

    /// Generate an `impl <name>` implementation. See [`Impl`] for more information.
    pub fn r#impl(
        &mut self,
        name: impl Into<String>,
    ) -> Impl<'_, Self> {
        Impl::new(self, name)
    }

    /// Generate an `impl <name>` implementation. See [`Impl`] for more information.
    ///
    /// Alias for [`impl`] which doesn't need a `r#` prefix.
    ///
    /// [`impl`]: #method.impl
    ///
    /// # Panics
    ///
    /// Panics if an internal invariant is violated.
    pub fn generate_impl(
        &mut self,
        name: impl Into<String>,
    ) -> Impl<'_, Self> {
        Impl::new(self, name)
    }
}

impl<P: Parent> Drop for GenerateMod<'_, P> {
    fn drop(&mut self) {
        let mut builder = StreamBuilder::new();
        if self.vis == Visibility::Pub {
            builder.ident_str("pub");
        }
        builder
            .ident_str("mod")
            .ident(self.name.clone())
            .group(Delimiter::Brace, |group| {
                for r#use in std::mem::take(&mut self.uses) {
                    group.append(r#use);
                }
                group.append(std::mem::take(&mut self.content));
                Ok(())
            })
            .unwrap();

        self.parent.append(builder);
    }
}

impl<P: Parent> Parent for GenerateMod<'_, P> {
    fn append(
        &mut self,
        builder: StreamBuilder,
    ) {
        self.content.append(builder);
    }

    fn name(&self) -> &crate::prelude::Ident {
        &self.name
    }

    fn generics(&self) -> Option<&crate::parse::Generics> {
        None
    }

    fn generic_constraints(&self) -> Option<&crate::parse::GenericConstraints> {
        None
    }
}
