use alloc::{string::String, vec::Vec};
use core::{
    cmp::Ordering,
    fmt::{self, Write},
    ops::Range,
};

use smallvec::SmallVec;

use crate::Kind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    String(Vec<u8>),
    Parameter(Kind),
}

#[derive(Clone)]
pub struct Node<T> {
    pub key: Key,
    pub value: Option<T>,
    /// Stores string node
    pub nodes0: Option<Vec<Self>>,
    /// Stores parameter node
    pub nodes1: Option<Vec<Self>>,
}

impl<T: fmt::Debug> Node<T> {
    pub fn new(key: Key, value: Option<T>) -> Self {
        Self {
            key,
            value,
            nodes0: None,
            nodes1: None,
        }
    }

    pub fn insert_bytes(&mut self, mut bytes: &[u8]) -> &mut Self {
        let diff = match &mut self.key {
            Key::String(s) => {
                if s.is_empty() {
                    *s = bytes.to_vec();
                    return self;
                }

                let cursor = s
                    .iter()
                    .zip(bytes.iter())
                    .take_while(|(a, b)| a == b)
                    .count();

                if cursor == 0 {
                    true
                } else {
                    // split node
                    if cursor < s.len() {
                        let (prefix, suffix) = s.split_at(cursor);
                        let mut node = Node::new(Key::String(prefix.to_vec()), None);
                        *s = suffix.to_vec();
                        ::core::mem::swap(self, &mut node);
                        self.nodes0.get_or_insert_with(Vec::new).push(node);
                    }
                    if cursor == bytes.len() {
                        false
                    } else {
                        bytes = &bytes[cursor..];
                        true
                    }
                }
            }
            Key::Parameter(_) => true,
        };

        // insert node
        if diff {
            let nodes = self.nodes0.get_or_insert_with(Vec::new);
            return match nodes.binary_search_by(|node| match &node.key {
                Key::String(s) => {
                    // s[0].cmp(&bytes[0])
                    // opt!
                    // lets `/` at end
                    compare(s[0], bytes[0])
                }
                Key::Parameter(_) => unreachable!(),
            }) {
                Ok(i) => nodes[i].insert_bytes(bytes),
                Err(i) => {
                    nodes.insert(i, Node::new(Key::String(bytes.to_vec()), None));
                    &mut nodes[i]
                }
            };
        }

        self
    }

    pub fn insert_parameter(&mut self, kind: Kind) -> &mut Self {
        let nodes = self.nodes1.get_or_insert_with(Vec::new);
        let i = nodes
            .binary_search_by(|node| match node.key {
                Key::Parameter(pk) => pk.cmp(&kind),
                Key::String(_) => unreachable!(),
            })
            .unwrap_or_else(|i| {
                nodes.insert(i, Node::new(Key::Parameter(kind), None));
                i
            });
        &mut nodes[i]
    }

