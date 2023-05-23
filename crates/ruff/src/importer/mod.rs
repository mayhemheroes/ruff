//! Add and modify import statements to make module members available during fix execution.

use anyhow::{bail, Result};
use libcst_native::{Codegen, CodegenState, ImportAlias, Name, NameOrAttribute};
use ruff_text_size::TextSize;
use rustpython_parser::ast::{self, Ranged, Stmt, StmtIf, Suite};

use ruff_diagnostics::Edit;
use ruff_python_ast::imports::{AnyImport, Import};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_semantic::analyze::typing::is_type_checking_block;
use ruff_python_semantic::model::SemanticModel;

use crate::cst::matchers::{match_aliases, match_import_from, match_statement};
use crate::importer::insertion::Insertion;

mod insertion;

#[allow(dead_code)]
struct TypeCheckingBlock<'a> {
    /// The [`StmtIf`] that represents the `if TYPE_CHECKING:` block.
    stmt: &'a StmtIf,
    /// The list of imports in the `if TYPE_CHECKING:` block.
    imports: Vec<&'a Stmt>,
}

pub(crate) struct Importer<'a> {
    /// The Python AST to which we are adding imports.
    python_ast: &'a Suite,
    /// The [`Locator`] for the Python AST.
    locator: &'a Locator<'a>,
    /// The [`Stylist`] for the Python AST.
    stylist: &'a Stylist<'a>,
    /// The list of visited, top-level runtime imports in the Python AST.
    runtime_imports: Vec<&'a Stmt>,
    /// The list of visited, top-level `if TYPE_BLOCKING:` blocks in the Python AST.
    typing_imports: Vec<TypeCheckingBlock<'a>>,
    /// The current stack of `if TYPE_BLOCKING:` blocks in the Python AST that are being visited,
    /// to support nested `if TYPE_BLOCKING:` blocks at the top level.
    blocks: Vec<TypeCheckingBlock<'a>>,
    /// The current depth of the visitor.
    depth: u32,
}

impl<'a> Importer<'a> {
    pub(crate) fn new(
        python_ast: &'a Suite,
        locator: &'a Locator<'a>,
        stylist: &'a Stylist<'a>,
    ) -> Self {
        Self {
            python_ast,
            locator,
            stylist,
            runtime_imports: Vec::default(),
            typing_imports: Vec::default(),
            blocks: Vec::default(),
            depth: 0,
        }
    }

    /// Enter a [`Stmt`].
    pub(crate) fn enter(&mut self, stmt: &'a Stmt, semantic_model: &SemanticModel) {
        match stmt {
            // Add import.
            Stmt::Import(_) | Stmt::ImportFrom(_) if self.depth == 0 => {
                if let Some(block) = self.blocks.last_mut() {
                    block.imports.push(stmt);
                } else {
                    self.runtime_imports.push(stmt);
                }
            }
            // Push `if TYPE_CHECKING:` block.
            Stmt::If(stmt)
                if self.depth == 0 && is_type_checking_block(semantic_model, &stmt.test) =>
            {
                self.blocks.push(TypeCheckingBlock {
                    stmt,
                    imports: Vec::default(),
                });
            }
            // Increment depth.
            _ => self.depth += 1,
        }
    }

    /// Leave a [`Stmt`]. It's assumed that the given [`Stmt`] was previously entered.
    pub(crate) fn leave(&mut self, stmt: &'a Stmt, semantic_model: &SemanticModel) {
        match stmt {
            // Nothing to do.
            Stmt::Import(_) | Stmt::ImportFrom(_) if self.depth == 0 => {}
            // Pop `if TYPE_CHECKING:` block.
            Stmt::If(stmt)
                if self.depth == 0 && is_type_checking_block(semantic_model, &stmt.test) =>
            {
                if let Some(block) = self.blocks.pop() {
                    self.typing_imports.push(block);
                }
            }
            // Decrement depth.
            _ => self.depth -= 1,
        }
    }

