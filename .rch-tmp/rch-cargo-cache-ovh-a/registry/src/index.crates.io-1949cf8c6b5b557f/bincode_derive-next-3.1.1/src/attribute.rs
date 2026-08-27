use virtue::prelude::*;
use virtue::utils::ParsedAttribute;
use virtue::utils::parse_tagged_attribute;

pub struct ContainerAttributes {
    pub crate_name: String,
    pub bounds: Option<(String, Literal)>,
    pub decode_bounds: Option<(String, Literal)>,
    pub decode_context: Option<(String, Literal)>,
    pub borrow_decode_bounds: Option<(String, Literal)>,
    pub encode_bounds: Option<(String, Literal)>,
    #[allow(dead_code)]
    pub endian: Option<(String, Literal)>,
    pub align: Option<(usize, Literal)>,
    pub bit_packing: Option<(String, Literal)>,
}

impl Default for ContainerAttributes {
    fn default() -> Self {
        Self {
            crate_name: "::bincode_next".to_string(),
            bounds: None,
            decode_bounds: None,
            decode_context: None,
            encode_bounds: None,
            borrow_decode_bounds: None,
            endian: None,
            align: None,
            bit_packing: None,
        }
    }
}

impl FromAttribute for ContainerAttributes {
    fn parse(group: &Group) -> Result<Option<Self>> {
        let attributes = match parse_tagged_attribute(group, "bincode")? {
            | Some(body) => body,
            | None => return Ok(None),
        };
        let mut result = Self::default();
        for attribute in attributes {
            match attribute {
                | ParsedAttribute::Property(key, val) if key.to_string() == "crate" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.crate_name = val_string[1..val_string.len() - 1].to_string();
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Property(key, val) if key.to_string() == "bounds" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Property(key, val) if key.to_string() == "decode_bounds" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.decode_bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Property(key, val) if key.to_string() == "decode_context" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.decode_context =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Property(key, val) if key.to_string() == "encode_bounds" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.encode_bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Property(key, val)
                    if key.to_string() == "borrow_decode_bounds" =>
                {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.borrow_decode_bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Property(key, val) if key.to_string() == "align" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        let inner = &val_string[1..val_string.len() - 1];
                        if let Ok(align) = inner.parse::<usize>() {
                            result.align = Some((align, val));
                        } else {
                            return Err(Error::custom_at("Should be a valid usize", val.span()));
                        }
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Property(key, val) if key.to_string() == "bit_packing" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        let inner = &val_string[1..val_string.len() - 1];
                        if inner == "msb" || inner == "lsb" {
                            result.bit_packing = Some((inner.to_string(), val));
                        } else {
                            return Err(Error::custom_at(
                                "Should be \"msb\" or \"lsb\"",
                                val.span(),
                            ));
                        }
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                },
                | ParsedAttribute::Tag(i) => {
                    return Err(Error::custom_at("Unknown field attribute", i.span()));
                },
                | ParsedAttribute::Property(key, _) => {
                    return Err(Error::custom_at("Unknown field attribute", key.span()));
                },
                | _ => {},
            }
        }
        Ok(Some(result))
    }
}

#[derive(Default)]
pub struct FieldAttributes {
    pub with_serde: bool,
    pub bits: Option<u8>,
    pub static_size_skip: bool,
    pub static_size_custom: Option<String>,
}

impl FromAttribute for FieldAttributes {
    fn parse(group: &Group) -> Result<Option<Self>> {
        let mut result = Self::default();
        let mut found = false;

        if let Some(attributes) = parse_tagged_attribute(group, "bincode")? {
            found = true;
            for attribute in attributes {
                match attribute {
                    | ParsedAttribute::Tag(i) if i.to_string() == "with_serde" => {
                        result.with_serde = true;
                    },
                    | ParsedAttribute::Property(key, val) if key.to_string() == "bits" => {
                        let val_string = val.to_string();
                        if let Ok(bits) = val_string.parse::<u8>() {
                            if bits == 0 || bits > 64 {
                                return Err(Error::custom_at(
                                    "Should be an integer from 1 to 64",
                                    val.span(),
                                ));
                            }
                            result.bits = Some(bits);
                        } else {
                            return Err(Error::custom_at(
                                "Should be an integer from 1 to 64",
                                val.span(),
                            ));
                        }
                    },
                    | ParsedAttribute::Tag(i) => {
                        return Err(Error::custom_at("Unknown field attribute", i.span()));
                    },
                    | ParsedAttribute::Property(key, _) => {
                        return Err(Error::custom_at("Unknown field attribute", key.span()));
                    },
                    | _ => {},
                }
            }
        }

        if let Some(attributes) = parse_tagged_attribute(group, "static_size")? {
            found = true;
            for attribute in attributes {
                match attribute {
                    | ParsedAttribute::Tag(i) if i.to_string() == "skip" => {
                        result.static_size_skip = true;
                    },
                    | ParsedAttribute::Property(key, val) if key.to_string() == "custom" => {
                        let val_string = val.to_string();
                        if val_string.starts_with('"') && val_string.ends_with('"') {
                            result.static_size_custom =
                                Some(val_string[1..val_string.len() - 1].to_string());
                        } else {
                            return Err(Error::custom_at("Should be a literal str", val.span()));
                        }
                    },
                    | ParsedAttribute::Tag(i) => {
                        return Err(Error::custom_at("Unknown static_size attribute", i.span()));
                    },
                    | ParsedAttribute::Property(key, _) => {
                        return Err(Error::custom_at(
                            "Unknown static_size attribute",
                            key.span(),
                        ));
                    },
                    | _ => {},
                }
            }
        }

        if found {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }
}
#[derive(Default)]
pub struct ReprAttributes {
    pub is_c: bool,
    pub is_transparent: bool,
    pub is_u8: bool,
    pub is_u16: bool,
    pub is_u32: bool,
    pub is_u64: bool,
    pub is_i8: bool,
    pub is_i16: bool,
    pub is_i32: bool,
    pub is_i64: bool,
}

impl FromAttribute for ReprAttributes {
    fn parse(group: &Group) -> Result<Option<Self>> {
        let attributes = match parse_tagged_attribute(group, "repr")? {
            | Some(body) => body,
            | None => return Ok(None),
        };
        let mut result = Self::default();
        let mut found = false;
        for attribute in attributes {
            match attribute {
                | ParsedAttribute::Tag(i) if i.to_string() == "C" => {
                    result.is_c = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "transparent" => {
                    result.is_transparent = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "u8" => {
                    result.is_u8 = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "u16" => {
                    result.is_u16 = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "u32" => {
                    result.is_u32 = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "u64" => {
                    result.is_u64 = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "i8" => {
                    result.is_i8 = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "i16" => {
                    result.is_i16 = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "i32" => {
                    result.is_i32 = true;
                    found = true;
                },
                | ParsedAttribute::Tag(i) if i.to_string() == "i64" => {
                    result.is_i64 = true;
                    found = true;
                },
                | _ => {},
            }
        }
        if found {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }
}
