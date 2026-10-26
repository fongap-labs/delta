//! `chart_render`: draw a chart from a CSV file with R's base graphics (ADR-0070).
//!
//! The same pinned R that computes the reported numbers draws the figure, so no second runtime is
//! needed. R writes one PNG into the artifact staging directory; Rust hashes it and writes a
//! `delta.figure` record next to it, so the figure can be traced to its input, its R version and
//! the program that drew it. Column names and the title reach R as command-line arguments, never
//! as program text.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::{find_rscript, parse_dcf, run_rscript, sha256_hex};
use crate::capability::{
    resolve_read_path, CapabilityArtifact, CapabilityGrants, CapabilityRegistration,
    CapabilityResult, NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

/// How long R may take to draw one chart.
const CHART_TIMEOUT: Duration = Duration::from_secs(60);
/// Most bins a histogram may ask for.
const MAX_BINS: i64 = 200;
/// Longest title accepted, in characters.
const MAX_TITLE_CHARS: usize = 120;

/// The R program that draws the chart. Arguments: kind, input CSV, output PNG, column, group,
/// x, y, fit (TRUE/FALSE), bins (0 for automatic), title. Missing arguments are empty strings.
const CHART_SCRIPT: &str = r##"
args <- commandArgs(trailingOnly = TRUE)
kind <- args[1]; input <- args[2]; output <- args[3]
column <- args[4]; group <- args[5]; xname <- args[6]; yname <- args[7]
fit <- args[8] == "TRUE"; bins <- as.integer(args[9]); title <- args[10]
data <- read.csv(input, stringsAsFactors = FALSE, check.names = FALSE)
need <- function(name) {
  if (!(name %in% names(data))) stop("column not found: ", name)
  data[[name]]
}
numeric_column <- function(name) suppressWarnings(as.numeric(need(name)))
keep <- function(...) {
  columns <- list(...)
  ok <- Reduce(`&`, lapply(columns, function(v) !is.na(v)))
  lapply(columns, function(v) v[ok])
}
points <- 0L; dropped <- 0L
device <- if (capabilities("cairo")) "cairo" else if (.Platform$OS.type == "windows") "windows" else NA
if (is.na(device)) stop("this R has no PNG device without X11; build R with cairo support")
png(output, width = 1200, height = 800, res = 150, type = device)
if (kind == "histogram") {
  x <- numeric_column(column)
  kept <- keep(x)
  x <- kept[[1]]
  if (length(x) < 2) stop("the column has fewer than two numbers")
  breaks <- if (bins > 0) bins else "Sturges"
  hist(x, breaks = breaks, main = title, xlab = column, ylab = "count",
       col = "#3b82f6", border = "white")
  points <- length(x)
} else if (kind == "boxplot" || kind == "bar") {
  value <- numeric_column(column)
  g <- need(group)
  kept <- keep(value, g)
  value <- kept[[1]]; g <- factor(kept[[2]])
  if (length(levels(g)) < 1) stop("the group column has no values")
  if (length(levels(g)) > 20) stop("more than 20 groups; choose a column with fewer values")
  if (kind == "boxplot") {
    boxplot(value ~ g, main = title, xlab = group, ylab = column,
            col = "#93c5fd", border = "#1e3a8a", las = 2)
  } else {
    means <- tapply(value, g, mean)
    barplot(means, main = title, xlab = group, ylab = paste("mean of", column),
            col = "#f59e0b", border = NA, las = 2)
  }
  points <- length(value)
} else if (kind == "scatter" || kind == "line") {
  if (kind == "line") {
    xraw <- need(xname)
    as_date <- suppressWarnings(as.Date(xraw))
    non_missing <- !is.na(xraw)
    parsed <- if (is.numeric(xraw)) NULL else tryCatch(as.Date(xraw), error = function(e) NULL)
    is_date <- !is.null(parsed) && any(non_missing) && all(!is.na(parsed[non_missing]))
    xv <- if (is_date) as.numeric(parsed) else suppressWarnings(as.numeric(xraw))
    yv <- numeric_column(yname)
    kept <- keep(xv, yv)
    xv <- kept[[1]]; yv <- kept[[2]]
    ord <- order(xv)
    xv <- xv[ord]; yv <- yv[ord]
    if (length(xv) < 2) stop("fewer than two points to draw")
    if (is_date) {
      plot(as.Date(xv, origin = "1970-01-01"), yv, type = "l", col = "#10b981", lwd = 2,
           main = title, xlab = xname, ylab = yname)
    } else {
      plot(xv, yv, type = "l", col = "#10b981", lwd = 2,
           main = title, xlab = xname, ylab = yname)
    }
  } else {
    xv <- numeric_column(xname); yv <- numeric_column(yname)
    kept <- keep(xv, yv)
    xv <- kept[[1]]; yv <- kept[[2]]
    if (length(xv) < 2) stop("fewer than two points to draw")
    plot(xv, yv, pch = 16, col = "#3b82f6", main = title, xlab = xname, ylab = yname)
    if (fit) {
      if (length(xv) < 3) stop("a fitted line needs at least three points")
      abline(lm(yv ~ xv), col = "#ef4444", lwd = 2)
    }
  }
  points <- length(xv)
} else {
  stop("unknown chart kind: ", kind)
}
invisible(dev.off())
cat("r_version: ", paste(R.version$major, R.version$minor, sep = "."), "\n", sep = "")
cat("points: ", points, "\n", sep = "")
"##;

/// The chart kinds and the arguments each one needs.
const KINDS: [&str; 5] = ["histogram", "boxplot", "bar", "scatter", "line"];

/// What one chart request asks R for, checked before R is started.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    kind: &'static str,
    column: String,
    group: String,
    x: String,
    y: String,
    fit: bool,
    bins: i64,
    title: String,
}

