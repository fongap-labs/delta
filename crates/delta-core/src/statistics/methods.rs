//! Inference, regression and design-of-experiments methods computed by R (ADR-0054).
//!
//! Every method follows the shape of `describe`: Rust validates the arguments and passes them to a
//! compiled-in R program as command-line arguments, never as R source, so a column name can not
//! become code. R computes every figure and prints `key: value` lines; Rust parses them and adds
//! the provenance. Text that comes from the data (group labels, test names, warnings) is
//! percent-encoded by R, so a label can not break the line format.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::{find_rscript, parse_dcf, run_rscript, sha256_hex};
use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};
use crate::skills::SideEffectClass;

/// How long R may take for one method.
const METHOD_TIMEOUT: Duration = Duration::from_secs(60);

/// The most predictors a regression takes, and the most factors a design takes.
const MAX_PREDICTORS: usize = 8;
const MAX_FACTORS: usize = 8;
/// The most runs a generated design may have.
const MAX_DESIGN_RUNS: usize = 1024;
const MAX_REPLICATES: u64 = 10;
const MAX_CENTER_POINTS: u64 = 10;
/// R reports at most this many warnings; later ones are dropped.
const MAX_WARNINGS: usize = 5;
/// The argument that stands for "no input file". It does not start with `-`, which Rscript would
/// read as an option.
const NO_INPUT: &str = "none";

/// The R code every method starts with. It reads the arguments (as UTF-8, whatever the locale),
/// loads the CSV on request, and collects R's warnings so they can be reported with the result.
const PRELUDE: &str = r#"
args <- commandArgs(trailingOnly = TRUE)
Encoding(args) <- "UTF-8"
num <- function(value) sprintf("%.17g", value)
enc <- function(text) utils::URLencode(enc2utf8(as.character(text)), reserved = TRUE)
caught <- character(0)
quietly <- function(expr) withCallingHandlers(expr, warning = function(w) {
  caught <<- c(caught, conditionMessage(w))
  invokeRestart("muffleWarning")
})
load_data <- function() read.csv(args[1], stringsAsFactors = FALSE, check.names = FALSE, encoding = "UTF-8")
column <- function(data, name) {
  if (!(name %in% names(data))) stop("column not found: ", name)
  data[[name]]
}
numeric_column <- function(data, name) suppressWarnings(as.numeric(column(data, name)))
finish <- function() {
  shown <- head(caught, 5)
  for (i in seq_along(shown)) cat("warning.", i, ": ", enc(shown[i]), "\n", sep = "")
  cat("r_version: ", paste(R.version$major, R.version$minor, sep = "."), "\n", sep = "")
}
"#;

/// Arguments: kind, column, other column (group or second column), mu, equal variance,
/// alternative, confidence level.
const TTEST_SCRIPT: &str = r#"
data <- load_data()
kind <- args[2]
mu <- as.numeric(args[5])
equal <- args[6] == "true"
alternative <- args[7]
level <- as.numeric(args[8])
if (kind == "one_sample") {
  x <- numeric_column(data, args[3])
  x <- x[!is.na(x)]
  test <- quietly(t.test(x, mu = mu, alternative = alternative, conf.level = level))
  cat("n: ", num(length(x)), "\n", "mean: ", num(mean(x)), "\n", sep = "")
  estimate <- unname(test$estimate)
} else if (kind == "paired") {
  a <- numeric_column(data, args[3])
  b <- numeric_column(data, args[4])
  keep <- !is.na(a) & !is.na(b)
  a <- a[keep]
  b <- b[keep]
  test <- quietly(t.test(a, b, paired = TRUE, mu = mu, alternative = alternative, conf.level = level))
  cat("n: ", num(length(a)), "\n", sep = "")
  estimate <- unname(test$estimate)
} else {
  y <- numeric_column(data, args[3])
  g <- as.character(column(data, args[4]))
  keep <- !is.na(y) & !is.na(g) & nzchar(g)
  y <- y[keep]
  g <- g[keep]
  groups <- sort(unique(g), method = "radix")
  if (length(groups) != 2) stop("the group column must have exactly two groups, found ", length(groups))
  a <- y[g == groups[1]]
  b <- y[g == groups[2]]
  test <- quietly(t.test(a, b, var.equal = equal, mu = mu, alternative = alternative, conf.level = level))
  cat("group1: ", enc(groups[1]), "\n", "group2: ", enc(groups[2]), "\n",
      "n1: ", num(length(a)), "\n", "n2: ", num(length(b)), "\n",
      "mean1: ", num(mean(a)), "\n", "mean2: ", num(mean(b)), "\n", sep = "")
  estimate <- unname(test$estimate[1] - test$estimate[2])
}
cat("estimate: ", num(estimate), "\n",
    "statistic: ", num(unname(test$statistic)), "\n",
    "df: ", num(unname(test$parameter)), "\n",
    "p_value: ", num(test$p.value), "\n",
    "conf_low: ", num(test$conf.int[1]), "\n",
    "conf_high: ", num(test$conf.int[2]), "\n",
    "method: ", enc(test$method), "\n", sep = "")
