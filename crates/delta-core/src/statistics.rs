//! Statistics computed by R (ADR-0054).
//!
//! R computes every figure. Rust passes the input file and the column name, and parses R's
//! answer. The R program is compiled into the binary, so its SHA-256 is fixed per build and every
//! result records it. R runs under a timeout and is killed when the timeout passes. R writes
//! `key: value` lines in base R only; each number is printed with `%.17g`, so the double reads
//! back exactly.

mod chart;
mod methods;
pub use chart::registration as chart_registration;
pub use methods::registrations as method_registrations;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};

/// The R release the statistics are computed with (ADR-0054, amended 2026-10-12). Any other R is
/// refused: the figures must come from the one environment that was validated.
pub const PINNED_R_VERSION: &str = "4.6.0";

/// The R program for `describe`. Its arguments are the CSV path and the column name.
const DESCRIBE_SCRIPT: &str = r#"
args <- commandArgs(trailingOnly = TRUE)
data <- read.csv(args[1], stringsAsFactors = FALSE, check.names = FALSE)
if (!(args[2] %in% names(data))) stop("column not found: ", args[2])
x <- suppressWarnings(as.numeric(data[[args[2]]]))
x <- x[!is.na(x)]
num <- function(value) sprintf("%.17g", value)
cat(sep = "",
  "n: ", num(length(x)), "\n",
  "mean: ", num(mean(x)), "\n",
  "sd: ", num(sd(x)), "\n",
  "min: ", num(min(x)), "\n",
  "max: ", num(max(x)), "\n",
  "median: ", num(median(x)), "\n",
  "r_version: ", paste(R.version$major, R.version$minor, sep = "."), "\n"
)
"#;

/// The `statistics.describe` capability: describe one column of a CSV file in the workspace.
/// R computes the figures; the model only receives them with their provenance. When R is not
/// installed the capability says so and computes nothing else (ADR-0054 point 7).
pub fn describe_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "statistics.describe".to_owned(),
        tool_name: "statistics_describe".to_owned(),
        side_effect: crate::skills::SideEffectClass::Read,
        description: "Describe one numeric column of a CSV file in the workspace: n, mean, sample \
            standard deviation, min, max and median. The figures are computed by R; the result \
            records the R version and the hashes of the program and the input."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["path", "column"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "column": {"type": "string", "description": "Column header to describe."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["statistics.describe"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let Some(path) = job.arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "path is required",
                    Some("arguments"),
                );
            };
            let Some(column) = job.arguments.get("column").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "column is required",
                    Some("arguments"),
                );
            };
            let resolved = match crate::capability::resolve_read_path(job, path) {
                Ok(resolved) => resolved,
                Err((code, message)) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some(code))
                }
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let rscript = match find_rscript() {
                Ok(rscript) => rscript,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("unavailable"));
                }
            };
            match describe(&rscript, &resolved, column, DESCRIBE_TIMEOUT) {
                Ok(result) => CapabilityResult::completed(&job.job_id, description_json(&result)),
                Err(error) => {
                    CapabilityResult::failed(&job.job_id, &error, Some("statistics_failed"))
                }
            }
        })),
    }
}

/// How long R may take to describe one column.
const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The Rscript on PATH, if it is the pinned release. Another release, or none, is an error that
/// says which version was found.
fn find_rscript() -> Result<PathBuf, String> {
    let output = Command::new("Rscript").arg("--version").output();
    let Ok(output) = output else {
        return Err("R (Rscript) is not installed, so statistics are unavailable".to_owned());
    };
    // Rscript prints its version on standard error.
    let text = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);
    check_pinned_version(&text)?;
    Ok(PathBuf::from("Rscript"))
}

