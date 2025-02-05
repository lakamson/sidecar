//! We want to talk to the LSP and get useful information out of this
//! This way we can talk to the LSP running in the editor from the sidecar
pub mod create_file;
pub mod diagnostics;
pub mod file_diagnostics;
pub mod get_outline_nodes;
pub(crate) mod go_to_previous_word;
pub mod gotodefintion;
pub mod gotoimplementations;
pub mod gotoreferences;
pub(crate) mod gototypedefinition;
pub mod grep_symbol;
pub mod inlay_hints;
pub mod open_file;
pub mod quick_fix;
pub(crate) mod undo_changes;
