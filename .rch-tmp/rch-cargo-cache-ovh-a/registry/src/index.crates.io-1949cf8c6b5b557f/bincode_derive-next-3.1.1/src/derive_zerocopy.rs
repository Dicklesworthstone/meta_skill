use crate::attribute::ContainerAttributes;
use crate::attribute::ReprAttributes;
use virtue::parse::Visibility;
use virtue::prelude::*;

pub(crate) struct DeriveZeroCopy {
    pub fields: Option<Fields>,
    pub variants: Option<Vec<EnumVariant>>,
    pub attributes: ContainerAttributes,
    pub repr: ReprAttributes,
    pub visibility: Visibility,
}

impl DeriveZeroCopy {
    pub fn generate(
        self,
        generator: &mut Generator,
    ) -> Result<()> {
        self.generate_static_size(generator)?;
        self.generate_zerocopy_marker(generator)?;
        self.generate_builder(generator)?;
        self.generate_validator(generator)?;
        self.generate_deep_validator(generator)?;
        Ok(())
    }

    fn generate_static_size(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        generator
            .impl_for(format!("{}::relative_ptr::StaticSize", crate_name))
            .generate_const("SIZE", "usize")
            .with_value(|builder| {
                builder.push_parsed("core::mem::size_of::<Self>()")?;
                Ok(())
            })?;
        Ok(())
    }

    fn generate_zerocopy_marker(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        generator
            .impl_for(format!("{}::relative_ptr::ZeroCopy", crate_name))
            .make_unsafe()
            .generate_const("ALIGN", "usize")
            .with_value(|builder| {
                builder.push_parsed("core::mem::align_of::<Self>()")?;
                Ok(())
            })?;
        Ok(())
    }