    #[allow(clippy::range_plus_one)]
    #[allow(clippy::too_many_lines)]
    #[inline]
    fn find_with(
        &self,
        mut start: usize,
        mut bytes: &[u8],
        ranges: &mut SmallVec<[Range<usize>; 8]>,
    ) -> Option<&T> {
        let mut m = bytes.len();
        match &self.key {
            Key::String(s) => {
                let n = s.len();
                let mut flag = m >= n;

                // opt!
                if flag {
                    if n == 1 {
                        flag = s[0] == bytes[0];
                    } else {
                        flag = s == &bytes[..n];
                    }
                }

                // starts with prefix
                if flag {
                    m -= n;
                    start += n;
                    bytes = &bytes[n..];

                    if m == 0 {
                        if let Some(id) = &self.value {
                            return Some(id);
                        }
                    } else {
                        // static
                        if let Some(id) = self.nodes0.as_ref().and_then(|nodes| {
                            nodes
                                .binary_search_by(|node| match &node.key {
                                    Key::String(s) => {
                                        // s[0].cmp(&bytes[0])
                                        // opt!
                                        // lets `/` at end
                                        compare(s[0], bytes[0])
                                    }
                                    Key::Parameter(_) => unreachable!(),
                                })
                                .ok()
                                .and_then(|i| nodes[i].find_with(start, bytes, ranges))
                        }) {
                            return Some(id);
                        }
                    }

                    // parameter
                    if let Some(id) = self.nodes1.as_ref().and_then(|nodes| {
                        let b = m > 0;
                        nodes
                            .iter()
                            .filter(|node| match node.key {
                                Key::Parameter(pk)
                                    if pk == Kind::Normal || pk == Kind::OneOrMore =>
                                {
                                    b
                                }
                                _ => true,
                            })
                            .find_map(|node| node.find_with(start, bytes, ranges))
                    }) {
                        return Some(id);
                    }
                } else if n == 1 && s[0] == b'/' {
                    if let Some(id) = self.nodes1.as_ref().and_then(|nodes| {
                        nodes
                            .iter()
                            .filter(|node| {
                                matches!(node.key,
                                    Key::Parameter(pk)
                                        if pk == Kind::OptionalSegment
                                            || pk == Kind::ZeroOrMoreSegment
                                )
                            })
                            .find_map(|node| node.find_with(start, bytes, ranges))
                    }) {
                        return Some(id);
                    }
                }
            }
            Key::Parameter(k) => match k {
                Kind::Normal | Kind::Optional | Kind::OptionalSegment => {
                    if m == 0 {
                        if k == &Kind::Normal {
                            return None;
                        }

                        // last
                        if self.nodes0.is_none() && self.nodes1.is_none() {
                            return self.value.as_ref().inspect(|_| {
                                ranges.push(start..start);
                            });
                        }
                    } else {
                        // static
                        if let Some(id) = self.nodes0.as_ref().and_then(|nodes| {
                            nodes.iter().find_map(|node| match &node.key {
                                Key::String(s) => {
                                    let mut keep_running = true;
                                    bytes
                                        .iter()
                                        // as it turns out doing .copied() here is much slower than dereferencing in the closure
                                        // https://godbolt.org/z/7dnW91T1Y
                                        .take_while(|b| {
                                            if keep_running && **b == b'/' {
                                                keep_running = false;
                                                true
                                            } else {
                                                keep_running
                                            }
                                        })
                                        .enumerate()
                                        .filter_map(|(n, b)| (s[0] == *b).then_some(n))
                                        .find_map(|n| {
                                            node.find_with(start + n, &bytes[n..], ranges).inspect(
                                                |_| {
                                                    ranges.push(start..start + n);
                                                },
                                            )
                                        })
                                }
                                Key::Parameter(_) => unreachable!(),
                            })
                        }) {
                            return Some(id);
                        }

                        // parameter => `:a:b:c`
                        if let Some(id) = self.nodes1.as_ref().and_then(|nodes| {
                            let b = m - 1 > 0;
                            nodes
                                .iter()
                                .filter(|node| match node.key {
                                    Key::Parameter(pk)
                                        if pk == Kind::Normal || pk == Kind::OneOrMore =>
                                    {
                                        b
                                    }
                                    _ => true,
                                })
                                .find_map(|node| node.find_with(start + 1, &bytes[1..], ranges))
                        }) {
                            ranges.push(start..start + 1);
                            return Some(id);
                        }
                    }

                    // parameter => `:a:b?:c?`
                    if k == &Kind::Optional || k == &Kind::OptionalSegment {
                        if let Some(id) = self.nodes1.as_ref().and_then(|nodes| {
                            let b = m > 0;
                            nodes
                                .iter()
                                .filter(|node| match &node.key {
                                    Key::Parameter(pk)
                                        if pk == &Kind::Normal || pk == &Kind::OneOrMore =>
                                    {
                                        b
                                    }
                                    _ => true,
                                })
                                .find_map(|node| node.find_with(start, bytes, ranges))
                        }) {
                            // param should be empty
                            ranges.push(start + m..start + m);
                            return Some(id);
                        }
                    }

                    if let Some(n) = bytes.iter().position(|b| *b == b'/') {
                        bytes = &bytes[n..];
                    } else {
                        if let Some(id) = &self.value {
                            ranges.push(start..start + m);
                            return Some(id);
                        }
                        bytes = &bytes[m..];
                    }

                    if k == &Kind::OptionalSegment {
                        if let Some(id) = self.nodes0.as_ref().and_then(|nodes| {
                            nodes
                                .last()
                                .filter(|node| match &node.key {
                                    Key::String(s) => s[0] == b'/',
                                    Key::Parameter(_) => unreachable!(),
                                })
                                .and_then(|node| node.find_with(start, bytes, ranges))
                        }) {
                            ranges.push(start..start + m);
                            return Some(id);
                        }
                    }
                }
                Kind::OneOrMore | Kind::ZeroOrMore | Kind::ZeroOrMoreSegment => {
                    let is_one_or_more = k == &Kind::OneOrMore;
                    if m == 0 {
                        if is_one_or_more {
                            return None;
                        }

                        if self.nodes0.is_none() && self.nodes1.is_none() {
                            return self.value.as_ref().inspect(|_| {
                                ranges.push(start..start);
                            });
                        }
                    } else {
                        if self.nodes0.is_none() && self.nodes1.is_none() {
                            if let Some(id) = &self.value {
                                ranges.push(start..start + m);
                                return Some(id);
                            }
                        }

                        // static
                        if let Some(id) = self.nodes0.as_ref().and_then(|nodes| {
                            nodes.iter().find_map(|node| {
                                if let Key::String(s) = &node.key {
                                    let right_length = if is_one_or_more {
                                        m > s.len()
                                    } else {
                                        m >= s.len()
                                    };
                                    if right_length {
                                        return bytes
                                            .iter()
                                            .enumerate()
                                            .filter_map(|(n, b)| (s[0] == *b).then_some(n))
                                            .find_map(|n| {
                                                node.find_with(start + n, &bytes[n..], ranges)
                                                    .inspect(|_| {
                                                        ranges.push(start..start + n);
                                                    })
                                            });
                                    }
                                }
                                None
                            })
                        }) {
                            return Some(id);
                        }
                    }

                    if k == &Kind::ZeroOrMoreSegment {
                        if let Some(id) = self.nodes0.as_ref().and_then(|nodes| {
                            nodes
                                .last()
                                .filter(|node| match &node.key {
                                    Key::String(s) => s[0] == b'/',
                                    Key::Parameter(_) => unreachable!(),
                                })
                                .and_then(|node| node.find_with(start, bytes, ranges))
                        }) {
                            // param should be empty
                            ranges.push(start + m..start + m);
                            return Some(id);
                        }
                    }
                }
            },
        }
        None
    }