finish()
"#;

/// Arguments: response column, group column.
const ANOVA_SCRIPT: &str = r#"
data <- load_data()
y <- numeric_column(data, args[2])
g <- as.character(column(data, args[3]))
keep <- !is.na(y) & !is.na(g) & nzchar(g)
y <- y[keep]
g <- factor(g[keep], levels = sort(unique(g[keep]), method = "radix"))
if (nlevels(g) < 2) stop("the group column needs at least two groups, found ", nlevels(g))
if (length(y) <= nlevels(g)) stop("every group has one observation, so the within-group variance cannot be estimated")
fit <- quietly(aov(y ~ g))
table <- summary(fit)[[1]]
cat("n: ", num(length(y)), "\n", "groups: ", num(nlevels(g)), "\n", sep = "")
for (i in seq_len(nlevels(g))) {
  v <- y[g == levels(g)[i]]
  cat("group.", i, ": ", enc(levels(g)[i]), "\n",
      "group_n.", i, ": ", num(length(v)), "\n",
      "group_mean.", i, ": ", num(mean(v)), "\n",
      "group_sd.", i, ": ", num(sd(v)), "\n", sep = "")
}
cat("df_between: ", num(table[["Df"]][1]), "\n",
    "df_within: ", num(table[["Df"]][2]), "\n",
    "ss_between: ", num(table[["Sum Sq"]][1]), "\n",
    "ss_within: ", num(table[["Sum Sq"]][2]), "\n",
    "ms_between: ", num(table[["Mean Sq"]][1]), "\n",
    "ms_within: ", num(table[["Mean Sq"]][2]), "\n",
    "f_value: ", num(table[["F value"]][1]), "\n",
    "p_value: ", num(table[["Pr(>F)"]][1]), "\n", sep = "")
finish()
"#;

/// Arguments: first column, second column, continuity correction.
const CHISQ_SCRIPT: &str = r#"
data <- load_data()
a <- as.character(column(data, args[2]))
b <- as.character(column(data, args[3]))
keep <- !is.na(a) & !is.na(b) & nzchar(a) & nzchar(b)
cells <- table(a[keep], b[keep])
if (nrow(cells) < 2 || ncol(cells) < 2) stop("each column needs at least two distinct values")
test <- quietly(chisq.test(cells, correct = args[4] == "true"))
cat("n: ", num(sum(cells)), "\n",
    "rows: ", num(nrow(cells)), "\n",
    "columns: ", num(ncol(cells)), "\n",
    "statistic: ", num(unname(test$statistic)), "\n",
    "df: ", num(unname(test$parameter)), "\n",
    "p_value: ", num(test$p.value), "\n",
    "min_expected: ", num(min(test$expected)), "\n",
    "method: ", enc(test$method), "\n", sep = "")
finish()
"#;

/// Arguments: first column, second column, method, confidence level.
const CORRELATION_SCRIPT: &str = r#"
data <- load_data()
a <- numeric_column(data, args[2])
b <- numeric_column(data, args[3])
keep <- !is.na(a) & !is.na(b)
a <- a[keep]
b <- b[keep]
test <- quietly(cor.test(a, b, method = args[4], conf.level = as.numeric(args[5])))
low <- if (is.null(test$conf.int)) NA_real_ else test$conf.int[1]
high <- if (is.null(test$conf.int)) NA_real_ else test$conf.int[2]
cat("n: ", num(length(a)), "\n",
    "estimate: ", num(unname(test$estimate)), "\n",
    "statistic_name: ", enc(names(test$statistic)), "\n",
    "statistic: ", num(unname(test$statistic)), "\n",
    "p_value: ", num(test$p.value), "\n",
    "conf_low: ", num(low), "\n",
    "conf_high: ", num(high), "\n",
    "method: ", enc(test$method), "\n", sep = "")