    fn generate_builder(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        let target_name_ident = generator.target_name();
        let target_name = target_name_ident.to_string();
        let builder_name = format!("{}Builder", target_name);

        let endian_type = if let Some((ref e, _)) = self.attributes.endian {
            match e.as_str() {
                | "little" => format!("{}::relative_ptr::LittleEndian", crate_name),
                | "big" => format!("{}::relative_ptr::BigEndian", crate_name),
                | "native" => format!("{}::relative_ptr::NativeEndian", crate_name),
                | _ => {
                    return Err(Error::custom(
                        "Invalid endianness. Expected 'little', 'big', or 'native'",
                    ));
                },
            }
        } else {
            format!("{}::relative_ptr::NativeEndian", crate_name)
        };

        if let Some(ref variants) = self.variants {
            // 1. Define the Builder enum
            {
                let mut builder_enum = generator.generate_enum(&builder_name);
                if self.visibility == Visibility::Pub {
                    builder_enum.make_pub();
                }
                for variant in variants {
                    let v = builder_enum.add_value(variant.name.to_string());
                    if let Some(ref fields) = variant.fields {
                        match fields {
                            | Fields::Struct(s) => {
                                for (ident, field) in s.iter() {
                                    v.add_field(
                                        ident.to_string(),
                                        format!(
                                            "<{} as {}::relative_ptr::ZeroCopyType<{}>>::Builder",
                                            field.type_string(),
                                            crate_name,
                                            endian_type
                                        ),
                                    );
                                }
                            },
                            | Fields::Tuple(t) => {
                                v.make_tuple();
                                for field in t.iter() {
                                    v.add_field(
                                        "",
                                        format!(
                                            "<{} as {}::relative_ptr::ZeroCopyType<{}>>::Builder",
                                            field.type_string(),
                                            crate_name,
                                            endian_type
                                        ),
                                    );
                                }
                            },
                        }
                    } else {
                        v.make_zst();
                    }
                }
            }

            // 2. Implement ZeroCopyType
            let align_value = self.attributes.align.as_ref().map(|a| a.0).unwrap_or(0);
            generator
                .impl_trait_for_other_type(
                    format!(
                        "{}::relative_ptr::ZeroCopyType<{}>",
                        crate_name, endian_type
                    ),
                    &*target_name,
                )
                .impl_type("Builder", &*builder_name)?;

            // 3. Implement ZeroCopyBuilder
            {
                let mut impl_for = generator.impl_trait_for_other_type(
                    format!(
                        "{}::relative_ptr::ZeroCopyBuilder<{}, {}>",
                        crate_name, endian_type, align_value
                    ),
                    &*builder_name,
                );
                impl_for.impl_type("Target", &*target_name)?;

                let tag_type = if self.repr.is_u8 {
                    "u8"
                } else if self.repr.is_u16 {
                    "u16"
                } else if self.repr.is_u32 {
                    "u32"
                } else if self.repr.is_u64 {
                    "u64"
                } else if self.repr.is_i8 {
                    "i8"
                } else if self.repr.is_i16 {
                    "i16"
                } else if self.repr.is_i32 {
                    "i32"
                } else if self.repr.is_i64 {
                    "i64"
                } else {
                    "u32"
                }; // Default

                impl_for.generate_fn("build_to_target")
                    .with_inline_always()
                    .with_arg("self", "Self")
                    .with_arg("builder", format!("&mut {}::relative_ptr::ZeroBuilder", crate_name))
                    .with_arg("offset", "usize")
                    .with_return_type("Self::Target")
                    .body(|fn_body: &mut StreamBuilder| {
                        fn_body.push_parsed(format!("let data_offset = if core::mem::align_of::<Self::Target>() > core::mem::size_of::<{}>() {{ core::mem::align_of::<Self::Target>() }} else {{ core::mem::size_of::<{}>() }};", tag_type, tag_type))?;
                        fn_body.push_parsed("match self")?;
                        fn_body.group(Delimiter::Brace, |match_body: &mut StreamBuilder| {
                            let target_name_ref = &target_name;
                            let crate_name_ref = crate_name;
                            let endian_type_ref = &endian_type;
                            let tag_type_ref = tag_type;

                            for (variant_index, variant) in variants.iter().enumerate() {
                                let variant_name = variant.name.to_string();
                                let variant_fields = variant.fields.as_ref();

                                match_body.push_parsed(format!("Self::{}", variant_name))?;
                                if let Some(fields) = variant_fields {
                                    match fields {
                                        | Fields::Struct(s) => {
                                            match_body.group(Delimiter::Brace, |field_body: &mut StreamBuilder| {
                                                for (ident, _) in s.iter() {
                                                    field_body.push_parsed(format!("{},", ident))?;
                                                }
                                                Ok(())
                                            })?;
                                        },
                                        | Fields::Tuple(t) => {
                                            match_body.group(Delimiter::Parenthesis, |field_body: &mut StreamBuilder| {
                                                for i in 0..t.len() {
                                                    field_body.push_parsed(format!("__field_{},", i))?;
                                                }
                                                Ok(())
                                            })?;
                                        }
                                    }
                                }
                                match_body.puncts("=>");
                                match_body.group(Delimiter::Brace, |arm_body: &mut StreamBuilder| {
                                    arm_body.push_parsed(format!("let mut __target_val = {}::{}", target_name_ref, variant_name))?;
                                    if let Some(fields) = variant_fields {
                                        let delimiter = fields.delimiter();
                                        arm_body.group(delimiter, |field_call_body: &mut StreamBuilder| {
                                            match fields {
                                                | Fields::Struct(s) => {
                                                    for (ident, _field) in s.iter() {
                                                        field_call_body.ident_str(ident.to_string());
                                                        field_call_body.punct(':');
                                                        field_call_body.group(Delimiter::Brace, |fb: &mut StreamBuilder| {
                                                            fb.push_parsed("#[repr(C)]")?;
                                                            fb.push_parsed("struct __VariantFields")?;
                                                            fb.group(Delimiter::Brace, |struct_body: &mut StreamBuilder| {
                                                                for (i_f, f) in s.iter() {
                                                                    struct_body.push_parsed(format!("{}: {},", i_f, f.type_string()))?;
                                                                }
                                                                Ok(())
                                                            })?;
                                                            fb.punct(';');

                                                            let call = format!(
                                                                "{}::relative_ptr::ZeroCopyBuilder::<{}, _ \
                                                                >::build_to_target({}, builder, offset + data_offset + core::mem::offset_of!(__VariantFields, {}))",
                                                                crate_name_ref, endian_type_ref, ident, ident
                                                            );
                                                            fb.push_parsed(call)?;
                                                            Ok(())
                                                        })?;
                                                        field_call_body.punct(',');
                                                    }
                                                },
                                                | Fields::Tuple(t) => {
                                                    for (i, _field) in t.iter().enumerate() {
                                                        field_call_body.group(Delimiter::Brace, |fb: &mut StreamBuilder| {
                                                            fb.push_parsed("#[repr(C)]")?;
                                                            fb.push_parsed("struct __VariantFields")?;
                                                            fb.group(Delimiter::Parenthesis, |struct_body: &mut StreamBuilder| {
                                                                for f in t.iter() {
                                                                    struct_body.push_parsed(format!("{},", f.type_string()))?;
                                                                }
                                                                Ok(())
                                                            })?;
                                                            fb.punct(';');

                                                            let call = format!(
                                                                "{}::relative_ptr::ZeroCopyBuilder::<{}, _ \
                                                                >::build_to_target(__field_{}, builder, offset + data_offset + core::mem::offset_of! \
                                                                (__VariantFields, {}))",
                                                                crate_name_ref, endian_type_ref, i, i
                                                            );
                                                            fb.push_parsed(call)?;
                                                            Ok(())
                                                        })?;
                                                        field_call_body.punct(',');
                                                    }
                                                }
                                            }
                                            Ok(())
                                        })?;
                                    }
                                    arm_body.punct(';');
                                    arm_body.push_parsed(format!("builder.write::<{}>(offset, &(({} as u32) as {}));", tag_type_ref, variant_index, tag_type_ref))?;
                                    arm_body.push_parsed("__target_val")?;
                                    Ok(())
                                })?;
                                match_body.punct(',');
                            }
                            Ok(())
                        })?;
                        Ok(())
                    })?;
            }
        } else {
            let mut fields_info = Vec::new();
            if let Some(ref fields) = self.fields {
                match fields {
                    | Fields::Struct(s) => {
                        for (ident, field) in s.iter() {
                            fields_info.push((Some(ident.clone()), field.type_string()));
                        }
                    },
                    | Fields::Tuple(t) => {
                        for field in t.iter() {
                            fields_info.push((None, field.type_string()));
                        }
                    },
                }
            }

            // 1. Define the Builder struct
            {
                let mut builder_struct = generator.generate_struct(&builder_name);
                if self.visibility == Visibility::Pub {
                    builder_struct.make_pub();
                }
                for (i, (ident, type_string)) in fields_info.iter().enumerate() {
                    let name = ident
                        .as_ref()
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| format!("field_{}", i));
                    let mut f = builder_struct.add_field(
                        name,
                        format!(
                            "<{} as {}::relative_ptr::ZeroCopyType<{}>>::Builder",
                            type_string, crate_name, endian_type
                        ),
                    );
                    if self.visibility == Visibility::Pub {
                        f.make_pub();
                    }
                }
            }

            // 2. Implement ZeroCopyType for the target struct
            let align_value = self.attributes.align.as_ref().map(|a| a.0).unwrap_or(0);
            generator
                .impl_for(format!(
                    "{}::relative_ptr::ZeroCopyType<{}>",
                    crate_name, endian_type
                ))
                .impl_type("Builder", &*builder_name)?;

            // 3. Implement ZeroCopyBuilder for the Builder struct
            {
                let is_tuple = matches!(self.fields, Some(Fields::Tuple(_)));
                let mut impl_for = generator.impl_trait_for_other_type(
                    format!(
                        "{}::relative_ptr::ZeroCopyBuilder<{}, {}>",
                        crate_name, endian_type, align_value
                    ),
                    &*builder_name,
                );
                impl_for.impl_type("Target", &*target_name)?;
                impl_for.generate_fn("build_to_target")
                    .with_inline_always()
                    .with_arg("self", "Self")
                    .with_arg("builder", format!("&mut {}::relative_ptr::ZeroBuilder", crate_name))
                    .with_arg("offset", "usize")
                    .with_return_type("Self::Target")
                    .body(|fn_body: &mut StreamBuilder| {
                        fn_body.ident_str(&*target_name);
                        let delimiter = if is_tuple { Delimiter::Parenthesis } else { Delimiter::Brace };
                        fn_body.group(delimiter, |struct_body: &mut StreamBuilder| {
                            for (i, (ident, _)) in fields_info.iter().enumerate() {
                                let builder_field_name = ident.as_ref().map(|i| i.to_string()).unwrap_or_else(|| format!("field_{}", i));
                                if is_tuple {
                                    struct_body.push_parsed(format!(
                                        "{}::relative_ptr::ZeroCopyBuilder::<{}, _ \
                                        >::build_to_target(self.{}, builder, offset + core::mem::offset_of!(Self::Target, {})),",
                                        crate_name, endian_type, builder_field_name, i
                                    ))?;
                                } else {
                                    let field_name = ident.as_ref().map(|i| i.to_string()).expect("Should have ident");
                                    struct_body.push_parsed(format!(
                                        "{}: {}::relative_ptr::ZeroCopyBuilder::<{}, _ \
                                        >::build_to_target(self.{}, builder, offset + core::mem::offset_of!(Self::Target, {})),",
                                        field_name, crate_name, endian_type, builder_field_name, field_name
                                    ))?;
                                }
                            }
                            Ok(())
                        })?;
                        Ok(())
                    })?;
            }
        }

