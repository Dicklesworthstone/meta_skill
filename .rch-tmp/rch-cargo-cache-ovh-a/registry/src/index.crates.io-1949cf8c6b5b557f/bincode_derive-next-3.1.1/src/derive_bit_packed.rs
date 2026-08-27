use crate::attribute::ContainerAttributes;
use crate::attribute::FieldAttributes;
use virtue::prelude::*;

pub(crate) struct DeriveBitPacked {
    pub fields: Option<Fields>,
    pub attributes: ContainerAttributes,
}

impl DeriveBitPacked {
    pub fn generate(
        self,
        generator: &mut Generator,
    ) -> Result<()> {
        self.generate_encode(generator)?;
        self.generate_decode(generator)?;
        self.generate_borrow_decode(generator)?;
        Ok(())
    }

    fn generate_encode(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        let bit_packing = self
            .attributes
            .bit_packing
            .as_ref()
            .map(|(s, _)| s.as_str());
        generator
            .impl_for(format!("{}::Encode", crate_name))
            .modify_generic_constraints(|generics, where_constraints| {
                if let Some((bounds, lit)) = (self.attributes.encode_bounds.as_ref()).or(self.attributes.bounds.as_ref()) {
                    where_constraints.clear();
                    where_constraints.push_parsed_constraint(bounds).map_err(|e| e.with_span(lit.span()))?;
                } else {
                    for g in generics.iter_generics() {
                        where_constraints.push_constraint(g, format!("{}::Encode", crate_name)).unwrap();
                    }
                }
                Ok(())
            })?
            .generate_fn("encode")
            .with_inline_always()
            .with_generic_deps("__E", [format!("{}::enc::Encoder", crate_name)])
            .with_self_arg(virtue::generate::FnSelfArg::RefSelf)
            .with_arg("encoder", "&mut __E")
            .with_return_type(format!("core::result::Result<(), {}::error::EncodeError>", crate_name))
            .body(|fn_body| {
                if let Some(fields) = self.fields.as_ref() {
                    let mut groups: Vec<(bool, Vec<_>)> = Vec::new();
                    for field in fields.names() {
                        let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                        let is_packed = attrs.bits.is_some();
                        if let Some(last) = groups.last_mut()
                            && last.0 == is_packed {
                                last.1.push(field);
                                continue;
                            }
                        groups.push((is_packed, vec![field]));
                    }

                    for (is_packed, group) in groups {
                        if is_packed {
                            fn_body.push_parsed(format!("if {}::config::Config::bit_packing_enabled(encoder.config())", crate_name))?;
                            fn_body.group(Delimiter::Brace, |b| {
                                b.push_parsed("let __config = *encoder.config();")?;
                                b.push_parsed(format!("let mut bit_writer = {}::enc::bit_writer::BitWriter::new({}::enc::Encoder::writer(encoder));", crate_name, crate_name))?;
                                for field in group.iter() {
                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                    let bits = attrs.bits.unwrap();
                                    b.push_parsed(format!("{{ fn check_width<const B: u8, T: {0}::utils::BitPackedCheck<B>>(_: &T) {{ let _ = <T as {0}::utils::BitPackedCheck<B>>::CHECK; }} check_width::<{1}, _>(&self.{2}); }}", crate_name, bits, field))?;
                                    b.push_parsed(format!("if (self.{} as u128) >= (1u128 << {})", field, bits))?;
                                    b.group(Delimiter::Brace, |b_err| {
                                        b_err.push_parsed(format!("return {}::error::cold_encode_error_other(\"Value exceeds bit-packed width\");", crate_name))?;
                                        Ok(())
                                    })?;
                                    if let Some(order) = bit_packing {
                                        b.push_parsed(format!("bit_writer.write_bits_{}((self.{}) as u64, {})?;", order, field, bits))?;
                                    } else {
                                        b.push_parsed(format!("bit_writer.write_bits((self.{}) as u64, {}, &__config)?;", field, bits))?;
                                    }
                                }
                                b.push_parsed("bit_writer.flush()?;")?;
                                Ok(())
                            })?;
                            fn_body.push_parsed("else")?;
                            fn_body.group(Delimiter::Brace, |b| {
                                for field in group {
                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                    if attrs.with_serde {
                                        b.push_parsed(format!("{0}::Encode::encode(&{0}::serde::Compat(&self.{1}), encoder)?;", crate_name, field))?;
                                    } else {
                                        b.push_parsed(format!("{0}::Encode::encode(&self.{1}, encoder)?;", crate_name, field))?;
                                    }
                                }
                                Ok(())
                            })?;
                        } else {
                            for field in group {
                                let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                if attrs.with_serde {
                                    fn_body.push_parsed(format!("{0}::Encode::encode(&{0}::serde::Compat(&self.{1}), encoder)?;", crate_name, field))?;
                                } else {
                                    fn_body.push_parsed(format!("{0}::Encode::encode(&self.{1}, encoder)?;", crate_name, field))?;
                                }
                            }
                        }
                    }
                }
                fn_body.push_parsed("core::result::Result::Ok(())")?;
                Ok(())
            })?;
        Ok(())
    }