fn text_argument(arguments: &Value, key: &str) -> Result<String, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.contains('\n') || text.contains('\r') || text.contains('\0') {
                return Err(format!("{key} must be a single line"));
            }
            Ok(text.to_owned())
        }
        Some(_) => Err(format!("{key} must be text")),
    }
}

/// Check a request and fill in the parts each kind uses. Anything missing for the chosen kind is
/// an error that names it, never a silent default.
fn parse_request(arguments: &Value) -> Result<Request, String> {
    let kind_text = arguments
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = KINDS
        .iter()
        .copied()
        .find(|known| *known == kind_text)
        .ok_or_else(|| format!("kind must be one of: {}", KINDS.join(", ")))?;
    let column = text_argument(arguments, "column")?;
    let group = text_argument(arguments, "group")?;
    let x = text_argument(arguments, "x")?;
    let y = text_argument(arguments, "y")?;
    let title = text_argument(arguments, "title")?;
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("title is longer than {MAX_TITLE_CHARS} characters"));
    }
    let fit = match arguments.get("fit") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => return Err("fit must be true or false".to_owned()),
    };
    let bins = match arguments.get("bins") {
        None | Some(Value::Null) => 0,
        Some(value) => value
            .as_i64()
            .filter(|bins| (1..=MAX_BINS).contains(bins))
            .ok_or_else(|| format!("bins must be a whole number from 1 to {MAX_BINS}"))?,
    };
    let need = |present: bool, name: &str| {
        if present {
            Ok(())
        } else {
            Err(format!("{name} is required for a {kind} chart"))
        }
    };
    match kind {
        "histogram" => need(!column.is_empty(), "column")?,
        "boxplot" | "bar" => {
            need(!column.is_empty(), "column")?;
            need(!group.is_empty(), "group")?;
        }
        "scatter" => {
            need(!x.is_empty(), "x")?;
            need(!y.is_empty(), "y")?;
        }
        _ => {
            need(!x.is_empty(), "x")?;
            need(!y.is_empty(), "y")?;
        }
    }
    Ok(Request {
        kind,
        column,
        group,
        x,
        y,
        fit: fit && kind == "scatter",
        bins: if kind == "histogram" { bins } else { 0 },
        title,
    })
}

/// A PNG file name: relative, inside the workspace, ending in `.png`.
fn valid_figure_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    let is_relative = !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    let is_png = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if !is_relative || !is_png || path.components().next().is_none() {
        return Err(
            "chart_path must be a relative path inside the workspace, ending in .png".to_owned(),
        );
    }
    Ok(())
}

