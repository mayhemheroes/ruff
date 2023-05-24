use crate::comments::map::CommentsMap as GenericCommentsMap;
use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::SourceComment;
use ruff_formatter::SourceCode;
use ruff_python_ast::node::AnyNodeRef;
use std::fmt::{Debug, Formatter};
use std::rc::Rc;

type CommentsMap<'a> = GenericCommentsMap<NodeRefEqualityKey<'a>, SourceComment>;

/// The comments of a syntax tree stored by node.
///
/// Cloning `comments` is cheap as it only involves bumping a reference counter.
#[derive(Clone, Default)]
pub(crate) struct Comments<'a> {
    /// The use of a [Rc] is necessary to achieve that [Comments] has a lifetime that is independent from the [crate::Formatter].
    /// Having independent lifetimes is necessary to support the use case where a (formattable object)[crate::Format]
    /// iterates over all comments, and writes them into the [crate::Formatter] (mutably borrowing the [crate::Formatter] and in turn its context).
    ///
    /// ```block
    /// for leading in f.context().comments().leading_comments(node) {
    ///     ^
    ///     |- Borrows comments
    ///   write!(f, [comment(leading.piece.text())])?;
    ///          ^
    ///          |- Mutably borrows the formatter, state, context, and comments (if comments aren't cloned)
    /// }
    /// ```
    ///
    /// Using an `Rc` here allows to cheaply clone [Comments] for these use cases.
    data: Rc<CommentsData<'a>>,
}

const fn key(node: AnyNodeRef) -> NodeRefEqualityKey {
    NodeRefEqualityKey::from_ref(node)
}

impl<'a> Comments<'a> {
    pub(super) fn new(comments: CommentsMap<'a>) -> Self {
        Self {
            data: Rc::new(CommentsData { comments }),
        }
    }

    #[inline]
    pub(crate) fn has_comments(&self, node: AnyNodeRef) -> bool {
        self.data.comments.has(&key(node))
    }

    /// Returns `true` if the given `node` has any [leading comments](self#leading-comments).
    #[inline]
    pub(crate) fn has_leading_comments(&self, node: AnyNodeRef) -> bool {
        !self.leading_comments(node).is_empty()
    }

    /// Tests if the node has any [leading comments](self#leading-comments) that have a leading line break.
    ///
    /// Corresponds to [CommentTextPosition::OwnLine].
    pub(crate) fn has_leading_own_line_comment(&self, node: AnyNodeRef) -> bool {
        self.leading_comments(node)
            .iter()
            .any(|comment| comment.lines_after() > 0)
    }

    /// Returns the `node`'s [leading comments](self#leading-comments).
    #[inline]
    pub(crate) fn leading_comments(&self, node: AnyNodeRef<'a>) -> &[SourceComment] {
        self.data.comments.leading(&key(node))
    }

    /// Returns `true` if node has any [dangling comments](self#dangling-comments).
    pub(crate) fn has_dangling_comments(&self, node: AnyNodeRef<'a>) -> bool {
        !self.dangling_comments(node).is_empty()
    }

    /// Returns the [dangling comments](self#dangling-comments) of `node`
    pub(crate) fn dangling_comments(&self, node: AnyNodeRef<'a>) -> &[SourceComment] {
        self.data.comments.dangling(&key(node))
    }

    /// Returns the `node`'s [trailing comments](self#trailing-comments).
    #[inline]
    pub(crate) fn trailing_comments(&self, node: AnyNodeRef<'a>) -> &[SourceComment] {
        self.data.comments.trailing(&key(node))
    }

    /// Returns `true` if the given `node` has any [trailing comments](self#trailing-comments).
    #[inline]
    pub(crate) fn has_trailing_comments(&self, node: AnyNodeRef) -> bool {
        !self.trailing_comments(node).is_empty()
    }

    /// Returns an iterator over the [leading](self#leading-comments) and [trailing comments](self#trailing-comments) of `node`.
    pub(crate) fn leading_trailing_comments(
        &self,
        node: AnyNodeRef<'a>,
    ) -> impl Iterator<Item = &SourceComment> {
        self.leading_comments(node)
            .iter()
            .chain(self.trailing_comments(node).iter())
    }

    /// Returns an iterator over the [leading](self#leading-comments), [dangling](self#dangling-comments), and [trailing](self#trailing) comments of `node`.
    pub(crate) fn leading_dangling_trailing_comments(
        &self,
        node: AnyNodeRef<'a>,
    ) -> impl Iterator<Item = &SourceComment> {
        self.data.comments.parts(&key(node))
    }