        Ok(())
    }

    fn generate_validator(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        let mut impl_for = generator.impl_for(format!("{}::relative_ptr::Validator", crate_name));

        impl_for
            .generate_fn("is_valid")
            .with_arg("&self", "")
            .with_arg("buffer", "&[u8]")
            .with_return_type("bool")
            .body(|fn_body| {
                if let Some(variants) = self.variants.as_ref() {
                    fn_body.push_parsed("match self")?;
                    fn_body.group(Delimiter::Brace, |match_body| {
                        for variant in variants {
                            match_body.push_parsed(format!("Self::{}", variant.name))?;
                            if let Some(fields) = variant.fields.as_ref() {
                                match fields {
                                    | Fields::Struct(s) => {
                                        match_body.group(Delimiter::Brace, |field_body| {
                                            for (ident, _) in s.iter() {
                                                field_body.push_parsed(format!("{},", ident))?;
                                            }
                                            Ok(())
                                        })?;
                                    },
                                    | Fields::Tuple(t) => {
                                        match_body.group(Delimiter::Parenthesis, |field_body| {
                                            for i in 0..t.len() {
                                                field_body
                                                    .push_parsed(format!("__field_{},", i))?;
                                            }
                                            Ok(())
                                        })?;
                                    },
                                }
                            }
                            match_body.puncts("=>");
                            match_body.group(Delimiter::Brace, |arm_body| {
                                if let Some(fields) = variant.fields.as_ref() {
                                    let mut first = true;
                                    match fields {
                                        | Fields::Struct(s) => {
                                            for (ident, _) in s.iter() {
                                                if !first {
                                                    arm_body.push_parsed(" && ")?;
                                                }
                                                arm_body.push_parsed(format!(
                                                    "{}.is_valid(buffer)",
                                                    ident
                                                ))?;
                                                first = false;
                                            }
                                        },
                                        | Fields::Tuple(t) => {
                                            for i in 0..t.len() {
                                                if !first {
                                                    arm_body.push_parsed(" && ")?;
                                                }
                                                arm_body.push_parsed(format!(
                                                    "__field_{}.is_valid(buffer)",
                                                    i
                                                ))?;
                                                first = false;
                                            }
                                        },
                                    }
                                    if first {
                                        arm_body.push_parsed("true")?;
                                    }
                                } else {
                                    arm_body.push_parsed("true")?;
                                }
                                Ok(())
                            })?;
                            match_body.punct(',');
                        }
                        Ok(())
                    })?;
                } else if let Some(fields) = self.fields.as_ref() {
                    let mut first = true;
                    match fields {
                        | Fields::Struct(s) => {
                            for (ident, _) in s.iter() {
                                if !first {
                                    fn_body.push_parsed(" && ")?;
                                }
                                fn_body.push_parsed(format!("self.{}.is_valid(buffer)", ident))?;
                                first = false;
                            }
                        },
                        | Fields::Tuple(t) => {
                            for i in 0..t.len() {
                                if !first {
                                    fn_body.push_parsed(" && ")?;
                                }
                                fn_body.push_parsed(format!("self.{}.is_valid(buffer)", i))?;
                                first = false;
                            }
                        },
                    }
                    if first {
                        fn_body.push_parsed("true")?;
                    }
                } else {
                    fn_body.push_parsed("true")?;
                }
                Ok(())
            })?;
        Ok(())
    }

    fn generate_deep_validator(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        let mut impl_for =
            generator.impl_for(format!("{}::relative_ptr::DeepValidator", crate_name));

        impl_for
            .generate_fn("is_valid_deep")
            .with_arg("&self", "")
            .with_arg("buffer", "&[u8]")
            .with_return_type("bool")
            .body(|fn_body| {
                if let Some(variants) = self.variants.as_ref() {
                    fn_body.push_parsed("match self")?;
                    fn_body.group(Delimiter::Brace, |match_body| {
                        for variant in variants {
                            match_body.push_parsed(format!("Self::{}", variant.name))?;
                            if let Some(fields) = variant.fields.as_ref() {
                                match fields {
                                    | Fields::Struct(s) => {
                                        match_body.group(Delimiter::Brace, |field_body| {
                                            for (ident, _) in s.iter() {
                                                field_body.push_parsed(format!("{},", ident))?;
                                            }
                                            Ok(())
                                        })?;
                                    },
                                    | Fields::Tuple(t) => {
                                        match_body.group(Delimiter::Parenthesis, |field_body| {
                                            for i in 0..t.len() {
                                                field_body
                                                    .push_parsed(format!("__field_{},", i))?;
                                            }
                                            Ok(())
                                        })?;
                                    },
                                }
                            }
                            match_body.puncts("=>");
                            match_body.group(Delimiter::Brace, |arm_body| {
                                if let Some(fields) = variant.fields.as_ref() {
                                    let mut first = true;
                                    match fields {
                                        | Fields::Struct(s) => {
                                            for (ident, _) in s.iter() {
                                                if !first {
                                                    arm_body.push_parsed(" && ")?;
                                                }
                                                arm_body.push_parsed(format!(
                                                    "{}.is_valid_deep(buffer)",
                                                    ident
                                                ))?;
                                                first = false;
                                            }
                                        },
                                        | Fields::Tuple(t) => {
                                            for i in 0..t.len() {
                                                if !first {
                                                    arm_body.push_parsed(" && ")?;
                                                }
                                                arm_body.push_parsed(format!(
                                                    "__field_{}.is_valid_deep(buffer)",
                                                    i
                                                ))?;
                                                first = false;
                                            }
                                        },
                                    }
                                    if first {
                                        arm_body.push_parsed("true")?;
                                    }
                                } else {
                                    arm_body.push_parsed("true")?;
                                }
                                Ok(())
                            })?;
                            match_body.punct(',');
                        }
                        Ok(())
                    })?;
                } else if let Some(fields) = self.fields.as_ref() {
                    let mut first = true;
                    match fields {
                        | Fields::Struct(s) => {
                            for (ident, _) in s.iter() {
                                if !first {
                                    fn_body.push_parsed(" && ")?;
                                }
                                fn_body
                                    .push_parsed(format!("self.{}.is_valid_deep(buffer)", ident))?;
                                first = false;
                            }
                        },
                        | Fields::Tuple(t) => {
                            for i in 0..t.len() {
                                if !first {
                                    fn_body.push_parsed(" && ")?;
                                }
                                fn_body.push_parsed(format!("self.{}.is_valid_deep(buffer)", i))?;
                                first = false;
                            }
                        },
                    }
                    if first {
                        fn_body.push_parsed("true")?;
                    }
                } else {
                    fn_body.push_parsed("true")?;
                }
                Ok(())
            })?;
        Ok(())
    }
}