/// The record that sits next to a figure: what was drawn, from which file, by which R.
fn figure_record(
    request: &Request,
    chart_path: &str,
    input_sha256: &str,
    figure_sha256: &str,
    report: &std::collections::BTreeMap<String, String>,
) -> Value {
    json!({
        "format": "delta.figure",
        "version": 1,
        "figure": chart_path,
        "figure_sha256": figure_sha256,
        "kind": request.kind,
        "parameters": {
            "column": request.column, "group": request.group, "x": request.x, "y": request.y,
            "fit": request.fit, "bins": request.bins, "title": request.title,
        },
        "points": report.get("points").and_then(|value| value.parse::<u64>().ok()),
        "provenance": {
            "engine": "R",
            "r_version": report.get("r_version").cloned().unwrap_or_default(),
            "graphics": "base",
            "script_sha256": sha256_hex(CHART_SCRIPT.as_bytes()),
            "input_sha256": input_sha256,
        },
    })
}

pub fn registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "chart.render".to_owned(),
        tool_name: "chart_render".to_owned(),
        side_effect: SideEffectClass::LocalWrite,
        description: "Draw one chart from a CSV file in the workspace and save it as a PNG: a \
            histogram of one numeric column, a boxplot or a bar chart of group means, a scatter \
            plot of two numeric columns (with `fit` for a regression line), or a line over a date \
            or ordered column. The figure is drawn by R, the same R that computes the statistics, \
            and a `delta.figure` record next to it gives the input's SHA-256, the R version and \
            the drawing program. Missing values are left out. The chart can be referred to from a \
            Markdown report as an image; it cannot be placed inside a Word or Excel file yet."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["kind", "path", "chart_path"],
            "properties": {
                "kind": {"type": "string", "enum": KINDS},
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "chart_path": {"type": "string", "description": "Where to save the PNG, relative to the workspace, ending in .png. The record is saved beside it as <chart_path>.figure.json."},
                "column": {"type": "string", "description": "histogram, boxplot, bar: the numeric column."},
                "group": {"type": "string", "description": "boxplot, bar: the column that names each row's group (at most 20 groups)."},
                "x": {"type": "string", "description": "scatter: the numeric x column. line: the date or ordered column."},
                "y": {"type": "string", "description": "scatter, line: the numeric y column."},
                "fit": {"type": "boolean", "description": "scatter: draw the least-squares line. Default false."},
                "bins": {"type": "integer", "description": "histogram: number of bins, 1 to 200. Default: chosen by R."},
                "title": {"type": "string", "description": "The chart title, one line, at most 120 characters."}
            }
        }),
        metadata: json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "filesystem", "capabilities": ["chart.render"]
        }),
        grants: CapabilityGrants::default(),
        workspace_write: true,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let request = match parse_request(&job.arguments) {
                Ok(request) => request,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("arguments"))
                }
            };
            let Some(path) = job.arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "path is required",
                    Some("arguments"),
                );
            };
            let Some(chart_path) = job.arguments.get("chart_path").and_then(Value::as_str) else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "chart_path is required",
                    Some("arguments"),
                );
            };
            if let Err(message) = valid_figure_path(chart_path) {
                return CapabilityResult::failed(&job.job_id, &message, Some("arguments"));
            }
            let input = match resolve_read_path(job, path) {
                Ok(input) => input,
                Err((code, message)) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some(code))
                }
            };
            let Some(staging_dir) = job.artifact_staging_dir.as_deref() else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "artifact staging is unavailable",
                    Some("staging"),
                );
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let rscript = match find_rscript() {
                Ok(rscript) => rscript,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("unavailable"))
                }
            };
            let input_bytes = match std::fs::read(&input) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &format!("could not read the input: {error}"),
                        Some("read_failed"),
                    )
                }
            };
            let staged_png = PathBuf::from(staging_dir).join("candidate.png");
            let arguments = vec![
                request.kind.to_owned(),
                input.to_string_lossy().into_owned(),
                staged_png.to_string_lossy().into_owned(),
                request.column.clone(),
                request.group.clone(),
                request.x.clone(),
                request.y.clone(),
                if request.fit { "TRUE" } else { "FALSE" }.to_owned(),
                request.bins.to_string(),
                request.title.clone(),
            ];
            let output = match run_rscript(&rscript, CHART_SCRIPT, &arguments, CHART_TIMEOUT) {
                Ok(output) => output,
                Err(message) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &message,
                        Some("statistics_failed"),
                    )
                }
            };
            let report = match parse_dcf(&output) {
                Ok(report) => report,
                Err(message) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &message,
                        Some("statistics_failed"),
                    )
                }
            };
            let png = match std::fs::read(&staged_png) {
                Ok(bytes) if bytes.starts_with(b"\x89PNG") => bytes,
                _ => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        "R did not produce a PNG file, so nothing was saved",
                        Some("statistics_failed"),
                    )
                }
            };
            let figure_sha256 = sha256_hex(&png);
            let record = figure_record(
                &request,
                chart_path,
                &sha256_hex(&input_bytes),
                &figure_sha256,
                &report,
            );
            let record_bytes = match serde_json::to_vec_pretty(&record) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return CapabilityResult::failed(
                        &job.job_id,
                        &error.to_string(),
                        Some("write_failed"),
                    )
                }
            };
            let staged_record = PathBuf::from(staging_dir).join("candidate.figure.json");
            if let Err(error) = std::fs::write(&staged_record, &record_bytes) {
                return CapabilityResult::failed(
                    &job.job_id,
                    &error.to_string(),
                    Some("write_failed"),
                );
            }
            let record_path = format!("{chart_path}.figure.json");
            CapabilityResult::completed(
                &job.job_id,
                json!({
                    "ok": true, "kind": request.kind, "chart_path": chart_path,
                    "figure_path": record_path, "figure_sha256": figure_sha256,
                    "points": record["points"], "r_version": report.get("r_version"),
                }),
            )
            .with_artifact(CapabilityArtifact {
                staging_path: staged_png.to_string_lossy().into_owned(),
                relative_path: chart_path.to_owned(),
                kind: Some("file".to_owned()),
                sha256: None,
                size: Some(png.len() as u64),
                incomplete: false,
            })
            .with_artifact(CapabilityArtifact {
                staging_path: staged_record.to_string_lossy().into_owned(),
                relative_path: record_path,
                kind: Some("file".to_owned()),
                sha256: None,
                size: Some(record_bytes.len() as u64),
                incomplete: false,
            })
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{ToolExecutionContext, ToolExecutor};
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;

    fn arguments(value: Value) -> Value {
        value
    }

    fn chart_context(workspace: &Path) -> ToolExecutionContext {
        ToolExecutionContext {
            session_id: "session-1".to_owned(),
            run_id: "run-1".to_owned(),
            workspace: Some(workspace.to_string_lossy().into_owned()),
            timeout: Duration::from_secs(60),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::new(),
            proxy: None,
        }
    }

    #[test]
    fn each_kind_needs_its_own_columns_and_says_which_is_missing() {
        let histogram = parse_request(&arguments(
            json!({"kind": "histogram", "column": "revenue"}),
        ))
        .unwrap();
        assert_eq!(histogram.kind, "histogram");
        let error = parse_request(&arguments(json!({"kind": "histogram"}))).unwrap_err();
        assert!(error.contains("column is required"), "{error}");
        let error =
            parse_request(&arguments(json!({"kind": "boxplot", "column": "v"}))).unwrap_err();
        assert!(error.contains("group is required"), "{error}");
        let error = parse_request(&arguments(json!({"kind": "scatter", "x": "a"}))).unwrap_err();
        assert!(error.contains("y is required"), "{error}");
        let error = parse_request(&arguments(json!({"kind": "pie"}))).unwrap_err();
        assert!(error.contains("kind must be one of"), "{error}");
    }

    #[test]
    fn options_are_checked_and_ignored_where_they_do_not_apply() {
        let bad_bins = parse_request(&arguments(
            json!({"kind": "histogram", "column": "v", "bins": 0}),
        ))
        .unwrap_err();
        assert!(bad_bins.contains("bins must be"), "{bad_bins}");
        let bad_fit = parse_request(&arguments(
            json!({"kind": "scatter", "x": "a", "y": "b", "fit": "yes"}),
        ))
        .unwrap_err();
        assert!(bad_fit.contains("fit must be"), "{bad_fit}");
        // A bar chart has no fitted line, so `fit` and `bins` are left out rather than passed on.
        let bar = parse_request(&arguments(
            json!({"kind": "bar", "column": "v", "group": "g", "fit": true, "bins": 9}),
        ))
        .unwrap();
        assert!(!bar.fit);
        assert_eq!(bar.bins, 0);
    }

    #[test]
    fn a_title_with_a_line_break_or_too_long_is_refused_before_r_is_started() {
        let two_lines = parse_request(&arguments(
            json!({"kind": "histogram", "column": "v", "title": "a\nb"}),
        ))
        .unwrap_err();
        assert!(two_lines.contains("single line"), "{two_lines}");
        let long = "x".repeat(MAX_TITLE_CHARS + 1);
        let too_long = parse_request(&arguments(
            json!({"kind": "histogram", "column": "v", "title": long}),
        ))
        .unwrap_err();
        assert!(too_long.contains("120 characters"), "{too_long}");
    }

    #[test]
    fn the_figure_must_be_a_relative_png_in_the_workspace() {
        assert!(valid_figure_path("out/chart.png").is_ok());
        assert!(valid_figure_path("out/CHART.PNG").is_ok());
        assert!(valid_figure_path("../chart.png").is_err());
        assert!(valid_figure_path("/tmp/chart.png").is_err());
        assert!(valid_figure_path("out/chart.jpg").is_err());
        assert!(valid_figure_path("chart.png.json").is_err());
    }

    #[test]
    fn the_registration_offers_the_kinds_and_needs_approval() {
        let registration = registration();
        assert_eq!(registration.tool_name, "chart_render");
        assert_eq!(registration.metadata["requires_approval"], true);
        assert_eq!(registration.metadata["category"], "filesystem");
        assert_eq!(
            registration.parameters["properties"]["kind"]["enum"],
            json!(KINDS)
        );
    }

    #[test]
    fn the_record_names_the_input_the_figure_and_the_program() {
        let request =
            parse_request(&arguments(json!({"kind": "histogram", "column": "v"}))).unwrap();
        let mut report = std::collections::BTreeMap::new();
        report.insert("r_version".to_owned(), "4.6.0".to_owned());
        report.insert("points".to_owned(), "10".to_owned());
        let record = figure_record(&request, "out/h.png", "in-hash", "fig-hash", &report);
        assert_eq!(record["format"], "delta.figure");
        assert_eq!(record["figure_sha256"], "fig-hash");
        assert_eq!(record["provenance"]["input_sha256"], "in-hash");
        assert_eq!(record["provenance"]["r_version"], "4.6.0");
        assert_eq!(record["points"], 10);
        assert_eq!(
            record["provenance"]["script_sha256"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
    }

    /// Whether the Rscript on PATH can write PNG files without X11, which is what `chart_render`
    /// needs. The CI build of R 4.6.0 is compiled without cairo, so it cannot draw PNG files.
    fn r_can_draw_png() -> bool {
        std::process::Command::new("Rscript")
            .args(["--vanilla", "-e", "cat(capabilities(\"cairo\"))"])
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "TRUE")
    }

    /// The R-dependent test: the figure is a real PNG, and its record names it by hash. It is
    /// skipped where R cannot draw PNG files, as the other statistics tests skip without R.
    #[test]
    fn a_histogram_is_drawn_to_a_png_with_its_record() {
        if !r_can_draw_png() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("sales.csv"),
            "region,revenue\neast,1200\nwest,980\nnorth,1500\nsouth,900\neast,1300\nwest,1050\n",
        )
        .unwrap();
        let host = crate::capability::CapabilityHost::product_defaults().unwrap();
        let call = crate::runtime::ToolCall {
            id: "call-chart".to_owned(),
            name: "chart_render".to_owned(),
            arguments: json!({"kind": "histogram", "path": "sales.csv", "column": "revenue", "chart_path": "out/revenue.png", "title": "Revenue"}),
        };
        let result = host.execute(&call, &chart_context(temp.path()));
        if result
            .error
            .as_deref()
            .is_some_and(|e| e.contains("not installed") || e.contains("R 4."))
        {
            return;
        }
        assert_eq!(
            result.state,
            crate::runtime::ToolExitState::Completed,
            "{:?}",
            result.error
        );
        assert_eq!(result.staged_artifacts.len(), 2);
        let png = std::fs::read(&result.staged_artifacts[0].staging_path).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let record: Value = serde_json::from_slice(
            &std::fs::read(&result.staged_artifacts[1].staging_path).unwrap(),
        )
        .unwrap();
        assert_eq!(record["figure_sha256"], result.output["figure_sha256"]);
        assert_eq!(record["points"], 6);
    }
}
