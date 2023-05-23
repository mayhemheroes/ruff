use std::path::Path;

use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::imports::{Alias, AnyImport, Import};
use ruff_python_semantic::binding::{
    Binding, BindingKind, FromImportation, Importation, SubmoduleImportation,
};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::isort::{categorize, ImportSection, ImportType};

/// ## What it does
/// Checks for first-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import A
///
///
/// def foo(a: A) -> int:
///     return len(a)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import A
///
///
/// def foo(a: A) -> int:
///     return len(a)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyFirstPartyImport {
    full_name: String,
}

impl Violation for TypingOnlyFirstPartyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move application import `{}` into a type-checking block",
            self.full_name
        )
    }
}

/// ## What it does
/// Checks for third-party imports that are only used for type annotations, but
/// aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// import pandas as pd
///
///
/// def foo(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     import pandas as pd
///
///
/// def foo(df: pd.DataFrame) -> int:
///     return len(df)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyThirdPartyImport {
    full_name: String,
}

impl Violation for TypingOnlyThirdPartyImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move third-party import `{}` into a type-checking block",
            self.full_name
        )
    }
}

/// ## What it does
/// Checks for standard library imports that are only used for type
/// annotations, but aren't defined in a type-checking block.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
/// from pathlib import Path
///
///
/// def foo(path: Path) -> str:
///     return str(path)
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     from pathlib import Path
///
///
/// def foo(path: Path) -> str:
///     return str(path)
/// ```
///
/// ## References
/// - [PEP 536](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct TypingOnlyStandardLibraryImport {
    full_name: String,
}

impl Violation for TypingOnlyStandardLibraryImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Move standard library import `{}` into a type-checking block",
            self.full_name
        )
    }
}

/// Return `true` if `this` is implicitly loaded via importing `that`.
fn is_implicit_import(this: &Binding, that: &Binding) -> bool {
    match &this.kind {
        BindingKind::Importation(Importation {
            full_name: this_name,
            ..
        })
        | BindingKind::SubmoduleImportation(SubmoduleImportation {
            name: this_name, ..
        }) => match &that.kind {
            BindingKind::FromImportation(FromImportation {
                full_name: that_name,
                ..
            }) => {
                // Ex) `pkg.A` vs. `pkg`
                this_name
                    .rfind('.')
                    .map_or(false, |i| this_name[..i] == *that_name)
            }
            BindingKind::Importation(Importation {
                full_name: that_name,
                ..
            })
            | BindingKind::SubmoduleImportation(SubmoduleImportation {
                name: that_name, ..
            }) => {
                // Ex) `pkg.A` vs. `pkg.B`
                this_name == that_name
            }
            _ => false,
        },
        BindingKind::FromImportation(FromImportation {
            full_name: this_name,
            ..
        }) => match &that.kind {
            BindingKind::Importation(Importation {
                full_name: that_name,
                ..
            })
            | BindingKind::SubmoduleImportation(SubmoduleImportation {
                name: that_name, ..
            }) => {
                // Ex) `pkg.A` vs. `pkg`
                this_name
                    .rfind('.')
                    .map_or(false, |i| &this_name[..i] == *that_name)
            }
            BindingKind::FromImportation(FromImportation {
                full_name: that_name,
                ..
            }) => {
                // Ex) `pkg.A` vs. `pkg.B`
                this_name.rfind('.').map_or(false, |i| {
                    that_name
                        .rfind('.')
                        .map_or(false, |j| this_name[..i] == that_name[..j])
                })
            }
            _ => false,
        },
        _ => false,
    }
}

/// Return `true` if `name` is exempt from typing-only enforcement.
fn is_exempt(name: &str, exempt_modules: &[&str]) -> bool {
    let mut name = name;
    loop {
        if exempt_modules.contains(&name) {
            return true;
        }
        match name.rfind('.') {
            Some(idx) => {
                name = &name[..idx];
            }
            None => return false,
        }
    }
}