finish()
"#;

/// Arguments: response column, pairwise interactions, then the predictor columns. The predictors
/// are renamed x1, x2, … so that no column name reaches a model formula.
const REGRESSION_SCRIPT: &str = r#"
data <- load_data()
predictors <- args[4:length(args)]
d <- data.frame(y = numeric_column(data, args[2]))
for (i in seq_along(predictors)) d[[paste0("x", i)]] <- numeric_column(data, predictors[i])
d <- d[complete.cases(d), , drop = FALSE]
fit <- quietly(if (args[3] == "true") lm(y ~ .^2, data = d) else lm(y ~ ., data = d))
s <- quietly(summary(fit))
co <- coef(fit)
cells <- s$coefficients
cat("n: ", num(nrow(d)), "\n",
    "df_residual: ", num(fit$df.residual), "\n",
    "sigma: ", num(s$sigma), "\n",
    "r_squared: ", num(s$r.squared), "\n",
    "adj_r_squared: ", num(s$adj.r.squared), "\n", sep = "")
fs <- s$fstatistic
if (is.null(fs)) {
  cat("f_statistic: NA\nf_p_value: NA\n")
} else {
  cat("f_statistic: ", num(unname(fs[1])), "\n",
      "f_p_value: ", num(pf(fs[1], fs[2], fs[3], lower.tail = FALSE)), "\n", sep = "")
}
cat("terms: ", num(length(co)), "\n", sep = "")
for (i in seq_along(co)) {
  term <- names(co)[i]
  row <- if (term %in% rownames(cells)) cells[term, ] else rep(NA_real_, 4)
  cat("term.", i, ": ", term, "\n",
      "estimate.", i, ": ", num(co[[i]]), "\n",
      "std_error.", i, ": ", num(unname(row[2])), "\n",
      "t_value.", i, ": ", num(unname(row[3])), "\n",
      "p_value.", i, ": ", num(unname(row[4])), "\n", sep = "")
}
finish()
"#;

/// The registrations of every method in this module.
pub fn registrations() -> Vec<CapabilityRegistration> {
    vec![
        ttest_registration(),
        anova_registration(),
        chisq_registration(),
        correlation_registration(),
        regression_registration(),
        ancova::ancova_registration(),
        doe_registration(),
        record::registration(),
    ]
}

/// What one method needs: how its arguments become R arguments, and how R's answer becomes JSON.
struct Method {
    name: &'static str,
    script: &'static str,
    /// Whether the first R argument is a workspace CSV (every method but the design generator).
    reads_input: bool,
    prepare: fn(&Value) -> Result<Prepared, String>,
    shape: fn(&BTreeMap<String, String>, &Prepared) -> Result<Value, String>,
}

/// Validated arguments: what R receives after the input path, and what the result reports back.
struct Prepared {
    r_args: Vec<String>,
    parameters: Value,
    /// Names the answer refers to by position (factor or predictor names).
    names: Vec<String>,
}

fn registration(
    capability_id: &str,
    tool_name: &str,
    description: &str,
    parameters: Value,
    method: &'static Method,
) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: capability_id.to_owned(),
        tool_name: tool_name.to_owned(),
        side_effect: if method.reads_input {
            SideEffectClass::Read
        } else {
            SideEffectClass::None
        },
        description: description.to_owned(),
        parameters,
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": [capability_id]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            let input = if method.reads_input {
                let Some(path) = job.arguments.get("path").and_then(Value::as_str) else {
                    return CapabilityResult::failed(
                        &job.job_id,
                        "path is required",
                        Some("arguments"),
                    );
                };
                match crate::capability::resolve_read_path(job, path) {
                    Ok(resolved) => Some(resolved),
                    Err((code, message)) => {
                        return CapabilityResult::failed(&job.job_id, &message, Some(code))
                    }
                }
            } else {
                None
            };
            let prepared = match (method.prepare)(&job.arguments) {
                Ok(prepared) => prepared,
                Err(message) => {
                    return CapabilityResult::failed(&job.job_id, &message, Some("arguments"))
                }
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
            match run(
                method,
                &rscript,
                input.as_deref(),
                &prepared,
                METHOD_TIMEOUT,
            ) {
                Ok(result) => CapabilityResult::completed(&job.job_id, result),
                Err(error) => {
                    CapabilityResult::failed(&job.job_id, &error, Some("statistics_failed"))
                }
            }
        })),
    }
}

