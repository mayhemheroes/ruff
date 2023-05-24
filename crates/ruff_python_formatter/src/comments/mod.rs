//! Types for extracting and representing comments of a syntax tree.
//!
//! Most programming languages support comments allowing programmers to document their programs. Comments are different from other syntaxes  because programming languages allow comments in almost any position, giving programmers great flexibility on where they can write comments:
//!
//! ```ignore
//! /**
//!  * Documentation comment
//!  */
//! async /* comment */ function Test () // line comment
//! {/*inline*/}
//! ```
//!
//! However, this flexibility makes formatting comments challenging because:
//! * The formatter must consistently place comments so that re-formatting the output yields the same result and does not create invalid syntax (line comments).
//! * It is essential that formatters place comments close to the syntax the programmer intended to document. However, the lack of rules regarding where comments are allowed and what syntax they document requires the use of heuristics to infer the documented syntax.
//!
//! This module strikes a balance between placing comments as closely as possible to their source location and reducing the complexity of formatting comments. It does so by associating comments per node rather than a token. This greatly reduces the combinations of possible comment positions but turns out to be, in practice, sufficiently precise to keep comments close to their source location.
//!
//! ## Node comments
//!
//! Comments are associated per node but get further distinguished on their location related to that node:
//!
//! ### Leading Comments
//!
//! A comment at the start of a node
//!
//! ```ignore
//! // Leading comment of the statement
//! console.log("test");
//!
//! [/* leading comment of identifier */ a ];
//! ```
//!
//! ### Dangling Comments
//!
//! A comment that is neither at the start nor the end of a node
//!
//! ```ignore
//! [/* in between the brackets */ ];
//! async  /* between keywords */  function Test () {}
//! ```
//!
//! ### Trailing Comments
//!
//! A comment at the end of a node
//!
//! ```ignore
//! [a /* trailing comment of a */, b, c];
//! [
//!     a // trailing comment of a
//! ]
//! ```
//!
//! ## Limitations
//! Limiting the placement of comments to leading, dangling, or trailing node comments reduces complexity inside the formatter but means, that the formatter's possibility of where comments can be formatted depends on the AST structure.
//!
//! For example, the continue statement in JavaScript is defined as:
//!
//! ```ungram
//! JsContinueStatement =
//! 'continue'
//! (label: 'ident')?
//! ';'?
//! ```
//!
//! but a programmer may decide to add a comment in front or after the label:
//!
//! ```ignore
//! continue /* comment 1 */ label;
//! continue label /* comment 2*/; /* trailing */
//! ```
//!
//! Because all children of the `continue` statement are tokens, it is only possible to make the comments leading, dangling, or trailing comments of the `continue` statement. But this results in a loss of information as the formatting code can no longer distinguish if a comment appeared before or after the label and, thus, has to format them the same way.
//!
//! This hasn't shown to be a significant limitation today but the infrastructure could be extended to support a `label` on [`SourceComment`] that allows to further categorise comments.
//!

use std::cell::Cell;
use std::fmt::{Debug, Formatter};

mod builder;
mod comments;
mod map;
mod node_key;
mod placement;

pub(crate) use builder::CommentsVisitor;
pub(crate) use comments::Comments;
use ruff_formatter::{SourceCode, SourceCodeSlice};

/// A comment in the source document.
#[derive(Debug, Clone)]
pub(crate) struct SourceComment {
    /// The number of lines appearing before this comment
    pub(super) lines_before: u32,

    pub(super) lines_after: u32,

    /// The slice of the comment in the source document
    pub(super) slice: SourceCodeSlice,

    /// Whether the comment has been formatted or not.
    #[cfg(debug_assertions)]
    pub(super) formatted: Cell<bool>,
}

impl SourceComment {
    pub fn slice(&self) -> &SourceCodeSlice {
        &self.slice
    }

    /// The number of lines between this comment and the **previous** token or comment.
    ///
    /// # Examples
    ///
    /// ## Same line
    ///
    /// ```ignore
    /// a // end of line
    /// ```
    ///
    /// Returns `0` because there's no line break between the token `a` and the comment.
    ///
    /// ## Own Line
    ///
    /// ```ignore
    /// a;
    ///
    /// /* comment */
    /// ```
    ///
    /// Returns `2` because there are two line breaks between the token `a` and the comment.
    pub fn lines_before(&self) -> u32 {
        self.lines_before
    }

    /// The number of line breaks right after this comment.
    ///
    /// # Examples
    ///
    /// ## End of line
    ///
    /// ```ignore
    /// a; // comment
    ///
    /// b;
    /// ```
    ///
    /// Returns `2` because there are two line breaks between the comment and the token `b`.
    ///
    /// ## Same line
    ///
    /// ```ignore
    /// a;
    /// /* comment */ b;
    /// ```
    ///
    /// Returns `0` because there are no line breaks between the comment and the token `b`.
    pub fn lines_after(&self) -> u32 {
        self.lines_after
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub fn mark_formatted(&self) {}

    /// Marks the comment as formatted
    #[cfg(debug_assertions)]
    pub fn mark_formatted(&self) {
        self.formatted.set(true)
    }
}

impl SourceComment {
    pub(crate) fn debug<'a>(&'a self, source_code: SourceCode<'a>) -> DebugComment<'a> {
        DebugComment {
            comment: self,
            source_code,
        }
    }
}

pub(crate) struct DebugComment<'a> {
    comment: &'a SourceComment,
    source_code: SourceCode<'a>,
}

impl Debug for DebugComment<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut strut = f.debug_struct("SourceComment");

        strut
            .field("text", &self.comment.slice.text(self.source_code))
            .field("lines_before", &self.comment.lines_before)
            .field("lines_after", &self.comment.lines_after);

        #[cfg(debug_assertions)]
        strut.field("formatted", &self.comment.formatted.get());

        strut.finish()
    }
}
