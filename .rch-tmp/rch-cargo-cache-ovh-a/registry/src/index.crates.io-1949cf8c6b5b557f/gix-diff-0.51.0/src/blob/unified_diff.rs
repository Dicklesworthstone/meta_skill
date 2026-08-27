//! Facilities to produce the unified diff format.
//!
//! Originally based on <https://github.com/pascalkuthe/imara-diff/pull/14>.

/// Defines the size of the context printed before and after each change.
///
/// Similar to the `-U` option in git diff or gnu-diff. If the context overlaps
/// with previous or next change, the context gets reduced accordingly.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct ContextSize {
    /// Defines the size of the context printed before and after each change.
    symmetrical: u32,
}

impl Default for ContextSize {
    fn default() -> Self {
        ContextSize::symmetrical(3)
    }
}

/// Instantiation
impl ContextSize {
    /// Create a symmetrical context with `n` lines before and after a changed hunk.
    pub fn symmetrical(n: u32) -> Self {
        ContextSize { symmetrical: n }
    }
}

/// Specify where to put a newline.
#[derive(Debug, Copy, Clone)]
pub enum NewlineSeparator<'a> {
    /// Place the given newline separator, like `\n`, after each patch header as well as after each line.
    /// This is the right choice if tokens don't include newlines.
    AfterHeaderAndLine(&'a str),
    /// Place the given newline separator, like `\n`, only after each patch header or if a line doesn't contain a newline.
    /// This is the right choice if tokens do include newlines.
    /// Note that diff-tokens *with* newlines may diff strangely at the end of files when lines have been appended,
    /// as it will make the last line look like it changed just because the whitespace at the end 'changed'.
    AfterHeaderAndWhenNeeded(&'a str),
}

/// A utility trait for use in [`UnifiedDiff`](super::UnifiedDiff).
pub trait ConsumeHunk {
    /// The item this instance produces after consuming all hunks.
    type Out;

    /// Consume a single `hunk` in unified diff format, that would be prefixed with `header`.
    /// Note that all newlines are added.
    ///
    /// Note that the [`UnifiedDiff`](super::UnifiedDiff) sink will wrap its output in an [`std::io::Result`].
    /// After this method returned its first error, it will not be called anymore.
    ///
    /// The following is hunk-related information and the same that is used in the `header`.
    /// * `before_hunk_start` is the 1-based first line of this hunk in the old file.
    /// * `before_hunk_len` the amount of lines of this hunk in the old file.
    /// * `after_hunk_start` is the 1-based first line of this hunk in the new file.
    /// * `after_hunk_len` the amount of lines of this hunk in the new file.
    fn consume_hunk(
        &mut self,
        before_hunk_start: u32,
        before_hunk_len: u32,
        after_hunk_start: u32,
        after_hunk_len: u32,
        header: &str,
        hunk: &[u8],
    ) -> std::io::Result<()>;
    /// Called after the last hunk is consumed to produce an output.
    fn finish(self) -> Self::Out;
}

pub(super) mod _impl {
    use super::{ConsumeHunk, ContextSize, NewlineSeparator};
    use bstr::{ByteSlice, ByteVec};
    use imara_diff::{intern, Sink};
    use intern::{InternedInput, Interner, Token};
    use std::hash::Hash;
    use std::io::ErrorKind;
    use std::ops::Range;

    /// A [`Sink`] that creates a textual diff in the format typically output by git or `gnu-diff` if the `-u` option is used,
    /// and passes it in full to a consumer.
    pub struct UnifiedDiff<'a, T, D>
    where
        T: Hash + Eq + AsRef<[u8]>,
        D: ConsumeHunk,
    {
        before: &'a [Token],
        after: &'a [Token],
        interner: &'a Interner<T>,

        pos: u32,
        before_hunk_start: u32,
        after_hunk_start: u32,
        before_hunk_len: u32,
        after_hunk_len: u32,
        /// Symmetrical context before and after the changed hunk.
        ctx_size: u32,

        buffer: Vec<u8>,
        header_buf: String,
        delegate: D,
        newline: NewlineSeparator<'a>,

        err: Option<std::io::Error>,
    }

    impl<'a, T, D> UnifiedDiff<'a, T, D>
    where
        T: Hash + Eq + AsRef<[u8]>,
        D: ConsumeHunk,
    {
        /// Create a new instance to create unified diff using the lines in `input`,
        /// which also must be used when running the diff algorithm.
        /// `context_size` is the amount of lines around each hunk which will be passed
        ///to `consume_hunk`.
        ///
        /// `consume_hunk` is called for each hunk in unified-diff format, as created from each line separated by `newline_separator`.
        pub fn new(
            input: &'a InternedInput<T>,
            consume_hunk: D,
            newline_separator: NewlineSeparator<'a>,
            context_size: ContextSize,
        ) -> Self {
            Self {
                before_hunk_start: 0,
                after_hunk_start: 0,
                before_hunk_len: 0,
                after_hunk_len: 0,
                buffer: Vec::with_capacity(8),
                header_buf: String::new(),
                delegate: consume_hunk,
                interner: &input.interner,
                before: &input.before,
                after: &input.after,
                pos: 0,
                ctx_size: context_size.symmetrical,
                newline: newline_separator,

                err: None,
            }
        }

        fn print_tokens(&mut self, tokens: &[Token], prefix: char) {
            for &token in tokens {
                self.buffer.push_char(prefix);
                let line = &self.interner[token];
                self.buffer.push_str(line);
                match self.newline {
                    NewlineSeparator::AfterHeaderAndLine(nl) => {
                        self.buffer.push_str(nl);
                    }
                    NewlineSeparator::AfterHeaderAndWhenNeeded(nl) => {
                        if !line.as_ref().ends_with_str(nl) {
                            self.buffer.push_str(nl);
                        }
                    }
                }
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.before_hunk_len == 0 && self.after_hunk_len == 0 {
                return Ok(());
            }

            let end = (self.pos + self.ctx_size).min(self.before.len() as u32);
            self.update_pos(end, end);

            self.header_buf.clear();

            std::fmt::Write::write_fmt(
                &mut self.header_buf,
                format_args!(
                    "@@ -{},{} +{},{} @@{nl}",
                    self.before_hunk_start + 1,
                    self.before_hunk_len,
                    self.after_hunk_start + 1,
                    self.after_hunk_len,
                    nl = match self.newline {
                        NewlineSeparator::AfterHeaderAndLine(nl) | NewlineSeparator::AfterHeaderAndWhenNeeded(nl) => {
                            nl
                        }
                    }
                ),
            )
            .map_err(|err| std::io::Error::new(ErrorKind::Other, err))?;
            self.delegate.consume_hunk(
                self.before_hunk_start + 1,
                self.before_hunk_len,
                self.after_hunk_start + 1,
                self.after_hunk_len,
                &self.header_buf,
                &self.buffer,
            )?;
            self.buffer.clear();
            self.before_hunk_len = 0;
            self.after_hunk_len = 0;
            Ok(())
        }

        fn update_pos(&mut self, print_to: u32, move_to: u32) {
            self.print_tokens(&self.before[self.pos as usize..print_to as usize], ' ');
            let len = print_to - self.pos;
            self.pos = move_to;
            self.before_hunk_len += len;
            self.after_hunk_len += len;
        }
    }

    impl<T, D> Sink for UnifiedDiff<'_, T, D>
    where
        T: Hash + Eq + AsRef<[u8]>,
        D: ConsumeHunk,
    {
        type Out = std::io::Result<D::Out>;

        fn process_change(&mut self, before: Range<u32>, after: Range<u32>) {
            if self.err.is_some() {
                return;
            }
            if ((self.pos == 0) && (before.start - self.pos > self.ctx_size))
                || (before.start - self.pos > 2 * self.ctx_size)
            {
                if let Err(err) = self.flush() {
                    self.err = Some(err);
                    return;
                }
                self.pos = before.start - self.ctx_size;
                self.before_hunk_start = self.pos;
                self.after_hunk_start = after.start - self.ctx_size;
            }
            self.update_pos(before.start, before.end);
            self.before_hunk_len += before.end - before.start;
            self.after_hunk_len += after.end - after.start;
            self.print_tokens(&self.before[before.start as usize..before.end as usize], '-');
            self.print_tokens(&self.after[after.start as usize..after.end as usize], '+');
        }

        fn finish(mut self) -> Self::Out {
            if let Err(err) = self.flush() {
                self.err = Some(err);
            }
            if let Some(err) = self.err {
                return Err(err);
            }
            Ok(self.delegate.finish())
        }
    }

    /// An implementation that fails if the input isn't UTF-8.
    impl ConsumeHunk for String {
        type Out = Self;

        fn consume_hunk(&mut self, _: u32, _: u32, _: u32, _: u32, header: &str, hunk: &[u8]) -> std::io::Result<()> {
            self.push_str(header);
            self.push_str(
                hunk.to_str()
                    .map_err(|err| std::io::Error::new(ErrorKind::Other, err))?,
            );
            Ok(())
        }

        fn finish(self) -> Self::Out {
            self
        }
    }

    /// An implementation that writes hunks into a byte buffer.
    impl ConsumeHunk for Vec<u8> {
        type Out = Self;

        fn consume_hunk(&mut self, _: u32, _: u32, _: u32, _: u32, header: &str, hunk: &[u8]) -> std::io::Result<()> {
            self.push_str(header);
            self.push_str(hunk);
            Ok(())
        }

        fn finish(self) -> Self::Out {
            self
        }
    }
}
