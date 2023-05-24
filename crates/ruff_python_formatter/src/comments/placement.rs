use crate::comments::SourceComment;
use ruff_formatter::SourceCodeSlice;
use ruff_python_ast::node::AnyNodeRef;
use std::cell::Cell;

/// A comment decorated with additional information about its surrounding context in the source document.
///
/// Used by [CommentStyle::place_comment] to determine if this should become a [leading](self#leading-comments), [dangling](self#dangling-comments), or [trailing](self#trailing-comments) comment.
#[derive(Debug, Clone)]
pub(crate) struct DecoratedComment<'a> {
    enclosing: AnyNodeRef<'a>,
    preceding: Option<AnyNodeRef<'a>>,
    following: Option<AnyNodeRef<'a>>,
    text_position: CommentTextPosition,
    lines_before: u32,
    lines_after: u32,
    slice: SourceCodeSlice,
}

impl<'a> DecoratedComment<'a> {
    /// The closest parent node that fully encloses the comment.
    ///
    /// A node encloses a comment when the comment is between two of its direct children (ignoring lists).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// [a, /* comment */ b]
    /// ```
    ///
    /// The enclosing node is the array expression and not the identifier `b` because
    /// `a` and `b` are children of the array expression and `comment` is a comment between the two nodes.
    pub fn enclosing_node(&self) -> AnyNodeRef<'a> {
        self.enclosing
    }

    /// Returns the comment piece.
    pub fn slice(&self) -> &SourceCodeSlice {
        &self.slice
    }

    /// Returns the node preceding the comment.
    ///
    /// The direct child node (ignoring lists) of the [`enclosing_node`](DecoratedComment::enclosing_node_id) that precedes this comment.
    ///
    /// Returns [None] if the [`enclosing_node`](DecoratedComment::enclosing_node_id) only consists of tokens or if
    /// all preceding children of the [`enclosing_node`](DecoratedComment::enclosing_node_id) have been tokens.
    ///
    /// The Preceding node is guaranteed to be a sibling of [`following_node`](DecoratedComment::following_node).
    ///
    /// # Examples
    ///
    /// ## Preceding tokens only
    ///
    /// ```ignore
    /// [/* comment */]
    /// ```
    /// Returns [None] because the comment has no preceding node, only a preceding `[` token.
    ///
    /// ## Preceding node
    ///
    /// ```ignore
    /// [a /* comment */, b]
    /// ```
    ///
    /// Returns `Some(a)` because `a` directly precedes the comment.
    ///
    /// ## Preceding token and node
    ///
    /// ```ignore
    /// [a, /* comment */]
    /// ```
    ///
    ///  Returns `Some(a)` because `a` is the preceding node of `comment`. The presence of the `,` token
    /// doesn't change that.
    pub fn preceding_node(&self) -> Option<AnyNodeRef<'a>> {
        self.preceding
    }

    /// Returns the node following the comment.
    ///
    /// The direct child node (ignoring lists) of the [`enclosing_node`](DecoratedComment::enclosing_node_id) that follows this comment.
    ///
    /// Returns [None] if the [`enclosing_node`](DecoratedComment::enclosing_node_id) only consists of tokens or if
    /// all children children of the [`enclosing_node`](DecoratedComment::enclosing_node_id) following this comment are tokens.
    ///
    /// The following node is guaranteed to be a sibling of [`preceding_node`](DecoratedComment::preceding_node).
    ///
    /// # Examples
    ///
    /// ## Following tokens only
    ///
    /// ```ignore
    /// [ /* comment */ ]
    /// ```
    ///
    /// Returns [None] because there's no node following the comment, only the `]` token.
    ///
    /// ## Following node
    ///
    /// ```ignore
    /// [ /* comment */ a ]
    /// ```
    ///
    /// Returns `Some(a)` because `a` is the node directly following the comment.
    ///
    /// ## Following token and node
    ///
    /// ```ignore
    /// async /* comment */ function test() {}
    /// ```
    ///
    /// Returns `Some(test)` because the `test` identifier is the first node following `comment`.
    ///
    /// ## Following parenthesized expression
    ///
    /// ```ignore
    /// !(
    ///     a /* comment */
    /// );
    /// b
    /// ```
    ///
    /// Returns `None` because `comment` is enclosed inside the parenthesized expression and it has no children
    /// following `/* comment */.
    pub fn following_node(&self) -> Option<AnyNodeRef<'a>> {
        self.following
    }

    /// The number of line breaks between this comment and the **previous** token or comment.
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

    /// The position of the comment in the text.
    pub fn text_position(&self) -> CommentTextPosition {
        self.text_position
    }
}

impl From<DecoratedComment<'_>> for SourceComment {
    fn from(decorated: DecoratedComment) -> Self {
        Self {
            lines_before: decorated.lines_before,
            lines_after: decorated.lines_after,
            slice: decorated.slice,
            #[cfg(debug_assertions)]
            formatted: Cell::new(false),
        }
    }
}

/// The position of a comment in the source text.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum CommentTextPosition {
    /// A comment that is on the same line as the preceding token and is separated by at least one line break from the following token.
    ///
    /// # Examples
    ///
    /// ## End of line
    ///
    /// ```ignore
    /// a; /* this */ // or this
    /// b;
    /// ```
    ///
    /// Both `/* this */` and `// or this` are end of line comments because both comments are separated by
    /// at least one line break from the following token `b`.
    ///
    /// ## Own line
    ///
    /// ```ignore
    /// a;
    /// /* comment */
    /// b;
    /// ```
    ///
    /// This is not an end of line comment because it isn't on the same line as the preceding token `a`.
    EndOfLine,

    /// A Comment that is separated by at least one line break from the preceding token.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// a;
    /// /* comment */ /* or this */
    /// b;
    /// ```
    ///
    /// Both comments are own line comments because they are separated by one line break from the preceding
    /// token `a`.
    OwnLine,

    /// A comment that is placed on the same line as the preceding and following token.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// a /* comment */ + b
    /// ```
    SameLine,
}