    fn generate_decode(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        let bit_packing = self
            .attributes
            .bit_packing
            .as_ref()
            .map(|(s, _)| s.as_str());
        let decode_context = if let Some((decode_context, _)) = &self.attributes.decode_context {
            decode_context.as_str()
        } else {
            "__Context"
        };

        let mut impl_for = generator.impl_for(format!("{}::Decode", crate_name));
        if self.attributes.decode_context.is_none() {
            impl_for = impl_for.with_impl_generics(["__Context"]);
        }

        impl_for
            .with_trait_generics([decode_context])
            .modify_generic_constraints(|generics, where_constraints| {
                if let Some((bounds, lit)) = (self.attributes.decode_bounds.as_ref()).or(self.attributes.bounds.as_ref()) {
                     where_constraints.clear();
                     where_constraints.push_parsed_constraint(bounds).map_err(|e| e.with_span(lit.span()))?;
                } else {
                    for g in generics.iter_generics() {
                        where_constraints.push_constraint(g, format!("{}::Decode<{}>", crate_name, decode_context)).unwrap();
                    }
                }
                Ok(())
            })?
            .generate_fn("decode")
            .with_inline_always()
            .with_generic_deps("__D", [format!("{}::de::Decoder<Context = {}>", crate_name, decode_context)])
            .with_arg("decoder", "&mut __D")
            .with_return_type(format!("core::result::Result<Self, {}::error::DecodeError>", crate_name))
            .body(|fn_body| {
                if let Some(fields) = self.fields.as_ref() {
                    let mut groups: Vec<(bool, Vec<_>)> = Vec::new();
                    for field in fields.names() {
                        fn_body.push_parsed(format!("let mut __{};", field))?;
                        let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                        let is_packed = attrs.bits.is_some();
                        if let Some(last) = groups.last_mut()
                            && last.0 == is_packed {
                                last.1.push(field);
                                continue;
                            }
                        groups.push((is_packed, vec![field]));
                    }

                    for (is_packed, group) in groups {
                        if is_packed {
                            fn_body.push_parsed(format!("if {}::config::Config::bit_packing_enabled(decoder.config())", crate_name))?;
                            fn_body.group(Delimiter::Brace, |b| {
                                b.push_parsed("let __config = *decoder.config();")?;
                                b.push_parsed(format!("let mut bit_reader = {}::de::bit_reader::BitReader::new({}::de::Decoder::reader(decoder));", crate_name, crate_name))?;
                                for field in group.iter() {
                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                    let bits = attrs.bits.unwrap();
                                    if let Some(order) = bit_packing {
                                        b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits_{3}({2})?);", field, crate_name, bits, order))?;
                                    } else {
                                        b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits({2}, &__config)?);", field, crate_name, bits))?;
                                    }
                                    b.push_parsed(format!("{{ fn check_width<const B: u8, T: {0}::utils::BitPackedCheck<B>>(_: &T) {{ let _ = <T as {0}::utils::BitPackedCheck<B>>::CHECK; }} check_width::<{1}, _>(&__{2}); }}", crate_name, bits, field))?;
                                }
                                Ok(())
                            })?;
                            fn_body.push_parsed("else")?;
                            fn_body.group(Delimiter::Brace, |b| {
                                for field in group {
                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                    if attrs.with_serde {
                                        b.push_parsed(format!("__{1} = (<{0}::serde::Compat<_> as {0}::Decode::<{2}>>::decode(decoder)?).0;", crate_name, field, decode_context))?;
                                    } else {
                                        b.push_parsed(format!("__{1} = {0}::Decode::decode(decoder)?;", crate_name, field))?;
                                    }
                                }
                                Ok(())
                            })?;
                        } else {
                            for field in group {
                                let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                if attrs.with_serde {
                                    fn_body.push_parsed(format!("__{1} = (<{0}::serde::Compat<_> as {0}::Decode::<{2}>>::decode(decoder)?).0;", crate_name, field, decode_context))?;
                                } else {
                                    fn_body.push_parsed(format!("__{1} = {0}::Decode::decode(decoder)?;", crate_name, field))?;
                                }
                            }
                        }
                    }

                    fn_body.push_parsed("core::result::Result::Ok")?;
                    fn_body.group(Delimiter::Parenthesis, |ok_group| {
                        ok_group.ident_str("Self");
                        ok_group.group(Delimiter::Brace, |struct_body| {
                            for field in fields.names() {
                                struct_body.push_parsed(format!("{}: __{},", field, field))?;
                            }
                            Ok(())
                        })?;
                        Ok(())
                    })?;
                } else {
                    fn_body.push_parsed("core::result::Result::Ok(Self {})")?;
                }
                Ok(())
            })?;
        Ok(())
    }

    fn generate_borrow_decode(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        let bit_packing = self
            .attributes
            .bit_packing
            .as_ref()
            .map(|(s, _)| s.as_str());

        let decode_context = if let Some((decode_context, _)) = &self.attributes.decode_context {
            decode_context.as_str()
        } else {
            "__Context"
        };

        let mut impl_for = generator
            .impl_for_with_lifetimes(format!("{}::BorrowDecode", crate_name), ["__de"])
            .with_trait_generics([decode_context]);
        if self.attributes.decode_context.is_none() {
            impl_for = impl_for.with_impl_generics(["__Context"]);
        }

        impl_for
            .modify_generic_constraints(|generics, where_constraints| {
                if let Some((bounds, lit)) = (self.attributes.borrow_decode_bounds.as_ref()).or(self.attributes.bounds.as_ref()) {
                    where_constraints.clear();
                    where_constraints.push_parsed_constraint(bounds).map_err(|e| e.with_span(lit.span()))?;
                } else {
                    for g in generics.iter_generics() {
                        where_constraints.push_constraint(g, format!("{}::de::BorrowDecode<'__de, {}>", crate_name, decode_context)).unwrap();
                    }
                    for lt in generics.iter_lifetimes() {
                        where_constraints.push_parsed_constraint(format!("'__de: '{}", lt.ident))?;
                    }
                }
                Ok(())
            })?
            .generate_fn("borrow_decode")
            .with_inline_always()
            .with_generic_deps("__D", [format!("{}::de::BorrowDecoder<'__de, Context = {}>", crate_name, decode_context)])
            .with_arg("decoder", "&mut __D")
            .with_return_type(format!("core::result::Result<Self, {}::error::DecodeError>", crate_name))
            .body(|fn_body| {
                if let Some(fields) = self.fields.as_ref() {
                    let mut groups: Vec<(bool, Vec<_>)> = Vec::new();
                    for field in fields.names() {
                        fn_body.push_parsed(format!("let mut __{};", field))?;
                        let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                        let is_packed = attrs.bits.is_some();
                        if let Some(last) = groups.last_mut()
                            && last.0 == is_packed {
                                last.1.push(field);
                                continue;
                            }
                        groups.push((is_packed, vec![field]));
                    }

                    for (is_packed, group) in groups {
                        if is_packed {
                            fn_body.push_parsed(format!("if {}::config::Config::bit_packing_enabled(decoder.config())", crate_name))?;
                            fn_body.group(Delimiter::Brace, |b| {
                                b.push_parsed("let __config = *decoder.config();")?;
                                b.push_parsed(format!("let mut bit_reader = {}::de::bit_reader::BitReader::new({}::de::Decoder::reader(decoder));", crate_name, crate_name))?;
                                for field in group.iter() {
                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                    let bits = attrs.bits.unwrap();
                                                        if let Some(order) = bit_packing {
                                                            b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits_{3}({2})?);", field, crate_name, bits, order))?;
                                                        } else {
                                                            b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits({2}, &__config)?);", field, crate_name, bits))?;
                                                        }
                                    b.push_parsed(format!("{{ fn check_width<const B: u8, T: {0}::utils::BitPackedCheck<B>>(_: &T) {{ let _ = <T as {0}::utils::BitPackedCheck<B>>::CHECK; }} check_width::<{1}, _>(&__{2}); }}", crate_name, bits, field))?;
                                }
                                Ok(())
                            })?;
                            fn_body.push_parsed("else")?;
                            fn_body.group(Delimiter::Brace, |b| {
                                for field in group {
                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                    if attrs.with_serde {
                                        b.push_parsed(format!("__{1} = (<{0}::serde::BorrowCompat<_> as {0}::BorrowDecode::<'__de, {2}>>::borrow_decode(decoder)?).0;", crate_name, field, decode_context))?;
                                    } else {
                                        b.push_parsed(format!("__{1} = {0}::BorrowDecode::borrow_decode(decoder)?;", crate_name, field))?;
                                    }
                                }
                                Ok(())
                            })?;
                        } else {
                            for field in group {
                                let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                if attrs.with_serde {
                                    fn_body.push_parsed(format!("__{1} = (<{0}::serde::BorrowCompat<_> as {0}::BorrowDecode::<'__de, {2}>>::borrow_decode(decoder)?).0;", crate_name, field, decode_context))?;
                                } else {
                                    fn_body.push_parsed(format!("__{1} = {0}::BorrowDecode::borrow_decode(decoder)?;", crate_name, field))?;
                                }
                            }
                        }
                    }

                    fn_body.push_parsed("core::result::Result::Ok")?;
                    fn_body.group(Delimiter::Parenthesis, |ok_group| {
                        ok_group.ident_str("Self");
                        ok_group.group(Delimiter::Brace, |struct_body| {
                            for field in fields.names() {
                                struct_body.push_parsed(format!("{}: __{},", field, field))?;
                            }
                            Ok(())
                        })?;
                        Ok(())
                    })?;
                } else {
                    fn_body.push_parsed("core::result::Result::Ok(Self {})")?;
                }
                Ok(())
            })?;
        Ok(())
    }
}