/// Run `method` with the R at `rscript` and return its figures with their provenance.
fn run(
    method: &Method,
    rscript: &Path,
    input: Option<&Path>,
    prepared: &Prepared,
    timeout: Duration,
) -> Result<Value, String> {
    let (input_arg, input_sha256) = match input {
        Some(path) => {
            let bytes = std::fs::read(path)
                .map_err(|error| format!("could not read the input: {error}"))?;
            (
                path.to_string_lossy().into_owned(),
                Some(sha256_hex(&bytes)),
            )
        }
        None => (NO_INPUT.to_owned(), None),
    };
    let script = format!("{PRELUDE}{}", method.script);
    let mut arguments = vec![input_arg];
    arguments.extend(prepared.r_args.iter().cloned());
    let output = run_rscript(rscript, &script, &arguments, timeout)?;
    let values = parse_dcf(&output)?;
    let mut result = (method.shape)(&values, prepared)?;
    let warnings = warnings_of(&values)?;
    let r_version = values
        .get("r_version")
        .cloned()
        .ok_or_else(|| "R did not report its version".to_owned())?;
    let object = result
        .as_object_mut()
        .ok_or_else(|| "a method result is an object".to_owned())?;
    object.insert("warnings".to_owned(), json!(warnings));
    object.insert(
        "provenance".to_owned(),
        json!({
            "engine": "R",
            "r_version": r_version,
            "packages": ["base", "stats", "utils"],
            "script_sha256": sha256_hex(script.as_bytes()),
            "input_sha256": input_sha256,
            "method": method.name,
            "parameters": prepared.parameters,
        }),
    );
    Ok(result)
}

// --- Reading R's answer -----------------------------------------------------------------

/// A figure R reported. R's `NA`, `NaN` and infinities are not JSON numbers; they read as null.
fn figure(values: &BTreeMap<String, String>, key: &str) -> Result<Value, String> {
    let text = values
        .get(key)
        .ok_or_else(|| format!("R did not report {key}"))?;
    if matches!(text.as_str(), "NA" | "NaN" | "Inf" | "-Inf") {
        return Ok(Value::Null);
    }
    let number = text
        .parse::<f64>()
        .map_err(|_| format!("R reported an unreadable {key}: {text}"))?;
    Ok(json!(number))
}

/// Text R reported, percent-decoded.
fn text(values: &BTreeMap<String, String>, key: &str) -> Result<String, String> {
    let encoded = values
        .get(key)
        .ok_or_else(|| format!("R did not report {key}"))?;
    percent_decode(encoded)
}

fn percent_decode(encoded: &str) -> Result<String, String> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = encoded
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| format!("R reported unreadable text: {encoded}"))?;
            decoded.push(pair);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| format!("R reported text that is not UTF-8: {encoded}"))
}

fn warnings_of(values: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
    (1..=MAX_WARNINGS)
        .map(|index| format!("warning.{index}"))
        .take_while(|key| values.contains_key(key))
        .map(|key| text(values, &key))
        .collect()
}

/// A count R reported, as an integer index.
fn count(values: &BTreeMap<String, String>, key: &str) -> Result<usize, String> {
    let text = values
        .get(key)
        .ok_or_else(|| format!("R did not report {key}"))?;
    text.parse::<f64>()
        .ok()
        .filter(|number| *number >= 0.0 && number.fract() == 0.0)
        .map(|number| number as usize)
        .ok_or_else(|| format!("R reported an unreadable {key}: {text}"))
}

// --- Reading the arguments --------------------------------------------------------------

fn required_text(arguments: &Value, key: &str) -> Result<String, String> {
    match arguments.get(key).and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => Ok(text.to_owned()),
        _ => Err(format!("{key} is required")),
    }
}

fn flag(arguments: &Value, key: &str, is_on_by_default: bool) -> Result<bool, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(is_on_by_default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{key} must be true or false")),
    }
}