/// Accept the output of `Rscript --version` only for the pinned release.
fn check_pinned_version(version_output: &str) -> Result<(), String> {
    let found = version_output
        .split_once("version ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .unwrap_or("unknown");
    if found == PINNED_R_VERSION {
        Ok(())
    } else {
        Err(format!(
            "statistics need R {PINNED_R_VERSION}, but R {found} was found, so nothing was computed"
        ))
    }
}

fn description_json(result: &Description) -> Value {
    json!({
        "n": result.n,
        "mean": result.mean,
        "sd": result.sd,
        "min": result.min,
        "max": result.max,
        "median": result.median,
        "provenance": {
            "engine": "R",
            "r_version": result.r_version,
            "script_sha256": result.script_sha256,
            "input_sha256": result.input_sha256,
            "method": "describe",
        },
    })
}

/// A descriptive summary of one column, with the provenance every result carries (ADR-0054 point 5).
#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    pub n: f64,
    pub mean: f64,
    pub sd: f64,
    pub min: f64,
    pub max: f64,
    pub median: f64,
    /// The R version that computed the figures, as R reports it.
    pub r_version: String,
    /// SHA-256 of the R program that ran.
    pub script_sha256: String,
    /// SHA-256 of the input file as it was read.
    pub input_sha256: String,
}

/// Describe `column` of the CSV at `input`, computed by the R at `rscript`, within `timeout`.
pub fn describe(
    rscript: &Path,
    input: &Path,
    column: &str,
    timeout: Duration,
) -> Result<Description, String> {
    let input_bytes =
        std::fs::read(input).map_err(|error| format!("could not read the input: {error}"))?;
    let output = run_rscript(
        rscript,
        DESCRIBE_SCRIPT,
        &[input.to_string_lossy().into_owned(), column.to_owned()],
        timeout,
    )?;
    let values = parse_dcf(&output)?;
    let number = |key: &str| -> Result<f64, String> {
        let text = values
            .get(key)
            .ok_or_else(|| format!("R did not report {key}"))?;
        text.parse::<f64>()
            .map_err(|_| format!("R reported an unreadable {key}: {text}"))
    };
    Ok(Description {
        n: number("n")?,
        mean: number("mean")?,
        sd: number("sd")?,
        min: number("min")?,
        max: number("max")?,
        median: number("median")?,
        r_version: values
            .get("r_version")
            .cloned()
            .ok_or_else(|| "R did not report its version".to_owned())?,
        script_sha256: sha256_hex(DESCRIBE_SCRIPT.as_bytes()),
        input_sha256: sha256_hex(&input_bytes),
    })
}

/// Run `script` with `args` under `Rscript` and return its standard output. A non-zero exit is an
/// error that carries R's standard error; a run past `timeout` is killed.
fn run_rscript(
    rscript: &Path,
    script: &str,
    args: &[String],
    timeout: Duration,
) -> Result<String, String> {
    let mut child = Command::new(rscript)
        .arg("--vanilla")
        .arg("-e")
        .arg(script)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start Rscript: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Rscript has no standard output".to_owned())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Rscript has no standard error".to_owned())?;
    let out_reader = thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).map(|_| text)
    });
    let err_reader = thread::spawn(move || {
        let mut text = String::new();
        stderr.read_to_string(&mut text).map(|_| text)
    });

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("could not wait for Rscript: {error}"))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("R did not finish within the time limit".to_owned());
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = out_reader
        .join()
        .map_err(|_| "could not read R's output".to_owned())?
        .map_err(|error| format!("could not read R's output: {error}"))?;
    let stderr = err_reader
        .join()
        .map_err(|_| "could not read R's errors".to_owned())?
        .unwrap_or_default();
    if !status.success() {
        return Err(format!("R failed: {}", stderr.trim()));
    }
    Ok(stdout)
}