pub(crate) struct DeriveBitPackedEnum {
    pub variants: Vec<EnumVariant>,
    pub attributes: ContainerAttributes,
}

enum EncItem {
    VariantIndex(usize, u32),
    Field {
        name: String,
        bits: Option<u8>,
        with_serde: bool,
    },
}

impl DeriveBitPackedEnum {
    pub fn generate(
        self,
        generator: &mut Generator,
    ) -> Result<()> {
        self.generate_encode(generator)?;
        self.generate_decode(generator)?;
        self.generate_borrow_decode(generator)?;
        Ok(())
    }

    fn generate_encode(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = self.attributes.crate_name.as_str();
        let bit_packing = self
            .attributes
            .bit_packing
            .as_ref()
            .map(|(s, _)| s.as_str());
        let variant_bits = if self.variants.len() <= 1 {
            0
        } else {
            (self.variants.len() as u32 - 1).ilog2() + 1
        };

        generator
            .impl_for(format!("{}::Encode", crate_name))
            .modify_generic_constraints(|generics, where_constraints| {
                if let Some((bounds, lit)) = (self.attributes.encode_bounds.as_ref()).or(self.attributes.bounds.as_ref()) {
                    where_constraints.clear();
                    where_constraints.push_parsed_constraint(bounds).map_err(|e| e.with_span(lit.span()))?;
                } else {
                    for g in generics.iter_generics() {
                        where_constraints.push_constraint(g, format!("{}::Encode", crate_name)).unwrap();
                    }
                }
                Ok(())
            })?
            .generate_fn("encode")
            .with_inline_always()
            .with_generic_deps("__E", [format!("{}::enc::Encoder", crate_name)])
            .with_self_arg(FnSelfArg::RefSelf)
            .with_arg("encoder", "&mut __E")
            .with_return_type(format!("core::result::Result<(), {}::error::EncodeError>", crate_name))
            .body(|fn_body| {
                fn_body.ident_str("match");
                fn_body.ident_str("self");
                fn_body.group(Delimiter::Brace, |match_body| {
                    if self.variants.is_empty() {
                        match_body.push_parsed("_ => core::unreachable!(),")?;
                    }
                    for (variant_index, variant) in self.variants.iter().enumerate() {
                        match_body.ident_str("Self");
                        match_body.puncts("::");
                        match_body.ident(variant.name.clone());

                        if let Some(fields) = variant.fields.as_ref() {
                            let delimiter = fields.delimiter();
                            match_body.group(delimiter, |field_body| {
                                for (idx, field_name) in fields.names().into_iter().enumerate() {
                                    if idx != 0 {
                                        field_body.punct(',');
                                    }
                                    field_body.push(field_name.to_token_tree_with_prefix("field_"));
                                }
                                Ok(())
                            })?;
                        }
                        match_body.puncts("=>");

                        match_body.group(Delimiter::Brace, |body| {
                            body.push_parsed(format!("if {}::config::Config::bit_packing_enabled(encoder.config())", crate_name))?;
                            body.group(Delimiter::Brace, |b_packed| {
                                b_packed.push_parsed("let mut __bit_state: (u8, u8) = (0, 0);")?;
                                let mut groups: Vec<(bool, Vec<EncItem>)> = Vec::new();
                                if variant_bits > 0 {
                                    groups.push((true, vec![EncItem::VariantIndex(variant_index, variant_bits)]));
                                }

                                if let Some(fields) = variant.fields.as_ref() {
                                    for field in fields.names() {
                                        let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                        let is_packed = attrs.bits.is_some();
                                        if let Some(last) = groups.last_mut()
                                            && last.0 == is_packed {
                                                last.1.push(EncItem::Field { name: field.to_string_with_prefix("field_"), bits: attrs.bits, with_serde: attrs.with_serde });
                                                continue;
                                            }
                                        groups.push((is_packed, vec![EncItem::Field { name: field.to_string_with_prefix("field_"), bits: attrs.bits, with_serde: attrs.with_serde }]));
                                    }
                                }

                                for (is_packed, group) in groups {
                                    if is_packed {
                                        b_packed.group(Delimiter::Brace, |b| {
                                            b.push_parsed("let __config = *encoder.config();")?;
                                            b.push_parsed(format!("let mut bit_writer = {}::enc::bit_writer::BitWriter::from_state({}::enc::Encoder::writer(encoder), __bit_state.0, __bit_state.1);", crate_name, crate_name))?;
                                            for item in group {
                                                match item {
                                                    EncItem::VariantIndex(idx, bits) => {
                                                        if let Some(order) = bit_packing {
                                                            b.push_parsed(format!("bit_writer.write_bits_{}({} as u64, {})?;", order, idx, bits))?;
                                                        } else {
                                                            b.push_parsed(format!("bit_writer.write_bits({} as u64, {}, &__config)?;", idx, bits))?;
                                                        }
                                                    }
                                                    EncItem::Field { name, bits, with_serde: _ } => {
                                                        let bits_val = bits.unwrap();
                                                        b.push_parsed(format!("{{ fn check_width<const B: u8, T: {0}::utils::BitPackedCheck<B>>(_: &T) {{ let _ = <T as {0}::utils::BitPackedCheck<B>>::CHECK; }} check_width::<{1}, _>({2}); }}", crate_name, bits_val, name))?;
                                                        b.push_parsed(format!("if ((*({})) as u128) >= (1u128 << {})", name, bits_val))?;
                                                        b.group(Delimiter::Brace, |b_err| {
                                                            b_err.push_parsed(format!("return {}::error::cold_encode_error_other(\"Value exceeds bit-packed width\");", crate_name))?;
                                                            Ok(())
                                                        })?;
                                                        if let Some(order) = bit_packing {
                                                            b.push_parsed(format!("bit_writer.write_bits_{}((*({})) as u64, {})?;", order, name, bits_val))?;
                                                        } else {
                                                            b.push_parsed(format!("bit_writer.write_bits((*({})) as u64, {}, &__config)?;", name, bits_val))?;
                                                        }
                                                    }
                                                }
                                            }
                                            b.push_parsed("__bit_state = bit_writer.get_state();")?;
                                            Ok(())
                                        })?;
                                    } else {
                                        b_packed.push_parsed("if __bit_state.1 > 0")?;
                                        b_packed.group(Delimiter::Brace, |b| {
                                            b.push_parsed(format!("let mut bit_writer = {}::enc::bit_writer::BitWriter::from_state({}::enc::Encoder::writer(encoder), __bit_state.0, __bit_state.1);", crate_name, crate_name))?;
                                            b.push_parsed("bit_writer.flush()?;")?;
                                            Ok(())
                                        })?;
                                        b_packed.push_parsed("__bit_state = (0, 0);")?;
                                        for item in group {
                                            if let EncItem::Field { name, bits: _, with_serde } = item {
                                                if with_serde {
                                                    b_packed.push_parsed(format!("{0}::Encode::encode(&{0}::serde::Compat({1}), encoder)?;", crate_name, name))?;
                                                } else {
                                                    b_packed.push_parsed(format!("{0}::Encode::encode({1}, encoder)?;", crate_name, name))?;
                                                }
                                            }
                                        }
                                    }
                                }
                                b_packed.push_parsed("if __bit_state.1 > 0")?;
                                b_packed.group(Delimiter::Brace, |b| {
                                    b.push_parsed(format!("let mut bit_writer = {}::enc::bit_writer::BitWriter::from_state({}::enc::Encoder::writer(encoder), __bit_state.0, __bit_state.1);", crate_name, crate_name))?;
                                    b.push_parsed("bit_writer.flush()?;")?;
                                    Ok(())
                                })?;
                                b_packed.push_parsed("core::result::Result::Ok(())")?;
                                Ok(())
                            })?;
                            body.push_parsed("else")?;
                            body.group(Delimiter::Brace, |b_unpacked| {
                                b_unpacked.push_parsed(format!("<u32 as {}::Encode>::encode(&( {} as u32), encoder)?;", crate_name, variant_index))?;
                                if let Some(fields) = variant.fields.as_ref() {
                                    for field in fields.names() {
                                        let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                        let name = field.to_string_with_prefix("field_");
                                        if attrs.with_serde {
                                            b_unpacked.push_parsed(format!("{0}::Encode::encode(&{0}::serde::Compat({1}), encoder)?;", crate_name, name))?;
                                        } else {
                                            b_unpacked.push_parsed(format!("{0}::Encode::encode({1}, encoder)?;", crate_name, name))?;
                                        }
                                    }
                                }
                                b_unpacked.push_parsed("core::result::Result::Ok(())")?;
                                Ok(())
                            })?;
                            Ok(())
                        })?;
                        match_body.punct(',');
                    }
                    Ok(())
                })?;
                Ok(())
            })?;
        Ok(())
    }