fn choice(arguments: &Value, key: &str, default: &str, allowed: &[&str]) -> Result<String, String> {
    let value = match arguments.get(key) {
        None | Some(Value::Null) => default,
        Some(Value::String(text)) => text.as_str(),
        Some(_) => return Err(format!("{key} must be text")),
    };
    if allowed.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err(format!("{key} must be one of: {}", allowed.join(", ")))
    }
}

fn number_in(
    arguments: &Value,
    key: &str,
    default: f64,
    open_range: (f64, f64),
) -> Result<f64, String> {
    let value = match arguments.get(key) {
        None | Some(Value::Null) => return Ok(default),
        Some(value) => value
            .as_f64()
            .ok_or_else(|| format!("{key} must be a number"))?,
    };
    if value.is_finite() && value > open_range.0 && value < open_range.1 {
        Ok(value)
    } else {
        Err(format!(
            "{key} must be between {} and {}, not including either",
            open_range.0, open_range.1
        ))
    }
}

fn whole_number(
    arguments: &Value,
    key: &str,
    default: u64,
    range: (u64, u64),
) -> Result<u64, String> {
    let value = match arguments.get(key) {
        None | Some(Value::Null) => return Ok(default),
        Some(value) => value
            .as_u64()
            .ok_or_else(|| format!("{key} must be a whole number"))?,
    };
    if (range.0..=range.1).contains(&value) {
        Ok(value)
    } else {
        Err(format!("{key} must be between {} and {}", range.0, range.1))
    }
}

fn real(arguments: &Value, key: &str, default: f64) -> Result<f64, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| format!("{key} must be a finite number")),
    }
}