    #[inline(always)]
    #[cfg(not(debug_assertions))]
    pub(crate) fn assert_formatted_all_comments(&self) {}

    #[cfg(debug_assertions)]
    pub(crate) fn assert_formatted_all_comments(&self) {
        let unformatted_comments: Vec<_> = self.data.comments.all_parts().collect();

        if !unformatted_comments.is_empty() {
            panic!("The following comments have not been formatted.\n{unformatted_comments:#?}")
        }
    }

    pub(crate) fn debug(&'a self, source_code: SourceCode<'a>) -> DebugComments<'a> {
        DebugComments {
            comments: &self.data.comments,
            source_code,
        }
    }
}

#[derive(Default)]
struct CommentsData<'a> {
    /// Stores all leading node comments by node
    comments: CommentsMap<'a>,
}

impl Debug for CommentsData<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.comments, f)
    }
}

/// Pretty-printed debug representation of [`Comments`].
pub(crate) struct DebugComments<'a> {
    comments: &'a CommentsMap<'a>,
    source_code: SourceCode<'a>,
}

impl Debug for DebugComments<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for node in self.comments.keys() {
            map.entry(
                &node,
                &DebugNodeComments {
                    comments: self.comments,
                    source_code: self.source_code,
                    key: *node,
                },
            );
        }

        map.finish()
    }
}

struct DebugNodeComments<'a> {
    comments: &'a CommentsMap<'a>,
    source_code: SourceCode<'a>,
    key: NodeRefEqualityKey<'a>,
}

impl Debug for DebugNodeComments<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entry(
                &"leading",
                &DebugNodeCommentSlice {
                    node_comments: self.comments.leading(&self.key),
                    source_code: self.source_code,
                },
            )
            .entry(
                &"dangling",
                &DebugNodeCommentSlice {
                    node_comments: self.comments.dangling(&self.key),
                    source_code: self.source_code,
                },
            )
            .entry(
                &"trailing",
                &DebugNodeCommentSlice {
                    node_comments: self.comments.trailing(&self.key),
                    source_code: self.source_code,
                },
            )
            .finish()
    }
}

struct DebugNodeCommentSlice<'a> {
    node_comments: &'a [SourceComment],
    source_code: SourceCode<'a>,
}

impl Debug for DebugNodeCommentSlice<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();

        for comment in self.node_comments {
            list.entry(&comment.debug(self.source_code));
        }

        list.finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::comments::comments::{key, Comments, CommentsData};
    use crate::comments::map::CommentsMap;
    use crate::comments::node_key::NodeRefEqualityKey;
    use crate::comments::SourceComment;
    use insta::assert_snapshot;
    use ruff_formatter::SourceCode;
    use ruff_python_ast::node::AnyNode;
    use ruff_text_size::{TextRange, TextSize};
    use rustpython_parser::ast::{StmtBreak, StmtContinue};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn debug() {
        let continue_statement = AnyNode::from(StmtContinue {
            range: TextRange::default(),
        });

        let break_statement = AnyNode::from(StmtBreak {
            range: TextRange::default(),
        });

        let source = r#"# leading comment
continue; # trailing
# break leading
break;
"#;

        let source_code = SourceCode::new(source);

        let mut comments_map: CommentsMap<NodeRefEqualityKey, SourceComment> = CommentsMap::new();

        comments_map.push_leading(
            key(continue_statement.as_ref()),
            SourceComment {
                lines_before: 0,
                lines_after: 0,
                slice: source_code.slice(TextRange::at(TextSize::new(0), TextSize::new(17))),
                formatted: Cell::new(false),
            },
        );

        comments_map.push_trailing(
            key(continue_statement.as_ref()),
            SourceComment {
                lines_before: 0,
                lines_after: 0,
                slice: source_code.slice(TextRange::at(TextSize::new(28), TextSize::new(10))),
                formatted: Cell::new(false),
            },
        );

        comments_map.push_leading(
            key(break_statement.as_ref()),
            SourceComment {
                lines_before: 0,
                lines_after: 0,
                slice: source_code.slice(TextRange::at(TextSize::new(39), TextSize::new(15))),
                formatted: Cell::new(false),
            },
        );

        let comments = Comments {
            data: Rc::new(CommentsData {
                comments: comments_map,
            }),
        };

        let formatted = format!("{:#?}", comments.debug(source_code));

        assert_snapshot!(formatted);
    }
}
