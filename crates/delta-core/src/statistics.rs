//! Statistics computed by R (ADR-0054).
//!
//! R computes every figure. Rust passes the input file and the column name, and parses R's
//! answer. The R program is compiled into the binary, so its SHA-256 is fixed per build and every
//! result records it. R runs under a timeout and is killed when the timeout passes. R writes
//! `key: value` lines in base R only; each number is printed with `%.17g`, so the double reads
//! back exactly.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

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