/// Parse R's `key: value` lines. A line without `: ` is an error, so a malformed answer never
/// reads as a number.
fn parse_dcf(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (key, value) = line
            .split_once(": ")
            .ok_or_else(|| format!("unexpected line from R: {line}"))?;
        values.insert(key.to_owned(), value.to_owned());
    }
    Ok(values)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Rscript on PATH, if there is one. Tests that need R say so when it is missing.
    fn rscript_available() -> bool {
        Command::new("Rscript").arg("--version").output().is_ok()
    }

    fn write_csv(directory: &Path, text: &str) -> std::path::PathBuf {
        let path = directory.join("data.csv");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_describe_capability_is_a_read_only_tool_the_product_offers() {
        let registration = describe_registration();
        assert_eq!(registration.capability_id, "statistics.describe");
        assert_eq!(registration.metadata["category"], "read");
        assert_eq!(registration.metadata["requires_approval"], false);
        let host = crate::capability::CapabilityHost::product_defaults().unwrap();
        let names: Vec<String> = host
            .tool_schemas()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_owned))
            .collect();
        assert!(
            names.iter().any(|name| name == "statistics_describe"),
            "{names:?}"
        );
    }

    #[test]
    fn a_description_carries_its_provenance_and_its_figures() {
        let value = description_json(&Description {
            n: 5.0,
            mean: 4.0,
            sd: 3.5,
            min: 1.0,
            max: 10.0,
            median: 3.0,
            r_version: "4.3".to_owned(),
            script_sha256: "aa".to_owned(),
            input_sha256: "bb".to_owned(),
        });
        assert_eq!(value["mean"], 4.0);
        assert_eq!(value["provenance"]["engine"], "R");
        assert_eq!(value["provenance"]["script_sha256"], "aa");
        assert_eq!(value["provenance"]["input_sha256"], "bb");
    }

    #[test]
    fn only_the_pinned_r_release_is_accepted() {
        let pinned = "R scripting front-end version 4.6.0 (2026-04-01)";
        assert!(check_pinned_version(pinned).is_ok());
        let other = "R scripting front-end version 4.3.3 (2024-02-29)";
        let error = check_pinned_version(other).unwrap_err();
        assert!(
            error.contains("4.6.0") && error.contains("4.3.3"),
            "{error}"
        );
        assert!(check_pinned_version("no version here").is_err());
    }

    #[test]
    fn parse_dcf_reads_each_key_and_rejects_a_malformed_line() {
        let values = parse_dcf("n: 5\nmean: 4\n").unwrap();
        assert_eq!(values["n"], "5");
        assert_eq!(values["mean"], "4");
        assert!(parse_dcf("no separator here\n").is_err());
    }

    #[test]
    fn describe_matches_hand_computed_values_and_records_provenance() {
        if !rscript_available() {
            eprintln!("skipped: Rscript is not installed");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let input = write_csv(temp.path(), "x,label\n1,a\n2,b\n3,c\n4,d\n10,e\n");
        let result = describe(Path::new("Rscript"), &input, "x", Duration::from_secs(60)).unwrap();
        assert_eq!(result.n, 5.0);
        assert_eq!(result.mean, 4.0);
        assert_eq!(result.min, 1.0);
        assert_eq!(result.max, 10.0);
        assert_eq!(result.median, 3.0);
        // sample sd of 1, 2, 3, 4, 10 is sqrt(12.5)
        assert!((result.sd - 12.5_f64.sqrt()).abs() < 1e-12, "{}", result.sd);
        assert!(!result.r_version.is_empty());
        assert_eq!(result.script_sha256, sha256_hex(DESCRIBE_SCRIPT.as_bytes()));
        assert_eq!(
            result.input_sha256,
            sha256_hex(&std::fs::read(&input).unwrap())
        );
    }

    #[test]
    fn describe_leaves_out_values_that_are_not_numbers() {
        if !rscript_available() {
            eprintln!("skipped: Rscript is not installed");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let input = write_csv(temp.path(), "x\n1\n\nnot a number\n3\n");
        let result = describe(Path::new("Rscript"), &input, "x", Duration::from_secs(60)).unwrap();
        assert_eq!(result.n, 2.0);
        assert_eq!(result.mean, 2.0);
    }

    #[test]
    fn describe_reports_a_missing_column_as_an_error() {
        if !rscript_available() {
            eprintln!("skipped: Rscript is not installed");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let input = write_csv(temp.path(), "x\n1\n2\n");
        let error =
            describe(Path::new("Rscript"), &input, "y", Duration::from_secs(60)).unwrap_err();
        assert!(error.contains("column not found"), "{error}");
    }

    #[test]
    fn describe_stops_r_that_runs_past_the_time_limit() {
        if !rscript_available() {
            eprintln!("skipped: Rscript is not installed");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let input = write_csv(temp.path(), "x\n1\n");
        let error =
            describe(Path::new("Rscript"), &input, "x", Duration::from_millis(1)).unwrap_err();
        assert!(error.contains("time limit"), "{error}");
    }
}
