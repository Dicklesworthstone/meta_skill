use crate::attribute::ContainerAttributes;
use crate::attribute::FieldAttributes;
use virtue::prelude::*;

pub(crate) struct DeriveStaticSize {
    pub fields: Option<Fields>,
    pub variants: Option<Vec<EnumVariant>>,
    pub attributes: ContainerAttributes,
}

/// Build a sum expression string for all fields in a `Fields` (no bit-packing).
fn fields_sum_expr(
    fields: &Fields,
    crate_name: &str,
) -> Result<String> {
    let mut parts: Vec<String> = Vec::new();
    match fields {
        | Fields::Struct(s) => {
            for (_ident, field) in s {
                let attrs = field
                    .attributes
                    .get_attribute::<FieldAttributes>()?
                    .unwrap_or_default();
                if attrs.static_size_skip {
                    continue;
                }
                if let Some(custom) = attrs.static_size_custom {
                    parts.push(custom);
                } else {
                    parts.push(format!(
                        "<{} as {}::StaticSize>::MAX_SIZE",
                        field.type_string(),
                        crate_name
                    ));
                }
            }
        },
        | Fields::Tuple(t) => {
            for field in t {
                let attrs = field
                    .attributes
                    .get_attribute::<FieldAttributes>()?
                    .unwrap_or_default();
                if attrs.static_size_skip {
                    continue;
                }
                if let Some(custom) = attrs.static_size_custom {
                    parts.push(custom);
                } else {
                    parts.push(format!(
                        "<{} as {}::StaticSize>::MAX_SIZE",
                        field.type_string(),
                        crate_name
                    ));
                }
            }
        },
    }
    if parts.is_empty() {
        Ok("0".to_string())
    } else {
        Ok(parts.join(" + "))
    }
}

/// Build a packed sum expression for all fields, collapsing consecutive bit-packed
/// fields into `ceil(sum_of_bits / 8)` byte contributions.
fn fields_packed_sum_expr(
    fields: &Fields,
    crate_name: &str,
) -> Result<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current_bits: u32 = 0;

    macro_rules! flush_bits {
        () => {
            if current_bits > 0 {
                parts.push(format!("{}", current_bits.div_ceil(8)));
                current_bits = 0;
            }
        };
    }

    macro_rules! process_field {
        ($field:expr) => {{
            let attrs = $field
                .attributes
                .get_attribute::<FieldAttributes>()?
                .unwrap_or_default();
            if !attrs.static_size_skip {
                if let Some(bits) = attrs.bits {
                    current_bits += bits as u32;
                } else {
                    flush_bits!();
                    if let Some(custom) = attrs.static_size_custom {
                        parts.push(custom);
                    } else {
                        parts.push(format!(
                            "<{} as {}::StaticSize>::PACKED_MAX_SIZE",
                            $field.type_string(),
                            crate_name
                        ));
                    }
                }
            }
        }};
    }

    match fields {
        | Fields::Struct(s) => {
            for (_ident, field) in s {
                process_field!(field);
            }
        },
        | Fields::Tuple(t) => {
            for field in t {
                process_field!(field);
            }
        },
    }
    // Final flush — inline to avoid a dead-write lint on `current_bits = 0`.
    if current_bits > 0 {
        parts.push(format!("{}", current_bits.div_ceil(8)));
    }

    if parts.is_empty() {
        Ok("0".to_string())
    } else {
        Ok(parts.join(" + "))
    }
}

/// Build a const-compatible "max of N values" expression.
///
/// Generates a nested `if/else` chain using intermediate `const` bindings
/// for stable Rust const evaluation compatibility:
/// ```text
/// { const __A: usize = e1; const __B: usize = e2; if __A > __B { __A } else { __B } }
/// ```
/// For 3+ values the nesting folds left, e.g. `max(max(a, b), c)`.
fn const_max_expr(exprs: &[String]) -> String {
    match exprs.len() {
        | 0 => "0".to_string(),
        | 1 => exprs[0].clone(),
        | _ => {
            let mut result = exprs[0].clone();
            for expr in &exprs[1..] {
                result = format!(
                    "{{ const __A: usize = {}; const __B: usize = {}; if __A > __B {{ __A }} else {{ __B }} }}",
                    result, expr
                );
            }
            result
        },
    }
}

