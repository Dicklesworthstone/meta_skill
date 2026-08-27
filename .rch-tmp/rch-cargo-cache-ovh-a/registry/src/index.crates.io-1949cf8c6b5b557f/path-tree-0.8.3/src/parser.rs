use alloc::{string::ToString, vec::Vec};
use core::{iter::Peekable, str::CharIndices};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `:` 58
    /// `:name`
    Normal,
    /// `?` 63
    /// Optional: `:name?-`
    Optional,
    /// Optional segment: `/:name?/` or `/:name?`
    OptionalSegment,
    // Optional,
    /// `+` 43
    OneOrMore,
    /// `*` 42
    /// Zero or more: `*-`
    ZeroOrMore,
    /// Zero or more segment: `/*/` or `/*`
    ZeroOrMoreSegment,
    // TODO: regexp
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    String(Vec<u8>),
    Parameter(Position, Kind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Position {
    Index(usize, Vec<u8>),
    Named(Vec<u8>),
}

pub struct Parser<'a> {
    pos: usize,
    count: usize,
    input: &'a str,
    cursor: Peekable<CharIndices<'a>>,
}

impl<'a> Parser<'a> {
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            count: 0,
            cursor: input.char_indices().peekable(),
        }
    }

    fn string(&mut self) -> &'a [u8] {
        let mut start = self.pos;
        while let Some(&(i, c)) = self.cursor.peek() {
            match c {
                '\\' => {
                    if start < i {
                        self.pos = i;
                        return self.input[start..i].as_bytes();
                    }

                    self.cursor.next();
                    if let Some(&(j, c)) = self.cursor.peek() {
                        // removes `\`
                        if c == '\\' {
                            start = j;
                        } else {
                            self.cursor.next();
                            self.pos = j + c.len_utf8();
                            return self.input[j..self.pos].as_bytes();
                        }
                    }
                }
                ':' | '+' | '*' => {
                    self.pos = i + 1;
                    return self.input[start..i].as_bytes();
                }
                _ => {
                    self.cursor.next();
                }
            }
        }

        self.input[start..].as_bytes()
    }

    fn parameter(&mut self) -> (Position, Kind) {
        let start = self.pos;
        while let Some(&(i, c)) = self.cursor.peek() {
            match c {
                '-' | '.' | '~' | '/' | '\\' | ':' => {
                    self.pos = i;
                    return (
                        Position::Named(self.input[start..i].as_bytes().to_vec()),
                        Kind::Normal,
                    );
                }
                '?' | '+' | '*' => {
                    self.cursor.next();
                    self.pos = i + 1;
                    return (
                        Position::Named(self.input[start..i].as_bytes().to_vec()),
                        if c == '+' {
                            Kind::OneOrMore
                        } else {
                            let f = {
                                let prefix = start >= 2
                                    && (self.input.get(start - 2..start - 1) == Some("/"));
                                let suffix = self.cursor.peek().is_none_or(|(_, c)| *c == '/');
                                prefix && suffix
                            };
                            if c == '?' {
                                if f {
                                    Kind::OptionalSegment
                                } else {
                                    Kind::Optional
                                }
                            } else if f {
                                Kind::ZeroOrMoreSegment
                            } else {
                                Kind::ZeroOrMore
                            }
                        },
                    );
                }
                _ => {
                    self.cursor.next();
                }
            }
        }

        (
            Position::Named(self.input[start..].as_bytes().to_vec()),
            Kind::Normal,
        )
    }
}

impl Iterator for Parser<'_> {
    type Item = Piece;

    fn next(&mut self) -> Option<Self::Item> {
        match self.cursor.peek() {
            Some(&(i, c)) => match c {
                ':' => {
                    self.cursor.next();
                    self.pos = i + 1;
                    let (position, kind) = self.parameter();
                    Some(Piece::Parameter(position, kind))
                }
                '+' | '*' => {
                    self.cursor.next();
                    self.count += 1;
                    self.pos = i + 1;
                    Some(Piece::Parameter(
                        Position::Index(self.count, {
                            let mut s = Vec::new();
                            s.push(c as u8);
                            s.extend_from_slice(self.count.to_string().as_bytes());
                            s
                        }),
                        if c == '+' {
                            Kind::OneOrMore
                        } else {
                            let f = {
                                let prefix = i >= 1 && (self.input.get(i - 1..i) == Some("/"));
                                let suffix = self.cursor.peek().is_none_or(|(_, c)| *c == '/');
                                prefix && suffix
                            };
                            if f {
                                Kind::ZeroOrMoreSegment
                            } else {
                                Kind::ZeroOrMore
                            }
                        },
                    ))
                }
                _ => Some(Piece::String(self.string().to_vec())),
            },
            None => None,
        }
    }
}
