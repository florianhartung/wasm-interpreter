use std::{collections::BTreeMap, fmt::Display, iter};

use anyhow::{anyhow, Context};
use itertools::Itertools;

use crate::{
    ci_reports::{
        CIFullReport,
        CIReportData::{self, Assert, ScriptError},
    },
    sanitize_path,
};

pub fn generate<'a>(
    old_report: &'a CIFullReport,
    new_report: &'a CIFullReport,
) -> anyhow::Result<FileDeltas<'a>> {
    let report_to_map = |report: &'a CIFullReport| -> BTreeMap<&'a str, &'a CIReportData> {
        report
            .entries
            .iter()
            .map(|report| (&*report.filepath, &report.data))
            .collect()
    };
    let old_entries = report_to_map(old_report);
    let new_entries = report_to_map(new_report);

    // Deduplicate all reports by their filenames
    let mut sorted_old_filepaths = old_entries.keys().peekable();
    let mut sorted_new_filepaths = new_entries.keys().peekable();
    let all_deduplicated_filepaths =
        iter::from_fn(
            || match (sorted_old_filepaths.peek(), sorted_new_filepaths.peek()) {
                (None, None) => None,
                (Some(_), None) => sorted_old_filepaths.next(),
                (None, Some(_)) => sorted_new_filepaths.next(),
                (Some(old_filepath), Some(new_filepath)) => {
                    if old_filepath <= new_filepath {
                        sorted_old_filepaths.next()
                    } else {
                        sorted_new_filepaths.next()
                    }
                }
            },
        )
        // Dedup removes consecutive elements, which works because this iterator is sorted
        .dedup();

    let deltas = all_deduplicated_filepaths
        .flat_map(|filepath| {
            let old_entry = old_entries.get(filepath);
            let new_entry = new_entries.get(filepath);

            let delta_type = match (old_entry, new_entry) {
                (None, None) => unreachable!("filepath must exist in at least one report"),
                (Some(_old), None) => FileDeltaType::FileDeleted {},
                (None, Some(_new)) => FileDeltaType::FileAdded {},
                (Some(ScriptError { .. }), Some(ScriptError { .. })) => return None,
                (Some(Assert { .. }), Some(ScriptError { .. })) => FileDeltaType::NewScriptError,
                (Some(ScriptError { .. }), Some(Assert { .. })) => {
                    FileDeltaType::ScriptErrorResolved
                }
                (Some(Assert { results: old }), Some(Assert { results: new })) => {
                    // Now we want pairs of old and new asserts each
                    if old.len() != new.len() {
                        return Some(Err(anyhow!("reports for {filepath} contains different amounts of asserts. make sure the same testsuite is used for both runs")));
                    }

                    struct OldAndNewAssert<'report> {
                        line_number: u32,
                        command: &'report str,
                        old_error: Option<&'report String>,
                        new_error: Option<&'report String>,
                    }
                    let pairs = old.iter().map(|old_assert| {
                        new.iter()
                            .find(|new_assert|
                                new_assert.line_number == old_assert.line_number
                                    && new_assert.command == old_assert.command)
                            .map(|new_assert| OldAndNewAssert {
                                line_number: old_assert.line_number,
                                command: &old_assert.command,
                                old_error: old_assert.error.as_ref(),
                                new_error: new_assert.error.as_ref(),
                            })
                            .with_context(|| format!("no assert for the new test report was found for {old_assert:?} in file {filepath}"))
                    });

                    let mut now_passing = Vec::new();
                    let mut now_failing = Vec::new();
                    let mut now_with_different_error_message = Vec::new();

                    for old_and_new in pairs {
                        let old_and_new = match old_and_new {
                            Ok(p) => p,
                            Err(err) => return Some(Err(err)),
                        };

                        match old_and_new {
                            OldAndNewAssert {
                                line_number,
                                command,
                                old_error: Some(old_error),
                                new_error: None,
                            } => now_passing.push(AssertDifference { line_number, command, error: old_error }),
                            OldAndNewAssert {
                                line_number,
                                command,
                                old_error: None,
                                new_error: Some(new_error),
                            } => {
                                now_failing.push(AssertDifference {
                                    line_number,
                                    command,
                                    error: new_error,
                                });
                            },
                            OldAndNewAssert {
                                line_number,
                                command,
                                old_error: Some(old_error),
                                new_error: Some(new_error),
                            } if old_error != new_error => {
                                if old_error != new_error {
                                    now_with_different_error_message.push(AssertDifferenceChangedErrorMsg {
                                        line_number,
                                        command,
                                        old_error,
                                        new_error,
                                    })
                                }
                            }
                            // All other cases do not produce differences
                            _ => {},
                        }
                    }

                    // If no differences were found, do not emit a delta for this file
                    if now_passing.is_empty() && now_failing.is_empty() && now_with_different_error_message.is_empty(){
                        return None;
                    }

                    FileDeltaType::AssertsChanged { now_passing, now_failing, now_with_different_error_message }
                }
            };

            Some(Ok(FileDelta {
                filename: filepath,
                ty: delta_type,
            }))
        })
        .collect::<anyhow::Result<Vec<FileDelta>>>()?;

    Ok(FileDeltas(deltas))
}