impl CommentTextPosition {
    pub const fn is_same_line(&self) -> bool {
        matches!(self, CommentTextPosition::SameLine)
    }

    pub const fn is_own_line(&self) -> bool {
        matches!(self, CommentTextPosition::OwnLine)
    }

    pub const fn is_end_of_line(&self) -> bool {
        matches!(self, CommentTextPosition::EndOfLine)
    }
}

#[derive(Debug)]
pub(crate) enum CommentPlacement<'a> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    Leading {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },
    /// Makes `comment` a [trailing comment](self#trailing-comments) of `node`.
    Trailing {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },

    /// Makes `comment` a [dangling comment](self#dangling-comments) of `node`.
    Dangling {
        node: AnyNodeRef<'a>,
        comment: SourceComment,
    },

    /// Uses the default heuristic to determine the placement of the comment.
    ///
    /// # Same line comments
    ///
    /// Makes the comment a...
    ///
    /// * [trailing comment] of the [`preceding_node`] if both the [`following_node`] and [`preceding_node`] are not [None]
    ///     and the comment and [`preceding_node`] are only separated by a space (there's no token between the comment and [`preceding_node`]).
    /// * [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * [dangling comment] of the [`enclosing_node`].
    ///
    /// ## Examples
    /// ### Comment with preceding and following nodes
    ///
    /// ```ignore
    /// [
    ///     a, // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```ignore
    /// [
    ///     a // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```ignore
    /// [ // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```ignore
    /// [ // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the enclosing array expression because both the [`preceding_node`] and [`following_node`] are [None].
    ///
    /// # Own line comments
    ///
    /// Makes the comment a...
    ///
    /// * [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * or a [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * or a [dangling comment] of the [`enclosing_node`].
    ///
    /// ## Examples
    ///
    /// ### Comment with leading and preceding nodes
    ///
    /// ```ignore
    /// [
    ///     a,
    ///     // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```ignore
    /// [
    ///     a
    ///     // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a`.
    ///
    /// ### Comment with following node only
    ///
    /// ```ignore
    /// [
    ///     // comment
    ///     b
    /// ]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b`.
    ///
    /// ### Dangling comment
    ///
    /// ```ignore
    /// [
    ///     // comment
    /// ]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the array expression because both [`preceding_node`] and [`following_node`] are [None].
    ///
    ///
    /// # End of line comments
    /// Makes the comment a...
    ///
    /// * [trailing comment] of the [`preceding_node`] if the [`preceding_node`] is not [None]
    /// * or a [leading comment] of the [`following_node`] if the [`following_node`] is not [None]
    /// * or a [dangling comment] of the [`enclosing_node`].
    ///
    ///
    /// ## Examples
    ///
    /// ### Comment with leading and preceding nodes
    ///
    /// ```ignore
    /// [a /* comment */, b]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a` because there's no token between the node `a` and the `comment`.
    ///
    /// ```ignore
    /// [a, /* comment */ b]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b` because the node `a` and the comment are separated by a `,` token.
    ///
    /// ### Comment with preceding node only
    ///
    /// ```ignore
    /// [a, /* last */ ]
    /// ```
    ///
    /// The comment becomes a [trailing comment] of the node `a` because the [`following_node`] is [None].
    ///
    /// ### Comment with following node only
    ///
    /// ```ignore
    /// [/* comment */ b]
    /// ```
    ///
    /// The comment becomes a [leading comment] of the node `b` because the [`preceding_node`] is [None]
    ///
    /// ### Dangling comment
    ///
    /// ```ignore
    /// [/* comment*/]
    /// ```
    ///
    /// The comment becomes a [dangling comment] of the array expression because both [`preceding_node`] and [`following_node`] are [None].
    ///
    /// [`preceding_node`]: DecoratedComment::preceding_node
    /// [`following_node`]: DecoratedComment::following_node
    /// [`enclosing_node`]: DecoratedComment::enclosing_node_id
    /// [trailing comment]: self#trailing-comments
    /// [leading comment]: self#leading-comments
    /// [dangling comment]: self#dangling-comments
    Default(DecoratedComment<'a>),
}

impl<'a> CommentPlacement<'a> {
    /// Makes `comment` a [leading comment](self#leading-comments) of `node`.
    #[inline]
    pub fn leading(node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) -> Self {
        Self::Leading {
            node,
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [dangling comment](self::dangling-comments) of `node`.
    pub fn dangling(node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) -> Self {
        Self::Dangling {
            node,
            comment: comment.into(),
        }
    }

    /// Makes `comment` a [trailing comment](self::trailing-comments) of `node`.
    #[inline]
    pub fn trailing(node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) -> Self {
        Self::Trailing {
            node,
            comment: comment.into(),
        }
    }

    /// Returns the placement if it isn't [CommentPlacement::Default], otherwise calls `f` and returns the result.
    #[inline]
    pub fn or_else<F>(self, f: F) -> Self
    where
        F: FnOnce(DecoratedComment<'a>) -> CommentPlacement<'a>,
    {
        match self {
            CommentPlacement::Default(comment) => f(comment),
            placement => placement,
        }
    }
}

/// Defines how to format comments for a specific [Language].
pub(crate) trait CommentStyle: Default {
    /// Determines the placement of `comment`.
    ///
    /// The default implementation returns [CommentPlacement::Default].
    fn place_comment<'a>(&self, comment: DecoratedComment<'a>) -> CommentPlacement<'a> {
        CommentPlacement::Default(comment)
    }
}
