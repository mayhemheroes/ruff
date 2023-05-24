use crate::comments::map::CommentsMap;
use crate::comments::node_key::NodeRefEqualityKey;
use crate::comments::placement::{CommentPlacement, CommentTextPosition};
use crate::comments::{Comments, SourceComment};
use ruff_formatter::SourceCode;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::prelude::Ranged;
use ruff_python_ast::source_code::CommentRanges;

/// Visitor extracting the comments from nodes.
#[derive(Debug, Clone)]
pub(crate) struct CommentsVisitor<'a> {
    builder: CommentsBuilder<'a>,
    parents: Vec<AnyNodeRef<'a>>,
    preceding_node: Option<AnyNodeRef<'a>>,
    following_node: Option<AnyNodeRef<'a>>,
    comment_ranges: &'a CommentRanges,
}

impl<'a> CommentsVisitor<'a> {
    pub(crate) fn new(source_code: SourceCode<'a>, comment_ranges: &'a CommentRanges) -> Self {
        Self {
            builder: CommentsBuilder::new(source_code),
            parents: Vec::new(),
            preceding_node: None,
            following_node: None,
            comment_ranges,
        }
    }

    pub(crate) fn start_node(&mut self, node: AnyNodeRef<'a>) {
        self.parents.push(node);
    }

    pub(crate) fn finish_node(&mut self, node: AnyNodeRef<'a>) {
        self.parents.pop();

        self.preceding_node = Some(node);
    }

    pub(crate) fn finish(self) -> Comments<'a> {
        Comments::new(self.builder.finish())
    }
}

#[derive(Clone, Debug)]
struct CommentsBuilder<'a> {
    source_code: SourceCode<'a>,
    comments: CommentsMap<NodeRefEqualityKey<'a>, SourceComment>,
}

impl<'a> CommentsBuilder<'a> {
    fn new(source_code: SourceCode<'a>) -> Self {
        Self {
            source_code,
            comments: CommentsMap::default(),
        }
    }

    fn add_comment(&mut self, placement: CommentPlacement<'a>) {
        match placement {
            CommentPlacement::Leading { node, comment } => {
                self.push_leading_comment(node, comment);
            }
            CommentPlacement::Trailing { node, comment } => {
                self.push_trailing_comment(node, comment);
            }
            CommentPlacement::Dangling { node, comment } => {
                self.push_dangling_comment(node, comment)
            }
            CommentPlacement::Default(mut comment) => {
                match comment.text_position() {
                    CommentTextPosition::EndOfLine => {
                        match (comment.preceding_node(), comment.following_node()) {
                            (Some(preceding), Some(_)) => {
                                // Attach comments with both preceding and following node to the preceding
                                // because there's a line break separating it from the following node.
                                // ```javascript
                                // a; // comment
                                // b
                                // ```
                                self.push_trailing_comment(preceding, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(preceding, comment);
                            }
                            (None, Some(following)) => {
                                self.push_leading_comment(following, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(comment.enclosing_node(), comment);
                            }
                        }
                    }
                    CommentTextPosition::OwnLine => {
                        match (comment.preceding_node(), comment.following_node()) {
                            // Following always wins for a leading comment
                            // ```javascript
                            // a;
                            // // comment
                            // b
                            // ```
                            // attach the comment to the `b` expression statement
                            (_, Some(following)) => {
                                self.push_leading_comment(following, comment);
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(preceding, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(comment.enclosing_node(), comment);
                            }
                        }
                    }
                    CommentTextPosition::SameLine => {
                        match (comment.preceding_node(), comment.following_node()) {
                            (Some(preceding), Some(following)) => {
                                // FIXME(micha): Conflict resolution

                                // Only make it a trailing comment if it directly follows the preceding node but not if it is separated
                                // by one or more tokens
                                // ```javascript
                                // a /* comment */ b;   //  Comment is a trailing comment
                                // a, /* comment */ b;  // Comment should be a leading comment
                                // ```
                                // if preceding.range().end()
                                //     == comment.piece().as_piece().token().text_range().end()
                                // {
                                //     self.push_trailing_comment(preceding, comment);
                                // } else {
                                self.push_leading_comment(following, comment);
                                // }
                            }
                            (Some(preceding), None) => {
                                self.push_trailing_comment(preceding, comment);
                            }
                            (None, Some(following)) => {
                                self.push_leading_comment(following, comment);
                            }
                            (None, None) => {
                                self.push_dangling_comment(comment.enclosing_node(), comment);
                            }
                        }
                    }
                }
            }
        }
    }

    fn push_leading_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments.push_leading(key(node), comment.into());
    }

    fn push_dangling_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments.push_dangling(key(node), comment.into());
    }

    fn push_trailing_comment(&mut self, node: AnyNodeRef<'a>, comment: impl Into<SourceComment>) {
        self.comments.push_trailing(key(node), comment.into());
    }

    fn finish(self) -> CommentsMap<NodeRefEqualityKey<'a>, SourceComment> {
        self.comments
    }
}

const fn key(node: AnyNodeRef) -> NodeRefEqualityKey {
    NodeRefEqualityKey::from_ref(node)
}