    fn invalid_variant_case(
        &self,
        enum_name: &str,
        result: &mut StreamBuilder,
    ) -> Result<()> {
        let crate_name = self.attributes.crate_name.as_str();
        result.ident_str("variant");
        result.puncts("=>");
        result.push_parsed(format!(
            "{}::error::cold_decode_error_unexpected_variant",
            crate_name
        ))?;
        result.group(Delimiter::Parenthesis, |args| {
            args.lit_str(enum_name);
            args.punct(',');

            if self.variants.iter().any(|i| i.value.is_some()) {
                args.push_parsed(format!(
                    "&{}::error::AllowedEnumVariants::Allowed",
                    crate_name
                ))?;
                args.group(Delimiter::Parenthesis, |allowed_inner| {
                    allowed_inner.punct('&');
                    allowed_inner.group(Delimiter::Bracket, |allowed_slice| {
                        for (idx, variant) in self.variants.iter().enumerate() {
                            if idx != 0 {
                                allowed_slice.punct(',');
                            }
                            allowed_slice.ident(variant.name.clone());
                        }
                        Ok(())
                    })?;
                    Ok(())
                })?;
            } else {
                args.push_parsed(format!(
                    "&{0}::error::AllowedEnumVariants::Range {{ min: 0, max: {1} }}",
                    crate_name,
                    self.variants.len() - 1
                ))?;
            }
            args.punct(',');
            args.ident_str("variant");
            Ok(())
        })?;
        Ok(())
    }