    /// Add an import statement to import the given module.
    ///
    /// If there are no existing imports, the new import will be added at the top
    /// of the file. Otherwise, it will be added after the most recent top-level
    /// import statement.
    pub(crate) fn add_import(&self, import: &AnyImport, at: TextSize) -> Edit {
        let required_import = import.to_string();
        if let Some(stmt) = self.preceding_import(&self.runtime_imports, at) {
            // Insert after the last top-level import.
            Insertion::end_of_statement(stmt, self.locator, self.stylist)
                .into_edit(&required_import)
        } else {
            // Insert at the top of the file.
            Insertion::top_of_file(self.python_ast, self.locator, self.stylist)
                .into_edit(&required_import)
        }
    }

    pub(crate) fn add_typing_import(
        &self,
        // TODO(charlie): This won't preserve comments or anything like that. It'll also lose
        // information like explicit re-exports.
        import: &AnyImport,
        at: TextSize,
        semantic_model: &SemanticModel,
    ) -> Result<(Edit, Edit)> {
        // Grab the `TYPE_CHECKING` symbol from `typing`.
        // TODO(charlie): This isn't quite right, we could end up importing the symbol too late.
        let (type_checking_edit, type_checking) =
            self.get_or_import_symbol("typing", "TYPE_CHECKING", at, semantic_model)?;

        // Add the import.
        let required_import = import.to_string();
        let add_import_edit = if let Some(block) = self.preceding_block(at) {
            // Insert at the top of the block.
            // TODO(charlie): We should probably do this with LibCST.
            Insertion::top_of_block(&block.stmt.body, self.locator, self.stylist)
                .into_edit(&required_import)
        } else {
            // Insert a new block after the last top-level import.
            // TODO(charlie): I think we should do this with LibCST...
            let block = format!(
                "if {}:{}{}{}",
                type_checking,
                self.stylist.line_ending().as_str(),
                self.stylist.indentation().as_str(),
                required_import
            );

            // In this case, these both need to be at the start of a new line.
            if let Some(stmt) = self.preceding_import(&self.runtime_imports, at) {
                // Insert after the last import in the block.
                // TODO(charlie): This will break if the statement is a multi-line statement
                // (semicolon-delimited).
                Insertion::end_of_statement(stmt, self.locator, self.stylist).into_edit(&block)
            } else {
                // Insert at the top of the block.
                // TODO(charlie): This is probably wrong too.
                Insertion::top_of_file(&self.python_ast, self.locator, self.stylist)
                    .into_edit(&block)
            }
        };

        Ok((type_checking_edit, add_import_edit))
    }

    /// Generate an [`Edit`] to reference the given symbol. Returns the [`Edit`] necessary to make
    /// the symbol available in the current scope along with the bound name of the symbol.
    ///
    /// Attempts to reuse existing imports when possible.
    pub(crate) fn get_or_import_symbol(
        &self,
        module: &str,
        member: &str,
        at: TextSize,
        semantic_model: &SemanticModel,
    ) -> Result<(Edit, String)> {
        self.get_symbol(module, member, at, semantic_model)?
            .map_or_else(
                || self.import_symbol(module, member, at, semantic_model),
                Ok,
            )
    }

    /// Return an [`Edit`] to reference an existing symbol, if it's present in the given [`SemanticModel`].
    fn get_symbol(
        &self,
        module: &str,
        member: &str,
        at: TextSize,
        semantic_model: &SemanticModel,
    ) -> Result<Option<(Edit, String)>> {
        // If the symbol is already available in the current scope, use it.
        let Some((source, binding)) = semantic_model.resolve_qualified_import_name(module, member) else {
            return Ok(None);
        };

        // The exception: the symbol source (i.e., the import statement) comes after the current
        // location. For example, we could be generating an edit within a function, and the import
        // could be defined in the module scope, but after the function definition. In this case,
        // it's unclear whether we can use the symbol (the function could be called between the
        // import and the current location, and thus the symbol would not be available). It's also
        // unclear whether should add an import statement at the top of the file, since it could
        // be shadowed between the import and the current location.
        if source.start() > at {
            bail!("Unable to use existing symbol `{binding}` due to late-import");
        }

        // We also add a no-op edit to force conflicts with any other fixes that might try to
        // remove the import. Consider:
        //
        // ```py
        // import sys
        //
        // quit()
        // ```
        //
        // Assume you omit this no-op edit. If you run Ruff with `unused-imports` and
        // `sys-exit-alias` over this snippet, it will generate two fixes: (1) remove the unused
        // `sys` import; and (2) replace `quit()` with `sys.exit()`, under the assumption that `sys`
        // is already imported and available.
        //
        // By adding this no-op edit, we force the `unused-imports` fix to conflict with the
        // `sys-exit-alias` fix, and thus will avoid applying both fixes in the same pass.
        let import_edit = Edit::range_replacement(
            self.locator.slice(source.range()).to_string(),
            source.range(),
        );
        Ok(Some((import_edit, binding)))
    }