/// TCH001
pub(crate) fn typing_only_runtime_import(
    checker: &Checker,
    binding: &Binding,
    runtime_imports: &[&Binding],
    package: Option<&Path>,
) -> Option<Diagnostic> {
    // If we're in un-strict mode, don't flag typing-only imports that are
    // implicitly loaded by way of a valid runtime import.
    if !checker.settings.flake8_type_checking.strict
        && runtime_imports
            .iter()
            .any(|import| is_implicit_import(binding, import))
    {
        return None;
    }

    let full_name = match &binding.kind {
        BindingKind::Importation(Importation { full_name, .. }) => full_name,
        BindingKind::FromImportation(FromImportation { full_name, .. }) => full_name.as_str(),
        BindingKind::SubmoduleImportation(SubmoduleImportation { full_name, .. }) => full_name,
        _ => return None,
    };

    if is_exempt(
        full_name,
        &checker
            .settings
            .flake8_type_checking
            .exempt_modules
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }

    let Some(reference_id) = binding.references.first() else {
        return None;
    };

    if binding.context.is_runtime()
        && binding.references.iter().all(|reference_id| {
            checker
                .semantic_model()
                .references
                .resolve(*reference_id)
                .context()
                .is_typing()
        })
    {
        // Extract the module base and level from the full name.
        // Ex) `foo.bar.baz` -> `foo`, `0`
        // Ex) `.foo.bar.baz` -> `foo`, `1`
        let level = full_name
            .chars()
            .take_while(|c| *c == '.')
            .count()
            .try_into()
            .unwrap();

        // Categorize the import.
        let mut diagnostic = match categorize(
            full_name,
            Some(level),
            &checker.settings.src,
            package,
            &checker.settings.isort.known_modules,
            checker.settings.target_version,
        ) {
            ImportSection::Known(ImportType::LocalFolder | ImportType::FirstParty) => {
                Diagnostic::new(
                    TypingOnlyFirstPartyImport {
                        full_name: full_name.to_string(),
                    },
                    binding.range,
                )
            }
            ImportSection::Known(ImportType::ThirdParty) | ImportSection::UserDefined(_) => {
                Diagnostic::new(
                    TypingOnlyThirdPartyImport {
                        full_name: full_name.to_string(),
                    },
                    binding.range,
                )
            }
            ImportSection::Known(ImportType::StandardLibrary) => Diagnostic::new(
                TypingOnlyStandardLibraryImport {
                    full_name: full_name.to_string(),
                },
                binding.range,
            ),
            ImportSection::Known(ImportType::Future) => {
                unreachable!("`__future__` imports should be marked as used")
            }
        };

        if checker.settings.rules.enabled(diagnostic.kind.rule()) {
            if checker.patch(diagnostic.kind.rule()) {
                // What does it take to autofix this?
                // First, we need to remove the import.
                // Second, we need to add a type-checking block.
                // Third, we need to add the import within the type-checking block.

                diagnostic.try_set_fix(|| {
                    // Step 1) Remove the import.
                    let source = binding.source.unwrap();
                    let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
                    let stmt = checker.semantic_model().stmts[source];
                    let parent = checker
                        .semantic_model()
                        .stmts
                        .parent_id(source)
                        .map(|id| checker.semantic_model().stmts[id]);
                    let remove_import_edit = autofix::actions::remove_unused_imports(
                        std::iter::once(full_name),
                        stmt,
                        parent,
                        &deleted,
                        checker.locator,
                        checker.indexer,
                        checker.stylist,
                    )?;

                    // I think we should format the import here, by taking `stmt`, and removing
                    // every member that isn't `full_name`, then generating that string.

                    // Step 2) Add the import within the type-checking block.
                    let reference = checker.semantic_model().references.resolve(*reference_id);
                    let (add_type_checking_edit, add_import_edit) =
                        checker.importer.add_typing_import(
                            // Create the `AnyImport`.
                            &any_import(binding).unwrap(),
                            reference.range().start(),
                            checker.semantic_model(),
                        )?;

                    // TODO(charlie): Sort these. They can be in ~any order.
                    Ok(Fix::suggested_edits(
                        remove_import_edit,
                        [add_type_checking_edit, add_import_edit],
                    ))
                });
            }

            Some(diagnostic)
        } else {
            None
        }
    } else {
        None
    }
}

/// Convert a [`Binding`] into an [`AnyImport`].
fn any_import<'a>(binding: &Binding<'a>) -> Option<AnyImport<'a>> {
    match &binding.kind {
        BindingKind::Importation(import) => {
            // TODO(charlie): This is incorrect for explicit re-exports.
            if import.name == import.full_name {
                Some(AnyImport::Import(Import {
                    name: Alias {
                        name: import.full_name,
                        as_name: None,
                    },
                }))
            } else {
                Some(AnyImport::Import(Import {
                    name: Alias {
                        name: import.full_name,
                        as_name: Some(import.name),
                    },
                }))
            }
        }
        BindingKind::FromImportation(import) => {
            todo!()
            // let level = import.full_name.chars().take_while(|c| *c == '.').count();
            // let module = &import.full_name[level..];
            // Some(AnyImport::ImportFrom(ImportFrom {
            //     module: None,
            //     name: Alias { name: "", as_name: None },
            //     level: Some(level as u32),
            // }))
        }
        BindingKind::SubmoduleImportation(import) => Some(AnyImport::Import(Import {
            name: Alias {
                name: import.full_name,
                as_name: None,
            },
        })),
        _ => return None,
    }
}