/// Compute the StaticSize expression string from fields/variants.
/// `packed` controls whether bit-packed field widths are collapsed.
/// This is public so the Decode derive can reuse it for auto-deriving StaticSize.
pub(crate) fn compute_static_size_expr(
    fields: Option<&Fields>,
    variants: Option<&Vec<EnumVariant>>,
    crate_name: &str,
) -> Result<String> {
    if let Some(variants) = variants {
        // Enum: discriminant (max 5 bytes varint for u32) + max(variant sizes)
        if variants.is_empty() {
            Ok("5".to_string())
        } else {
            let mut variant_exprs = Vec::new();
            for variant in variants {
                if let Some(fields) = variant.fields.as_ref() {
                    variant_exprs.push(fields_sum_expr(fields, crate_name)?);
                } else {
                    variant_exprs.push("0".to_string());
                }
            }
            let max_expr = const_max_expr(&variant_exprs);
            Ok(format!("5 + {}", max_expr))
        }
    } else if let Some(fields) = fields {
        fields_sum_expr(fields, crate_name)
    } else {
        Ok("0".to_string())
    }
}

/// Compute the PACKED_MAX_SIZE expression — like `compute_static_size_expr` but
/// consecutive bit-packed fields are collapsed to `ceil(bits / 8)` bytes, and
/// non-packed fields use `PACKED_MAX_SIZE` recursively.
fn compute_packed_size_expr(
    fields: Option<&Fields>,
    variants: Option<&Vec<EnumVariant>>,
    crate_name: &str,
) -> Result<String> {
    if let Some(variants) = variants {
        // Enum discriminant is still a varint u32 (5 bytes) for non-BitPacked enums.
        if variants.is_empty() {
            Ok("5".to_string())
        } else {
            let mut variant_exprs = Vec::new();
            for variant in variants {
                if let Some(fields) = variant.fields.as_ref() {
                    variant_exprs.push(fields_packed_sum_expr(fields, crate_name)?);
                } else {
                    variant_exprs.push("0".to_string());
                }
            }
            let max_expr = const_max_expr(&variant_exprs);
            Ok(format!("5 + {}", max_expr))
        }
    } else if let Some(fields) = fields {
        fields_packed_sum_expr(fields, crate_name)
    } else {
        Ok("0".to_string())
    }
}

impl DeriveStaticSize {
    pub fn generate(
        self,
        generator: &mut Generator,
    ) -> Result<()> {
        let crate_name = &self.attributes.crate_name;

        let expr =
            compute_static_size_expr(self.fields.as_ref(), self.variants.as_ref(), crate_name)?;
        let packed_expr =
            compute_packed_size_expr(self.fields.as_ref(), self.variants.as_ref(), crate_name)?;

        // Two bindings are required: `impl_for()` returns an owned value whose address
        // must be stable before `modify_generic_constraints` borrows from it.
        let mut impl_for = generator.impl_for(format!("{}::StaticSize", crate_name));
        let impl_gen = impl_for.modify_generic_constraints(|generics, where_constraints| {
            for g in generics.iter_generics() {
                where_constraints.push_constraint(g, format!("{}::StaticSize", crate_name))?;
            }
            Ok(())
        })?;

        impl_gen.generate_const("MAX_SIZE", "usize").with_value(
            |fn_body: &mut StreamBuilder| {
                fn_body.push_parsed(&expr)?;
                Ok(())
            },
        )?;

        impl_gen
            .generate_const("PACKED_MAX_SIZE", "usize")
            .with_value(|fn_body: &mut StreamBuilder| {
                fn_body.push_parsed(&packed_expr)?;
                Ok(())
            })?;

        Ok(())
    }
}
