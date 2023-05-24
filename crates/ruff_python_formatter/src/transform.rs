use crate::comments::{Comments, CommentsVisitor};
use ruff_formatter::SourceCode;
use ruff_python_ast::helpers::extract_handled_exceptions;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::prelude::*;
use ruff_python_ast::source_code::CommentRanges;
use ruff_python_ast::visitor::{
    walk_alias, walk_arg, walk_arguments, walk_body, walk_comprehension, walk_excepthandler,
    walk_expr, walk_keyword, walk_match_case, walk_pattern, walk_stmt, walk_withitem, Visitor,
};

pub(crate) fn transform<'a>(
    root: &'a Mod,
    source_code: SourceCode<'a>,
    comment_ranges: &'a CommentRanges,
) -> Comments<'a> {
    let mut visitor = TransformVisitor::new(source_code, comment_ranges);

    let root_ref = AnyNodeRef::from(root);
    visitor.start_node(root_ref);

    match root {
        Mod::Interactive(ModInteractive { body, .. }) | Mod::Module(ModModule { body, .. }) => {
            visitor.visit_body(&body);
        }
        Mod::Expression(expression) => visitor.visit_expr(&expression.body),
        Mod::FunctionType(ModFunctionType {
            argtypes, returns, ..
        }) => {
            for arg_type in argtypes {
                visitor.visit_expr(arg_type);
            }

            visitor.visit_expr(&returns);
        }
    }

    visitor.finish_node(root_ref);

    visitor.finish()
}

#[derive(Debug, Clone)]
struct TransformVisitor<'a> {
    comments: CommentsVisitor<'a>,
}

impl<'a> TransformVisitor<'a> {
    fn new(source_code: SourceCode<'a>, comment_ranges: &'a CommentRanges) -> Self {
        Self {
            comments: CommentsVisitor::new(source_code, comment_ranges),
        }
    }

    #[inline]
    fn start_node<T>(&mut self, node: T)
    where
        T: Into<AnyNodeRef<'a>>,
    {
        self.start_node_impl(node.into());
    }

    // Non generic version of `start_node` to avoid Rust unrolling all the code multiple times.
    fn start_node_impl(&mut self, node: AnyNodeRef<'a>) {
        self.comments.start_node(node);
    }

    fn finish_node<T>(&mut self, node: T)
    where
        T: Into<AnyNodeRef<'a>>,
    {
        self.finish_node_impl(node.into());
    }

    fn finish_node_impl(&mut self, node: AnyNodeRef<'a>) {
        self.comments.finish_node(node);
    }

    fn finish(self) -> Comments<'a> {
        self.comments.finish()
    }
}

impl<'ast> Visitor<'ast> for TransformVisitor<'ast> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        self.start_node(stmt);
        walk_stmt(self, stmt);
        self.finish_node(stmt);
    }

    fn visit_annotation(&mut self, expr: &'ast Expr) {
        self.start_node(expr);
        walk_expr(self, expr);
        self.finish_node(expr);
    }
    fn visit_expr(&mut self, expr: &'ast Expr) {
        self.start_node(expr);
        walk_expr(self, expr);
        self.finish_node(expr);
    }

    fn visit_comprehension(&mut self, comprehension: &'ast Comprehension) {
        self.start_node(comprehension);
        walk_comprehension(self, comprehension);
        self.finish_node(comprehension);
    }

    fn visit_excepthandler(&mut self, excepthandler: &'ast Excepthandler) {
        self.start_node(excepthandler);
        walk_excepthandler(self, excepthandler);
        self.finish_node(excepthandler);
    }

    fn visit_format_spec(&mut self, format_spec: &'ast Expr) {
        self.start_node(format_spec);
        walk_expr(self, format_spec);
        self.finish_node(format_spec);
    }

    fn visit_arguments(&mut self, arguments: &'ast Arguments) {
        self.start_node(arguments);
        walk_arguments(self, arguments);
        self.finish_node(arguments);
    }

    fn visit_arg(&mut self, arg: &'ast Arg) {
        self.start_node(arg);
        walk_arg(self, arg);
        self.finish_node(arg);
    }

    fn visit_keyword(&mut self, keyword: &'ast Keyword) {
        self.start_node(keyword);
        walk_keyword(self, keyword);
        self.finish_node(keyword);
    }

    fn visit_alias(&mut self, alias: &'ast Alias) {
        self.start_node(alias);
        walk_alias(self, alias);
        self.finish_node(alias);
    }

    fn visit_withitem(&mut self, withitem: &'ast Withitem) {
        self.start_node(withitem);
        walk_withitem(self, withitem);
        self.finish_node(withitem);
    }
    fn visit_match_case(&mut self, match_case: &'ast MatchCase) {
        self.start_node(match_case);
        walk_match_case(self, match_case);
        self.finish_node(match_case);
    }

    fn visit_pattern(&mut self, pattern: &'ast Pattern) {
        self.start_node(pattern);
        walk_pattern(self, pattern);
        self.finish_node(pattern);
    }
}

#[cfg(test)]
mod tests {
    use crate::transform::transform;
    use insta::{assert_debug_snapshot, assert_snapshot};
    use ruff_formatter::SourceCode;
    use ruff_python_ast::node::NodeKind;
    use ruff_python_ast::prelude::*;
    use ruff_python_ast::source_code::CommentRangesBuilder;
    use ruff_text_size::{TextLen, TextRange};
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_program, parse_tokens, Mode};

    #[test]
    fn test_tree() {
        let source = r#"
def test(x, y):
    if x == y:
        print("Equal")
    elif x < y:
        print("Less")
    else:
        print("Greater")

test(10, 20);
"#;
        let source_code = SourceCode::new(source);
        let tokens: Vec<_> = lex(source, Mode::Module).collect();

        let mut comment_ranges = CommentRangesBuilder::default();

        for (token, range) in tokens.iter().flatten() {
            comment_ranges.visit_token(&token, *range);
        }

        let comment_ranges = comment_ranges.finish();

        let parsed = parse_tokens(tokens.into_iter(), Mode::Module, "test.py")
            .expect("Expect source to be valid Python");

        let comments = transform(&parsed, source_code, &comment_ranges);

        assert_debug_snapshot!(comments.debug(source_code));
    }
}