    pub fn find(&self, bytes: &[u8]) -> Option<(&T, SmallVec<[Range<usize>; 8]>)> {
        let mut ranges = SmallVec::<[Range<usize>; 8]>::new_const(); // opt!
        self.find_with(0, bytes, &mut ranges).map(|t| (t, ranges))
    }
}

impl<T: fmt::Debug> fmt::Debug for Node<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const EDGE: &str = "├──";
        const LINE: &str = "│  ";
        const CORNER: &str = "└──";
        const BLANK: &str = "   ";

        fn print_nodes<T: fmt::Debug>(
            f: &mut fmt::Formatter<'_>,
            nodes: &[Node<T>],
            check: bool,
            pad: &str,
            space: &str,
        ) -> fmt::Result {
            for (index, node) in nodes.iter().enumerate() {
                let (left, right) = if check && index == nodes.len() - 1 {
                    (BLANK, CORNER)
                } else {
                    (LINE, EDGE)
                };
                f.write_str(pad)?;
                f.write_str(space)?;
                f.write_str(right)?;
                let mut s = String::new();
                s.push_str(pad);
                s.push_str(space);
                s.push_str(left);
                print_tree(f, node, false, &s)?;
            }
            Ok(())
        }

        fn print_tree<T: fmt::Debug>(
            f: &mut fmt::Formatter<'_>,
            node: &Node<T>,
            root: bool,
            pad: &str,
        ) -> fmt::Result {
            let space = if root {
                f.write_char('\n')?;
                ""
            } else {
                f.write_char(' ')?;
                " "
            };
            match &node.key {
                Key::String(path) => {
                    f.write_str(
                        &String::from_utf8_lossy(path)
                            .replace(':', "\\:")
                            .replace('?', "\\?")
                            .replace('+', "\\+"),
                    )?;
                }
                Key::Parameter(kind) => {
                    let c = match kind {
                        Kind::Normal => ':',
                        Kind::Optional => '?',
                        Kind::OptionalSegment => {
                            f.write_char('?')?;
                            '?'
                        }
                        Kind::OneOrMore => '+',
                        Kind::ZeroOrMore => '*',
                        Kind::ZeroOrMoreSegment => {
                            f.write_char('*')?;
                            '*'
                        }
                    };
                    f.write_char(c)?;
                }
            }
            if let Some(value) = &node.value {
                f.write_str(" •")?;
                value.fmt(f)?;
            }
            f.write_char('\n')?;

            // nodes0
            if let Some(nodes) = &node.nodes0 {
                print_nodes(f, nodes, node.nodes1.is_none(), pad, space)?;
            }

            // nodes1
            if let Some(nodes) = &node.nodes1 {
                print_nodes(f, nodes, true, pad, space)?;
            }

            Ok(())
        }

        print_tree(f, self, true, "")
    }
}

#[inline]
fn compare(a: u8, b: u8) -> Ordering {
    if a == b {
        Ordering::Equal
    } else if a == b'/' {
        Ordering::Greater
    } else if b == b'/' {
        Ordering::Less
    } else {
        a.cmp(&b)
    }
}