    fn generate_decode(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = self.attributes.crate_name.as_str();
        let bit_packing = self
            .attributes
            .bit_packing
            .as_ref()
            .map(|(s, _)| s.as_str());
        let decode_context = if let Some((decode_context, _)) = &self.attributes.decode_context {
            decode_context.as_str()
        } else {
            "__Context"
        };
        let variant_bits = if self.variants.len() <= 1 {
            0
        } else {
            (self.variants.len() as u32 - 1).ilog2() + 1
        };
        let enum_name = generator.target_name().to_string();

        let mut impl_for = generator.impl_for(format!("{}::Decode", crate_name));
        if self.attributes.decode_context.is_none() {
            impl_for = impl_for.with_impl_generics(["__Context"]);
        }

        impl_for
            .with_trait_generics([decode_context])
            .modify_generic_constraints(|generics, where_constraints| {
                if let Some((bounds, lit)) = (self.attributes.decode_bounds.as_ref()).or(self.attributes.bounds.as_ref()) {
                    where_constraints.clear();
                    where_constraints.push_parsed_constraint(bounds).map_err(|e| e.with_span(lit.span()))?;
                } else {
                    for g in generics.iter_generics() {
                        where_constraints.push_constraint(g, format!("{}::Decode<{}>", crate_name, decode_context)).unwrap();
                    }
                }
                Ok(())
            })?
            .generate_fn("decode")
            .with_inline_always()
            .with_generic_deps("__D", [format!("{}::de::Decoder<Context = {}>", crate_name, decode_context)])
            .with_arg("decoder", "&mut __D")
            .with_return_type(format!("core::result::Result<Self, {}::error::DecodeError>", crate_name))
            .body(|fn_builder| {
                fn_builder.push_parsed(format!("if {}::config::Config::bit_packing_enabled(decoder.config())", crate_name))?;
                fn_builder.group(Delimiter::Brace, |b_packed| {
                    if self.variants.is_empty() {
                        b_packed.push_parsed(format!("{}::error::cold_decode_error_empty_enum(core::any::type_name::<Self>())", crate_name))?;
                    } else {
                        b_packed.push_parsed("let mut __bit_state: (u8, u8) = (0, 0);")?;
                        if variant_bits > 0 {
                            b_packed.push_parsed("let variant_index = ")?;
                            b_packed.group(Delimiter::Brace, |b| {
                                b.push_parsed("let __config = *decoder.config();")?;
                                b.push_parsed(format!("let mut bit_reader = {}::de::bit_reader::BitReader::from_state({}::de::Decoder::reader(decoder), __bit_state.0, __bit_state.1);", crate_name, crate_name))?;
                                if let Some(order) = bit_packing {
                                    b.push_parsed(format!("let variant_index = <u32 as {0}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits_{2}({1})?);", crate_name, variant_bits, order))?;
                                } else {
                                    b.push_parsed(format!("let variant_index = <u32 as {0}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits({1}, &__config)?);", crate_name, variant_bits))?;
                                }
                                b.push_parsed("__bit_state = bit_reader.get_state();")?;
                                b.push_parsed(format!("core::result::Result::Ok::<_, {}::error::DecodeError>(variant_index)", crate_name))?;
                                Ok(())
                            })?;
                            b_packed.push_parsed("?;")?;
                        } else {
                            b_packed.push_parsed("let variant_index = 0u32;")?;
                        }

                        b_packed.push_parsed("match variant_index")?;
                        b_packed.group(Delimiter::Brace, |variant_case| {
                            for (idx, variant) in self.variants.iter().enumerate() {
                                variant_case.push_parsed(format!("{} =>", idx))?;
                                variant_case.group(Delimiter::Brace, |arm_body| {
                                    if let Some(fields) = variant.fields.as_ref() {
                                        let mut groups: Vec<(bool, Vec<_>)> = Vec::new();
                                        for field in fields.names() {
                                            arm_body.push_parsed(format!("let mut __{};", field.to_string_with_prefix("field_")))?;
                                            let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                            let is_packed = attrs.bits.is_some();
                                            if let Some(last) = groups.last_mut()
                                                && last.0 == is_packed {
                                                    last.1.push(field);
                                                    continue;
                                                }
                                            groups.push((is_packed, vec![field]));
                                        }

                                        for (is_packed, group) in groups {
                                            if is_packed {
                                                arm_body.group(Delimiter::Brace, |b| {
                                                    b.push_parsed("let __config = *decoder.config();")?;
                                                    b.push_parsed(format!("let mut bit_reader = {}::de::bit_reader::BitReader::from_state({}::de::Decoder::reader(decoder), __bit_state.0, __bit_state.1);", crate_name, crate_name))?;
                                                    for field in group {
                                                        let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                                        let bits = attrs.bits.unwrap();
                                                        let name = field.to_string_with_prefix("field_");
                                                        if let Some(order) = bit_packing {
                                                            b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits_{3}({2})?);", name, crate_name, bits, order))?;
                                                        } else {
                                                            b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits({2}, &__config)?);", name, crate_name, bits))?;
                                                        }
                                                        b.push_parsed(format!("{{ fn check_width<const B: u8, T: {0}::utils::BitPackedCheck<B>>(_: &T) {{ let _ = <T as {0}::utils::BitPackedCheck<B>>::CHECK; }} check_width::<{1}, _>(&__{2}); }}", crate_name, bits, name))?;
                                                    }
                                                    b.push_parsed("__bit_state = bit_reader.get_state();")?;
                                                    Ok(())
                                                })?;
                                            } else {
                                                arm_body.push_parsed("__bit_state = (0, 0);")?;
                                                for field in group {
                                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                                    if attrs.with_serde {
                                                        arm_body.push_parsed(format!("__{1} = (<{0}::serde::Compat<_> as {0}::Decode::<{2}>>::decode(decoder)?).0;", crate_name, field.to_string_with_prefix("field_"), decode_context))?;
                                                    } else {
                                                        arm_body.push_parsed(format!("__{1} = {0}::Decode::decode(decoder)?;", crate_name, field.to_string_with_prefix("field_")))?;
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    arm_body.push_parsed("core::result::Result::Ok")?;
                                    arm_body.group(Delimiter::Parenthesis, |variant_case_body| {
                                        variant_case_body.ident_str("Self");
                                        variant_case_body.puncts("::");
                                        variant_case_body.ident(variant.name.clone());

                                        if let Some(fields) = variant.fields.as_ref() {
                                            let is_tuple = matches!(fields, Fields::Tuple(_));
                                            variant_case_body.group(
                                                if is_tuple { Delimiter::Parenthesis } else { Delimiter::Brace },
                                                |variant_body| {
                                                    for field in fields.names() {
                                                        if !is_tuple {
                                                            variant_body.ident(field.unwrap_ident().clone());
                                                            variant_body.punct(':');
                                                        }
                                                        variant_body.push_parsed(format!("__{},", field.to_string_with_prefix("field_")))?;
                                                    }
                                                    Ok(())
                                                }
                                            )?;
                                        }
                                        Ok(())
                                    })?;
                                    Ok(())
                                })?;
                                variant_case.punct(',');
                            }

                            self.invalid_variant_case(&enum_name, variant_case)?;
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
                fn_builder.push_parsed("else")?;
                fn_builder.group(Delimiter::Brace, |b_unpacked| {
                    if self.variants.is_empty() {
                         b_unpacked.push_parsed(format!("{}::error::cold_decode_error_empty_enum(core::any::type_name::<Self>())", crate_name))?;
                    } else {
                        b_unpacked.push_parsed(format!("let variant_index: u32 = {}::Decode::decode(decoder)?;", crate_name))?;
                        b_unpacked.push_parsed("match variant_index")?;
                        b_unpacked.group(Delimiter::Brace, |variant_case| {
                            for (idx, variant) in self.variants.iter().enumerate() {
                                variant_case.push_parsed(format!("{} =>", idx))?;
                                variant_case.group(Delimiter::Brace, |arm_body| {
                                    if let Some(fields) = variant.fields.as_ref() {
                                        for field in fields.names() {
                                            arm_body.push_parsed(format!("let mut __{};", field.to_string_with_prefix("field_")))?;
                                            let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                            if attrs.with_serde {
                                                arm_body.push_parsed(format!("__{1} = (<{0}::serde::Compat<_> as {0}::Decode::<{2}>>::decode(decoder)?).0;", crate_name, field.to_string_with_prefix("field_"), decode_context))?;
                                            } else {
                                                arm_body.push_parsed(format!("__{1} = {0}::Decode::decode(decoder)?;", crate_name, field.to_string_with_prefix("field_")))?;
                                            }
                                        }
                                    }

                                    arm_body.push_parsed("core::result::Result::Ok")?;
                                    arm_body.group(Delimiter::Parenthesis, |variant_case_body| {
                                        variant_case_body.ident_str("Self");
                                        variant_case_body.puncts("::");
                                        variant_case_body.ident(variant.name.clone());

                                        if let Some(fields) = variant.fields.as_ref() {
                                            let is_tuple = matches!(fields, Fields::Tuple(_));
                                            variant_case_body.group(
                                                if is_tuple { Delimiter::Parenthesis } else { Delimiter::Brace },
                                                |variant_body| {
                                                    for field in fields.names() {
                                                        if !is_tuple {
                                                            variant_body.ident(field.unwrap_ident().clone());
                                                            variant_body.punct(':');
                                                        }
                                                        variant_body.push_parsed(format!("__{},", field.to_string_with_prefix("field_")))?;
                                                    }
                                                    Ok(())
                                                }
                                            )?;
                                        }
                                        Ok(())
                                    })?;
                                    Ok(())
                                })?;
                            }
                            self.invalid_variant_case(&enum_name, variant_case)?;
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
                Ok(())
            })?;
        Ok(())
    }

    fn generate_borrow_decode(
        &self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = self.attributes.crate_name.as_str();
        let bit_packing = self
            .attributes
            .bit_packing
            .as_ref()
            .map(|(s, _)| s.as_str());
        let decode_context = if let Some((decode_context, _)) = &self.attributes.decode_context {
            decode_context.as_str()
        } else {
            "__Context"
        };
        let variant_bits = if self.variants.len() <= 1 {
            0
        } else {
            (self.variants.len() as u32 - 1).ilog2() + 1
        };
        let enum_name = generator.target_name().to_string();

        let mut impl_for = generator
            .impl_for_with_lifetimes(format!("{}::BorrowDecode", crate_name), ["__de"])
            .with_trait_generics([decode_context]);
        if self.attributes.decode_context.is_none() {
            impl_for = impl_for.with_impl_generics(["__Context"]);
        }

        impl_for
            .modify_generic_constraints(|generics, where_constraints| {
                if let Some((bounds, lit)) = (self.attributes.borrow_decode_bounds.as_ref()).or(self.attributes.bounds.as_ref()) {
                    where_constraints.clear();
                    where_constraints.push_parsed_constraint(bounds).map_err(|e| e.with_span(lit.span()))?;
                } else {
                    for g in generics.iter_generics() {
                        where_constraints.push_constraint(g, format!("{}::de::BorrowDecode<'__de, {}>", crate_name, decode_context)).unwrap();
                    }
                    for lt in generics.iter_lifetimes() {
                        where_constraints.push_parsed_constraint(format!("'__de: '{}", lt.ident))?;
                    }
                }
                Ok(())
            })?
            .generate_fn("borrow_decode")
            .with_inline_always()
            .with_generic_deps("__D", [format!("{}::de::BorrowDecoder<'__de, Context = {}>", crate_name, decode_context)])
            .with_arg("decoder", "&mut __D")
            .with_return_type(format!("core::result::Result<Self, {}::error::DecodeError>", crate_name))
            .body(|fn_builder| {
                fn_builder.push_parsed(format!("if {}::config::Config::bit_packing_enabled(decoder.config())", crate_name))?;
                fn_builder.group(Delimiter::Brace, |b_packed| {
                    if self.variants.is_empty() {
                        b_packed.push_parsed(format!("{}::error::cold_decode_error_empty_enum(core::any::type_name::<Self>())", crate_name))?;
                    } else {
                        b_packed.push_parsed("let mut __bit_state: (u8, u8) = (0, 0);")?;
                        if variant_bits > 0 {
                            b_packed.push_parsed("let variant_index = ")?;
                            b_packed.group(Delimiter::Brace, |b| {
                                b.push_parsed("let __config = *decoder.config();")?;
                                b.push_parsed(format!("let mut bit_reader = {}::de::bit_reader::BitReader::from_state({}::de::Decoder::reader(decoder), __bit_state.0, __bit_state.1);", crate_name, crate_name))?;
                                if let Some(order) = bit_packing {
                                    b.push_parsed(format!("let variant_index = <u32 as {0}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits_{2}({1})?);", crate_name, variant_bits, order))?;
                                } else {
                                    b.push_parsed(format!("let variant_index = <u32 as {0}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits({1}, &__config)?);", crate_name, variant_bits))?;
                                }
                                b.push_parsed("__bit_state = bit_reader.get_state();")?;
                                b.push_parsed(format!("core::result::Result::Ok::<_, {}::error::DecodeError>(variant_index)", crate_name))?;
                                Ok(())
                            })?;
                            b_packed.push_parsed("?;")?;
                        } else {
                            b_packed.push_parsed("let variant_index = 0u32;")?;
                        }

                        b_packed.push_parsed("match variant_index")?;
                        b_packed.group(Delimiter::Brace, |variant_case| {
                            for (idx, variant) in self.variants.iter().enumerate() {
                                variant_case.push_parsed(format!("{} =>", idx))?;
                                variant_case.group(Delimiter::Brace, |arm_body| {
                                    if let Some(fields) = variant.fields.as_ref() {
                                        let mut groups: Vec<(bool, Vec<_>)> = Vec::new();
                                        for field in fields.names() {
                                            arm_body.push_parsed(format!("let mut __{};", field.to_string_with_prefix("field_")))?;
                                            let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                            let is_packed = attrs.bits.is_some();
                                            if let Some(last) = groups.last_mut()
                                                && last.0 == is_packed {
                                                    last.1.push(field);
                                                    continue;
                                                }
                                            groups.push((is_packed, vec![field]));
                                        }

                                        for (is_packed, group) in groups {
                                            if is_packed {
                                                arm_body.group(Delimiter::Brace, |b| {
                                                    b.push_parsed("let __config = *decoder.config();")?;
                                                    b.push_parsed(format!("let mut bit_reader = {}::de::bit_reader::BitReader::from_state({}::de::Decoder::reader(decoder), __bit_state.0, __bit_state.1);", crate_name, crate_name))?;
                                                    for field in group {
                                                        let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                                        let bits = attrs.bits.unwrap();
                                                        let name = field.to_string_with_prefix("field_");
                                                        if let Some(order) = bit_packing {
                                                            b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits_{3}({2})?);", name, crate_name, bits, order))?;
                                                        } else {
                                                            b.push_parsed(format!("__{0} = <_ as {1}::de::bit_reader::Unpackable>::unpack(bit_reader.read_bits({2}, &__config)?);", name, crate_name, bits))?;
                                                        }
                                                        b.push_parsed(format!("{{ fn check_width<const B: u8, T: {0}::utils::BitPackedCheck<B>>(_: &T) {{ let _ = <T as {0}::utils::BitPackedCheck<B>>::CHECK; }} check_width::<{1}, _>(&__{2}); }}", crate_name, bits, name))?;
                                                    }
                                                    b.push_parsed("__bit_state = bit_reader.get_state();")?;
                                                    Ok(())
                                                })?;
                                            } else {
                                                arm_body.push_parsed("__bit_state = (0, 0);")?;
                                                for field in group {
                                                    let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                                    if attrs.with_serde {
                                                        arm_body.push_parsed(format!("__{1} = (<{0}::serde::BorrowCompat<_> as {0}::BorrowDecode::<'__de, {2}>>::borrow_decode(decoder)?).0;", crate_name, field.to_string_with_prefix("field_"), decode_context))?;
                                                    } else {
                                                        arm_body.push_parsed(format!("__{1} = {0}::BorrowDecode::borrow_decode(decoder)?;", crate_name, field.to_string_with_prefix("field_")))?;
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    arm_body.push_parsed("core::result::Result::Ok")?;
                                    arm_body.group(Delimiter::Parenthesis, |variant_case_body| {
                                        variant_case_body.ident_str("Self");
                                        variant_case_body.puncts("::");
                                        variant_case_body.ident(variant.name.clone());

                                        if let Some(fields) = variant.fields.as_ref() {
                                            let is_tuple = matches!(fields, Fields::Tuple(_));
                                            variant_case_body.group(
                                                if is_tuple { Delimiter::Parenthesis } else { Delimiter::Brace },
                                                |variant_body| {
                                                    for field in fields.names() {
                                                        if !is_tuple {
                                                            variant_body.ident(field.unwrap_ident().clone());
                                                            variant_body.punct(':');
                                                        }
                                                        variant_body.push_parsed(format!("__{},", field.to_string_with_prefix("field_")))?;
                                                    }
                                                    Ok(())
                                                }
                                            )?;
                                        }
                                        Ok(())
                                    })?;
                                    Ok(())
                                })?;
                                variant_case.punct(',');
                            }

                            self.invalid_variant_case(&enum_name, variant_case)?;
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
                fn_builder.push_parsed("else")?;
                fn_builder.group(Delimiter::Brace, |b_unpacked| {
                    if self.variants.is_empty() {
                         b_unpacked.push_parsed(format!("{}::error::cold_decode_error_empty_enum(core::any::type_name::<Self>())", crate_name))?;
                    } else {
                        b_unpacked.push_parsed(format!("let variant_index: u32 = {}::Decode::decode(decoder)?;", crate_name))?;
                        b_unpacked.push_parsed("match variant_index")?;
                        b_unpacked.group(Delimiter::Brace, |variant_case| {
                            for (idx, variant) in self.variants.iter().enumerate() {
                                variant_case.push_parsed(format!("{} =>", idx))?;
                                variant_case.group(Delimiter::Brace, |arm_body| {
                                    if let Some(fields) = variant.fields.as_ref() {
                                        for field in fields.names() {
                                            arm_body.push_parsed(format!("let mut __{};", field.to_string_with_prefix("field_")))?;
                                            let attrs = field.attributes().get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                            if attrs.with_serde {
                                                arm_body.push_parsed(format!("__{1} = (<{0}::serde::BorrowCompat<_> as {0}::BorrowDecode::<'__de, {2}>>::borrow_decode(decoder)?).0;", crate_name, field.to_string_with_prefix("field_"), decode_context))?;
                                            } else {
                                                arm_body.push_parsed(format!("__{1} = {0}::BorrowDecode::borrow_decode(decoder)?;", crate_name, field.to_string_with_prefix("field_")))?;
                                            }
                                        }
                                    }

                                    arm_body.push_parsed("core::result::Result::Ok")?;
                                    arm_body.group(Delimiter::Parenthesis, |variant_case_body| {
                                        variant_case_body.ident_str("Self");
                                        variant_case_body.puncts("::");
                                        variant_case_body.ident(variant.name.clone());

                                        if let Some(fields) = variant.fields.as_ref() {
                                            let is_tuple = matches!(fields, Fields::Tuple(_));
                                            variant_case_body.group(
                                                if is_tuple { Delimiter::Parenthesis } else { Delimiter::Brace },
                                                |variant_body| {
                                                    for field in fields.names() {
                                                        if !is_tuple {
                                                            variant_body.ident(field.unwrap_ident().clone());
                                                            variant_body.punct(':');
                                                        }
                                                        variant_body.push_parsed(format!("__{},", field.to_string_with_prefix("field_")))?;
                                                    }
                                                    Ok(())
                                                }
                                            )?;
                                        }
                                        Ok(())
                                    })?;
                                    Ok(())
                                })?;
                            }
                            self.invalid_variant_case(&enum_name, variant_case)?;
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
                Ok(())
            })?;
        Ok(())
    }
}
