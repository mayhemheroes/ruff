use ruff_text_size::TextRange;
use rustpython_parser::Tok;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;

/// Stores the ranges of comments sorted by [`TextRange::start`] in increasing order. No two ranges are overlapping.
#[derive(Clone)]
pub struct CommentRanges {
    raw: Vec<TextRange>,
}

impl Deref for CommentRanges {
    type Target = [TextRange];

    fn deref(&self) -> &Self::Target {
        self.raw.as_slice()
    }
}

impl Debug for CommentRanges {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CommentRanges").field(&self.raw).finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommentRangesBuilder {
    ranges: Vec<TextRange>,
}

impl CommentRangesBuilder {
    pub fn visit_token(&mut self, token: &Tok, range: TextRange) {
        if token.is_comment() {
            self.ranges.push(range);
        }
    }

    pub fn finish(self) -> CommentRanges {
        CommentRanges { raw: self.ranges }
    }
}