fn column_list(arguments: &Value, key: &str, limit: usize) -> Result<Vec<String>, String> {
    let list = arguments
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} is required: a list of column names"))?;
    if list.is_empty() || list.len() > limit {
        return Err(format!("{key} needs between 1 and {limit} names"));
    }
    let mut names = Vec::with_capacity(list.len());
    for item in list {
        let name = item
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| format!("{key} must hold column names"))?;
        if names.iter().any(|seen| seen == name) {
            return Err(format!("{key} names {name} twice"));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

// --- t test -----------------------------------------------------------------------------

static TTEST: Method = Method {
    name: "ttest",
    script: TTEST_SCRIPT,
    reads_input: true,
    prepare: prepare_ttest,
    shape: shape_ttest,
};

fn ttest_registration() -> CapabilityRegistration {
    registration(
        "statistics.ttest",
        "statistics_ttest",
        "Student's or Welch's t test on a CSV file in the workspace: one sample against a mean, \
         two groups of one column, or two paired columns. Returns the estimate, t, degrees of \
         freedom, p value and confidence interval, computed by R; a one-sided test's interval is \
         open on one side and that bound is null. The assumptions (normality, independence) are \
         not checked.",
        json!({
            "type": "object", "required": ["path", "kind", "column"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "kind": {"type": "string", "enum": ["one_sample", "two_sample", "paired"]},
                "column": {"type": "string", "description": "The numeric column (the first one, for paired)."},
                "group": {"type": "string", "description": "two_sample: the column that names the two groups."},
                "column_b": {"type": "string", "description": "paired: the second numeric column; the estimate is column minus column_b."},
                "mu": {"type": "number", "description": "The mean (or difference) under the null hypothesis. Default 0."},
                "equal_variance": {"type": "boolean", "description": "two_sample: pool the variances (Student). Default false (Welch)."},
                "alternative": {"type": "string", "enum": ["two.sided", "less", "greater"], "description": "Default two.sided."},
                "conf_level": {"type": "number", "description": "Confidence level, between 0 and 1. Default 0.95."}
            }
        }),
        &TTEST,
    )
}

fn prepare_ttest(arguments: &Value) -> Result<Prepared, String> {
    let kind = choice(
        arguments,
        "kind",
        "",
        &["one_sample", "two_sample", "paired"],
    )
    .map_err(|_| "kind must be one of: one_sample, two_sample, paired".to_owned())?;
    let column = required_text(arguments, "column")?;
    let other = match kind.as_str() {
        "two_sample" => required_text(arguments, "group")
            .map_err(|_| "group is required for a two_sample test".to_owned())?,
        "paired" => required_text(arguments, "column_b")
            .map_err(|_| "column_b is required for a paired test".to_owned())?,
        _ => String::new(),
    };
    let mu = real(arguments, "mu", 0.0)?;
    let equal_variance = flag(arguments, "equal_variance", false)?;
    let alternative = choice(
        arguments,
        "alternative",
        "two.sided",
        &["two.sided", "less", "greater"],
    )?;
    let conf_level = number_in(arguments, "conf_level", 0.95, (0.0, 1.0))?;
    Ok(Prepared {
        r_args: vec![
            kind.clone(),
            column.clone(),
            if other.is_empty() {
                NO_INPUT.to_owned()
            } else {
                other.clone()
            },
            mu.to_string(),
            equal_variance.to_string(),
            alternative.clone(),
            conf_level.to_string(),
        ],
        parameters: json!({
            "kind": kind, "column": column, "group": (kind == "two_sample").then_some(&other),
            "column_b": (kind == "paired").then_some(&other), "mu": mu,
            "equal_variance": equal_variance, "alternative": alternative,
            "conf_level": conf_level,
        }),
        names: Vec::new(),
    })
}

fn shape_ttest(values: &BTreeMap<String, String>, prepared: &Prepared) -> Result<Value, String> {
    let mut result = json!({
        "kind": prepared.parameters["kind"],
        "estimate": figure(values, "estimate")?,
        "statistic": figure(values, "statistic")?,
        "df": figure(values, "df")?,
        "p_value": figure(values, "p_value")?,
        "conf_low": figure(values, "conf_low")?,
        "conf_high": figure(values, "conf_high")?,
        "conf_level": prepared.parameters["conf_level"],
        "alternative": prepared.parameters["alternative"],
        "method": text(values, "method")?,
    });
    let object = result.as_object_mut().expect("a JSON object");
    match prepared.parameters["kind"].as_str() {
        Some("two_sample") => {
            object.insert(
                "groups".to_owned(),
                json!([
                    {"label": text(values, "group1")?, "n": figure(values, "n1")?, "mean": figure(values, "mean1")?},
                    {"label": text(values, "group2")?, "n": figure(values, "n2")?, "mean": figure(values, "mean2")?},
                ]),
            );
        }
        Some("one_sample") => {
            object.insert("n".to_owned(), figure(values, "n")?);
            object.insert("mean".to_owned(), figure(values, "mean")?);
        }
        _ => {
            object.insert("n".to_owned(), figure(values, "n")?);
        }
    }
    Ok(result)
}

// --- one-way ANOVA ----------------------------------------------------------------------

static ANOVA: Method = Method {
    name: "anova_oneway",
    script: ANOVA_SCRIPT,
    reads_input: true,
    prepare: prepare_anova,
    shape: shape_anova,
};

fn anova_registration() -> CapabilityRegistration {
    registration(
        "statistics.anova",
        "statistics_anova",
        "One-way analysis of variance of a numeric column across the groups named by another \
         column of a CSV file in the workspace. Returns each group's n, mean and standard \
         deviation, the sums of squares, F and the p value, computed by R. The assumptions \
         (normality, equal variances) are not checked.",
        json!({
            "type": "object", "required": ["path", "response", "group"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "response": {"type": "string", "description": "The numeric column."},
                "group": {"type": "string", "description": "The column that names each row's group."}
            }
        }),
        &ANOVA,
    )
}

fn prepare_anova(arguments: &Value) -> Result<Prepared, String> {
    let response = required_text(arguments, "response")?;
    let group = required_text(arguments, "group")?;
    Ok(Prepared {
        r_args: vec![response.clone(), group.clone()],
        parameters: json!({"response": response, "group": group}),
        names: Vec::new(),
    })
}

fn shape_anova(values: &BTreeMap<String, String>, _: &Prepared) -> Result<Value, String> {
    let groups = count(values, "groups")?;
    let mut table = Vec::with_capacity(groups);
    for index in 1..=groups {
        table.push(json!({
            "label": text(values, &format!("group.{index}"))?,
            "n": figure(values, &format!("group_n.{index}"))?,
            "mean": figure(values, &format!("group_mean.{index}"))?,
            "sd": figure(values, &format!("group_sd.{index}"))?,
        }));
    }
    Ok(json!({
        "n": figure(values, "n")?,
        "groups": table,
        "df_between": figure(values, "df_between")?,
        "df_within": figure(values, "df_within")?,
        "ss_between": figure(values, "ss_between")?,
        "ss_within": figure(values, "ss_within")?,
        "ms_between": figure(values, "ms_between")?,
        "ms_within": figure(values, "ms_within")?,
        "f_value": figure(values, "f_value")?,
        "p_value": figure(values, "p_value")?,
    }))
}

// --- chi-squared test of independence ---------------------------------------------------

static CHISQ: Method = Method {
    name: "chisq_independence",
    script: CHISQ_SCRIPT,
    reads_input: true,
    prepare: prepare_chisq,
    shape: shape_chisq,
};

fn chisq_registration() -> CapabilityRegistration {
    registration(
        "statistics.chisq",
        "statistics_chisq",
        "Pearson's chi-squared test of independence between two categorical columns of a CSV \
         file in the workspace. Returns the statistic, degrees of freedom, p value and the \
         smallest expected count, computed by R. A smallest expected count under 5 means the \
         approximation is unreliable; R's warning is returned too.",
        json!({
            "type": "object", "required": ["path", "column_a", "column_b"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "column_a": {"type": "string", "description": "The first categorical column."},
                "column_b": {"type": "string", "description": "The second categorical column."},
                "correct": {"type": "boolean", "description": "Yates' continuity correction, applied to 2x2 tables only. Default true."}
            }
        }),
        &CHISQ,
    )
}

fn prepare_chisq(arguments: &Value) -> Result<Prepared, String> {
    let a = required_text(arguments, "column_a")?;
    let b = required_text(arguments, "column_b")?;
    let correct = flag(arguments, "correct", true)?;
    Ok(Prepared {
        r_args: vec![a.clone(), b.clone(), correct.to_string()],
        parameters: json!({"column_a": a, "column_b": b, "correct": correct}),
        names: Vec::new(),
    })
}

fn shape_chisq(values: &BTreeMap<String, String>, _: &Prepared) -> Result<Value, String> {
    Ok(json!({
        "n": figure(values, "n")?,
        "rows": figure(values, "rows")?,
        "columns": figure(values, "columns")?,
        "statistic": figure(values, "statistic")?,
        "df": figure(values, "df")?,
        "p_value": figure(values, "p_value")?,
        "min_expected": figure(values, "min_expected")?,
        "method": text(values, "method")?,
    }))
}

// --- correlation ------------------------------------------------------------------------

static CORRELATION: Method = Method {
    name: "correlation",
    script: CORRELATION_SCRIPT,
    reads_input: true,
    prepare: prepare_correlation,
    shape: shape_correlation,
};

fn correlation_registration() -> CapabilityRegistration {
    registration(
        "statistics.correlation",
        "statistics_correlation",
        "Correlation between two numeric columns of a CSV file in the workspace, with a test of \
         zero correlation: Pearson, Spearman or Kendall. Rows missing either value are left out. \
         Returns the coefficient, statistic, p value and (Pearson only) a confidence interval, \
         computed by R.",
        json!({
            "type": "object", "required": ["path", "column_a", "column_b"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "column_a": {"type": "string", "description": "The first numeric column."},
                "column_b": {"type": "string", "description": "The second numeric column."},
                "method": {"type": "string", "enum": ["pearson", "spearman", "kendall"], "description": "Default pearson."},
                "conf_level": {"type": "number", "description": "Confidence level of the Pearson interval, between 0 and 1. Default 0.95."}
            }
        }),
        &CORRELATION,
    )
}

fn prepare_correlation(arguments: &Value) -> Result<Prepared, String> {
    let a = required_text(arguments, "column_a")?;
    let b = required_text(arguments, "column_b")?;
    let method = choice(
        arguments,
        "method",
        "pearson",
        &["pearson", "spearman", "kendall"],
    )?;
    let conf_level = number_in(arguments, "conf_level", 0.95, (0.0, 1.0))?;
    Ok(Prepared {
        r_args: vec![a.clone(), b.clone(), method.clone(), conf_level.to_string()],
        parameters: json!({"column_a": a, "column_b": b, "method": method, "conf_level": conf_level}),
        names: Vec::new(),
    })
}

fn shape_correlation(values: &BTreeMap<String, String>, _: &Prepared) -> Result<Value, String> {
    Ok(json!({
        "n": figure(values, "n")?,
        "estimate": figure(values, "estimate")?,
        "statistic_name": text(values, "statistic_name")?,
        "statistic": figure(values, "statistic")?,
        "p_value": figure(values, "p_value")?,
        "conf_low": figure(values, "conf_low")?,
        "conf_high": figure(values, "conf_high")?,
        "method": text(values, "method")?,
    }))
}

// --- linear regression ------------------------------------------------------------------

static REGRESSION: Method = Method {
    name: "regression_ols",
    script: REGRESSION_SCRIPT,
    reads_input: true,
    prepare: prepare_regression,
    shape: shape_regression,
};

fn regression_registration() -> CapabilityRegistration {
    registration(
        "statistics.regression",
        "statistics_regression",
        "Ordinary least squares regression of a numeric column on up to 8 numeric columns of a \
         CSV file in the workspace, optionally with every pairwise interaction. For a designed \
         experiment, use the coded -1/+1 columns. Returns each coefficient with its standard \
         error, t and p value, plus R squared, residual standard error and the overall F test, \
         computed by R. Rows missing any value are left out. A coefficient R can not estimate \
         (collinear columns) is returned as null and listed in `aliased_terms`.",
        json!({
            "type": "object", "required": ["path", "response", "predictors"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "response": {"type": "string", "description": "The numeric column to explain."},
                "predictors": {"type": "array", "items": {"type": "string"}, "description": "The numeric columns that explain it (1 to 8)."},
                "interactions": {"type": "boolean", "description": "Add every pairwise interaction. Default false."}
            }
        }),
        &REGRESSION,
    )
}

fn prepare_regression(arguments: &Value) -> Result<Prepared, String> {
    let response = required_text(arguments, "response")?;
    let predictors = column_list(arguments, "predictors", MAX_PREDICTORS)?;
    if predictors.contains(&response) {
        return Err("the response can not also be a predictor".to_owned());
    }
    let interactions = flag(arguments, "interactions", false)?;
    let mut r_args = vec![response.clone(), interactions.to_string()];
    r_args.extend(predictors.iter().cloned());
    Ok(Prepared {
        r_args,
        parameters: json!({
            "response": response, "predictors": predictors, "interactions": interactions
        }),
        names: predictors,
    })
}

/// A model term as R names it (`x1`, `x2:x3`, `(Intercept)`) in the user's column names.
fn term_label(term: &str, names: &[String]) -> Result<String, String> {
    if term == "(Intercept)" {
        return Ok("(Intercept)".to_owned());
    }
    term.split(':')
        .map(|part| {
            part.strip_prefix('x')
                .and_then(|digits| digits.parse::<usize>().ok())
                .and_then(|position| position.checked_sub(1))
                .and_then(|position| names.get(position))
                .cloned()
                .ok_or_else(|| format!("R reported an unknown term: {term}"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join(":"))
}

fn shape_regression(
    values: &BTreeMap<String, String>,
    prepared: &Prepared,
) -> Result<Value, String> {
    let terms = count(values, "terms")?;
    let mut coefficients = Vec::with_capacity(terms);
    let mut aliased = Vec::new();
    for index in 1..=terms {
        let term = term_label(
            values
                .get(&format!("term.{index}"))
                .ok_or_else(|| format!("R did not report term.{index}"))?,
            &prepared.names,
        )?;
        let estimate = figure(values, &format!("estimate.{index}"))?;
        if estimate.is_null() {
            aliased.push(term.clone());
        }
        coefficients.push(json!({
            "term": term,
            "estimate": estimate,
            "std_error": figure(values, &format!("std_error.{index}"))?,
            "t_value": figure(values, &format!("t_value.{index}"))?,
            "p_value": figure(values, &format!("p_value.{index}"))?,
        }));
    }
    Ok(json!({
        "n": figure(values, "n")?,
        "df_residual": figure(values, "df_residual")?,
        "sigma": figure(values, "sigma")?,
        "r_squared": figure(values, "r_squared")?,
        "adj_r_squared": figure(values, "adj_r_squared")?,
        "f_statistic": figure(values, "f_statistic")?,
        "f_p_value": figure(values, "f_p_value")?,
        "coefficients": coefficients,
        "aliased_terms": aliased,
    }))
}

mod ancova;
mod design;
use design::doe_registration;
mod record;

#[cfg(test)]
mod tests;