    /// Generate an [`Edit`] to reference the given symbol. Returns the [`Edit`] necessary to make
    /// the symbol available in the current scope along with the bound name of the symbol.
    ///
    /// For example, assuming `module` is `"functools"` and `member` is `"lru_cache"`, this function
    /// could return an [`Edit`] to add `import functools` to the top of the file, alongside with
    /// the name on which the `lru_cache` symbol would be made available (`"functools.lru_cache"`).
    fn import_symbol(
        &self,
        module: &str,
        member: &str,
        at: TextSize,
        semantic_model: &SemanticModel,
    ) -> Result<(Edit, String)> {
        if let Some(stmt) = self.find_import_from(module, at) {
            // Case 1: `from functools import lru_cache` is in scope, and we're trying to reference
            // `functools.cache`; thus, we add `cache` to the import, and return `"cache"` as the
            // bound name.
            if semantic_model
                .find_binding(member)
                .map_or(true, |binding| binding.kind.is_builtin())
            {
                let import_edit = self.add_member(stmt, member)?;
                Ok((import_edit, member.to_string()))
            } else {
                bail!("Unable to insert `{member}` into scope due to name conflict")
            }
        } else {
            // Case 2: No `functools` import is in scope; thus, we add `import functools`, and
            // return `"functools.cache"` as the bound name.
            if semantic_model
                .find_binding(module)
                .map_or(true, |binding| binding.kind.is_builtin())
            {
                let import_edit = self.add_import(&AnyImport::Import(Import::module(module)), at);
                Ok((import_edit, format!("{module}.{member}")))
            } else {
                bail!("Unable to insert `{module}` into scope due to name conflict")
            }
        }
    }

    /// Return the top-level [`Stmt`] that imports the given module using `Stmt::ImportFrom`
    /// preceding the given position, if any.
    fn find_import_from(&self, module: &str, at: TextSize) -> Option<&Stmt> {
        let mut import_from = None;
        for stmt in &self.runtime_imports {
            if stmt.start() >= at {
                break;
            }
            if let Stmt::ImportFrom(ast::StmtImportFrom {
                module: name,
                level,
                ..
            }) = stmt
            {
                if level.map_or(true, |level| level.to_u32() == 0)
                    && name.as_ref().map_or(false, |name| name == module)
                {
                    import_from = Some(*stmt);
                }
            }
        }
        import_from
    }

    /// Add the given member to an existing `Stmt::ImportFrom` statement.
    fn add_member(&self, stmt: &Stmt, member: &str) -> Result<Edit> {
        let mut statement = match_statement(self.locator.slice(stmt.range()))?;
        let import_from = match_import_from(&mut statement)?;
        let aliases = match_aliases(import_from)?;
        aliases.push(ImportAlias {
            name: NameOrAttribute::N(Box::new(Name {
                value: member,
                lpar: vec![],
                rpar: vec![],
            })),
            asname: None,
            comma: aliases.last().and_then(|alias| alias.comma.clone()),
        });
        let mut state = CodegenState {
            default_newline: &self.stylist.line_ending(),
            default_indent: self.stylist.indentation(),
            ..CodegenState::default()
        };
        statement.codegen(&mut state);
        Ok(Edit::range_replacement(state.to_string(), stmt.range()))
    }

    /// Return the import statement that precedes the given position, if any.
    fn preceding_import(&self, imports: &[&'a Stmt], at: TextSize) -> Option<&'a Stmt> {
        imports
            .partition_point(|stmt| stmt.start() < at)
            .checked_sub(1)
            .map(|idx| imports[idx])
    }

    /// Return the import statement that precedes the given position, if any.
    fn preceding_block(&self, at: TextSize) -> Option<&TypeCheckingBlock> {
        self.blocks
            .partition_point(|block| block.stmt.start() < at)
            .checked_sub(1)
            .map(|idx| &self.blocks[idx])
    }
}
