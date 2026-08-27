use crate::attribute::ContainerAttributes;
use crate::attribute::FieldAttributes;
use virtue::prelude::*;

pub(crate) struct DeriveFingerprint {
    pub fields: Option<Fields>,
    pub variants: Option<Vec<EnumVariant>>,
    pub attributes: ContainerAttributes,
}

impl DeriveFingerprint {
    pub fn generate(
        self,
        generator: &mut Generator,
        type_name: &str,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;
        generator
            .impl_for(format!("{}::Fingerprint<__C>", crate_name))
            .with_impl_generics([format!("__C: {}::config::Config", crate_name)])
            .modify_generic_constraints(|generics, where_constraints| {
                if let Some((bounds, lit)) = self.attributes.bounds.as_ref() {
                    where_constraints
                        .push_parsed_constraint(bounds)
                        .map_err(|e| e.with_span(lit.span()))?;
                } else {
                    for g in generics.iter_generics() {
                        where_constraints
                            .push_constraint(g, format!("{}::Fingerprint<__C>", crate_name))?;
                    }
                }
                Ok(())
            })?
            .generate_const("SCHEMA_HASH", "u64")
            .with_value(|const_body| {
                const_body.group(Delimiter::Brace, |block| {
                    block.push_parsed(format!("let mut hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"{}\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(<__C as {}::config::InternalConfigFingerprint>::CONFIG_HASH));", crate_name, type_name, crate_name, crate_name))?;

                    if let Some(fields) = self.fields.as_ref() {
                        block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"struct\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, crate_name))?;
                        match fields {
                            Fields::Struct(s) => {
                                for (ident, field) in s {
                                    block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"{}\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, ident, crate_name))?;
                                    let attributes = field.attributes.get_attribute::<FieldAttributes>()?.unwrap_or_default();
                                    if let Some(bits) = attributes.bits {
                                        block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(&[{}u8], &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, bits, crate_name))?;
                                    }
                                    block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(&<{} as {}::Fingerprint<__C>>::SCHEMA_HASH.to_le_bytes(), &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, field.type_string(), crate_name, crate_name))?;
                                }
                            }
                            Fields::Tuple(t) => {
                                for (i, field) in t.iter().enumerate() {
                                    block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"{}\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, i, crate_name))?;
                                    block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(&<{} as {}::Fingerprint<__C>>::SCHEMA_HASH.to_le_bytes(), &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, field.type_string(), crate_name, crate_name))?;
                                }
                            }
                        }
                    } else if let Some(variants) = self.variants.as_ref() {
                        block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"enum\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, crate_name))?;
                        for variant in variants {
                            block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"{}\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, variant.name, crate_name))?;
                            if let Some(fields) = variant.fields.as_ref() {
                                 match fields {
                                    Fields::Struct(s) => {
                                        for (ident, field) in s {
                                            block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"{}\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, ident, crate_name))?;
                                            block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(&<{} as {}::Fingerprint<__C>>::SCHEMA_HASH.to_le_bytes(), &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, field.type_string(), crate_name, crate_name))?;
                                        }
                                    }
                                    Fields::Tuple(t) => {
                                        for (i, field) in t.iter().enumerate() {
                                            block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(b\"{}\", &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, i, crate_name))?;
                                            block.push_parsed(format!("hash = {}::rapidhash::v3::rapidhash_v3_seeded(&<{} as {}::Fingerprint<__C>>::SCHEMA_HASH.to_le_bytes(), &{}::rapidhash::v3::RapidSecrets::seed_cpp(hash));", crate_name, field.type_string(), crate_name, crate_name))?;
                                        }
                                    }
                                 }
                            }
                        }
                    }
                    block.push_parsed("hash")?;
                    Ok(())
                })?;
                Ok(())
            })?;
        Ok(())
    }
}