pub struct FileDeltas<'report>(Vec<FileDelta<'report>>);
impl Display for FileDeltas<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            return writeln!(f, "<b> No changes detected </b>");
        }

        write!(
            f,
            r#"\
            ## PR delta:

            | **File** | **Notes** | ❓ |
            |:--------:|:---------:|:--:|
            "#
        )?;
        self.0.iter().try_for_each(|delta| writeln!(f, "{delta}"))
    }
}

struct FileDelta<'report> {
    filename: &'report str,
    ty: FileDeltaType<'report>,
}

enum FileDeltaType<'report> {
    FileAdded,
    FileDeleted,
    NewScriptError,
    ScriptErrorResolved,
    AssertsChanged {
        now_passing: Vec<AssertDifference<'report>>,
        now_failing: Vec<AssertDifference<'report>>,
        now_with_different_error_message: Vec<AssertDifferenceChangedErrorMsg<'report>>,
    },
}

struct AssertDifference<'report> {
    line_number: u32,
    command: &'report str,
    error: &'report str,
}

struct AssertDifferenceChangedErrorMsg<'report> {
    line_number: u32,
    command: &'report str,
    old_error: &'report str,
    new_error: &'report str,
}

impl Display for FileDelta<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "| {} | ", sanitize_path(self.filename))?;

        match &self.ty {
            FileDeltaType::FileAdded => writeln!(f, "File missing in target branch | ⚠ |"),
            FileDeltaType::FileDeleted => writeln!(f, "File missing in this PR | ⚠ |"),
            FileDeltaType::NewScriptError => writeln!(f, "File no longer compiles | ❌ |"),
            FileDeltaType::ScriptErrorResolved => writeln!(f, "File now compiles | ✅ |"),
            FileDeltaType::AssertsChanged {
                now_passing,
                now_failing,
                now_with_different_error_message,
            } => {
                let icon = match (
                    !now_passing.is_empty(),
                    !now_failing.is_empty(),
                    !now_with_different_error_message.is_empty(),
                ) {
                    (_, _, true) | (true, true, false) => "⚠",
                    (true, false, false) => "✅",
                    (false, true, false) => "❌",
                    (false, false, false) => {
                        unreachable!("a file delta should always contain some differences")
                    }
                };

                if !now_passing.is_empty() {
                    write!(f, "+{} passing<br>", now_passing.len())?;
                }

                if !now_failing.is_empty() {
                    write!(f, "-{} failing<br>", now_failing.len())?;
                }

                if !now_with_different_error_message.is_empty() {
                    write!(
                        f,
                        "{} with changed errors ",
                        now_with_different_error_message.len()
                    )?;
                }

                writeln!(f, "| {icon} |")
            }
        }
    }
}
