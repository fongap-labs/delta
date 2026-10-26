//! `statistics_shelf_life`: the time at which a one-sided confidence bound on the mean of a
//! linearly changing property crosses its specification limit (ADR-0074). R fits the linear model
//! (pooled, with one common slope and a separate intercept per batch, or separately per batch),
//! computes the bound from the predicted mean and its standard error, and solves for the crossing
//! time. The attribute's direction and the limit are the caller's; no rule of a particular
//! product or guideline is built in.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::{
    choice, count, figure, number_in, real, registration, required_text, text, Method, Prepared,
};
use crate::capability::CapabilityRegistration;

const MODELS: [&str; 2] = ["pooled", "separate"];
const DIRECTIONS: [&str; 2] = ["decreasing", "increasing"];

/// Arguments: time column, response column, batch column (empty for none), model, direction, the
/// specification limit, and the one-sided confidence level. A decreasing attribute is bounded from
/// below and an increasing one from above; the crossing is where that bound meets the limit.
const SHELF_LIFE_SCRIPT: &str = r#"
data <- load_data()
time <- numeric_column(data, args[2])
y <- numeric_column(data, args[3])
batch_name <- args[4]
model <- args[5]
direction <- args[6]
spec <- as.numeric(args[7])
level <- as.numeric(args[8])
has_batch <- nzchar(batch_name)
keep <- !is.na(time) & !is.na(y)
if (has_batch) {
  b <- as.character(column(data, batch_name))
  keep <- keep & !is.na(b) & nzchar(b)
}
d <- data.frame(t = time[keep], y = y[keep])
sign <- if (direction == "decreasing") 1 else -1
if (has_batch) {
  d$b <- factor(b[keep], levels = sort(unique(b[keep]), method = "radix"))
  labels <- levels(d$b)
} else {
  labels <- "all"
}
if (has_batch && nlevels(d$b) < 1) stop("the batch column has no values")
pooled <- has_batch && model == "pooled"
if (pooled && nlevels(d$b) < 2) stop("a pooled model needs at least two batches")
if (pooled) {
  fits <- list(all = quietly(lm(y ~ t + b, data = d)))
} else if (has_batch) {
  fits <- lapply(labels, function(lab) quietly(lm(y ~ t, data = d[d$b == lab, , drop = FALSE])))
  names(fits) <- labels
} else {
  fits <- list(all = quietly(lm(y ~ t, data = d)))
}
predict_at <- function(fit, lab, tt) {
  nd <- data.frame(t = tt)
  if (!is.null(fit$xlevels$b)) nd$b <- factor(lab, levels = fit$xlevels$b)
  p <- predict(fit, newdata = nd, se.fit = TRUE)
  list(mean = p$fit, se = p$se.fit)
}
limit_at <- function(fit, lab, tt) {
  p <- predict_at(fit, lab, tt)
  q <- qt(level, fit$df.residual)
  p$mean - sign * q * p$se
}
solve_shelf_life <- function(fit, lab) {
  g <- function(tt) sign * (limit_at(fit, lab, tt) - spec)
  lo <- 0
  hi <- 10 * max(d$t)
  if (g(lo) <= 0) return(list(time = NA_real_, extrapolated = NA_real_, note = "the bound is already beyond the limit at time 0"))
  if (g(hi) > 0) return(list(time = NA_real_, extrapolated = NA_real_, note = "no crossing within ten times the observed time range"))
  root <- uniroot(g, c(lo, hi), tol = 1e-10)$root
  list(time = root, extrapolated = as.numeric(root > max(d$t)), note = "")
}
model_name <- if (pooled) "pooled" else if (has_batch) "separate" else "single"
cat("model: ", enc(model_name), "\n", sep = "")
cat("direction: ", enc(direction), "\n", sep = "")
cat("spec_limit: ", num(spec), "\n", "confidence: ", num(level), "\n", sep = "")
cat("n: ", num(nrow(d)), "\n", "groups: ", num(length(labels)), "\n", "max_time: ", num(max(d$t)), "\n", sep = "")
for (i in seq_along(labels)) {
  lab <- labels[i]
  fit <- fits[[if (pooled || !has_batch) "all" else lab]]
  s <- solve_shelf_life(fit, lab)
  n_group <- if (has_batch) sum(d$b == lab) else nrow(d)
  zero <- predict_at(fit, lab, 0)
  cat("group.", i, ": ", enc(lab), "\n",
      "group_n.", i, ": ", num(n_group), "\n",
      "group_df.", i, ": ", num(fit$df.residual), "\n",
      "group_slope.", i, ": ", num(coef(fit)[["t"]]), "\n",
      "group_mean_at_zero.", i, ": ", num(zero$mean), "\n",
      "group_shelf_life.", i, ": ", num(s$time), "\n",
      "group_extrapolated.", i, ": ", num(s$extrapolated), "\n",
      "group_note.", i, ": ", enc(s$note), "\n", sep = "")
}
finish()
"#;

static SHELF_LIFE: Method = Method {
    name: "shelf_life",
    script: SHELF_LIFE_SCRIPT,
    reads_input: true,
    prepare: prepare_shelf_life,
    shape: shape_shelf_life,
};

pub(super) fn shelf_life_registration() -> CapabilityRegistration {
    registration(
        "statistics.shelf_life",
        "statistics_shelf_life",
        "Estimate a shelf life from stability data in a CSV file of the workspace. A linear model of \
         the property against time is fitted, either pooled (one common slope, a separate intercept \
         per batch) or separately for each batch. The one-sided confidence bound on the mean at each \
         time is the mean minus (for a decreasing property) or plus (for an increasing one) the \
         t quantile times its standard error; the shelf life is the time at which that bound crosses \
         the specification limit. Returns, for each group, the slope, the mean at time 0, the shelf \
         life, whether it lies beyond the observed time range (extrapolated), and any note. Computed \
         by R. The model assumptions are not checked, and the choice of pooled or separate models and \
         the confidence level belong to the caller.",
        json!({
            "type": "object", "required": ["path", "time", "response", "direction", "spec_limit"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "time": {"type": "string", "description": "The numeric time column."},
                "response": {"type": "string", "description": "The numeric property column, such as assay or dissolution."},
                "batch": {"type": "string", "description": "The column that names each batch. Leave out for one batch."},
                "model": {"type": "string", "enum": MODELS, "description": "pooled (common slope, a separate intercept per batch) or separate (one line per batch). Default separate. Used only with a batch column."},
                "direction": {"type": "string", "enum": DIRECTIONS, "description": "decreasing: the bound is a lower bound; increasing: an upper bound."},
                "spec_limit": {"type": "number", "description": "The acceptance limit the bound must not cross."},
                "confidence": {"type": "number", "description": "One-sided confidence level, between 0 and 1. Default 0.95."}
            }
        }),
        &SHELF_LIFE,
    )
}

fn prepare_shelf_life(arguments: &Value) -> Result<Prepared, String> {
    let time = required_text(arguments, "time")?;
    let response = required_text(arguments, "response")?;
    let batch = arguments
        .get("batch")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_owned();
    let model = choice(arguments, "model", "separate", &MODELS)?;
    if model == "pooled" && batch.is_empty() {
        return Err("model pooled needs a batch column".to_owned());
    }
    let direction = arguments
        .get("direction")
        .and_then(Value::as_str)
        .ok_or_else(|| "direction is required: decreasing or increasing".to_owned())?;
    if !DIRECTIONS.contains(&direction) {
        return Err("direction must be decreasing or increasing".to_owned());
    }
    let spec = real(arguments, "spec_limit", f64::NAN)
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| "spec_limit is required: a finite number".to_owned())?;
    let confidence = number_in(arguments, "confidence", 0.95, (0.0, 1.0))?;
    let model_for_r = if batch.is_empty() {
        "single".to_owned()
    } else {
        model.clone()
    };
    Ok(Prepared {
        r_args: vec![
            time.clone(),
            response.clone(),
            batch.clone(),
            model_for_r,
            direction.to_owned(),
            spec.to_string(),
            confidence.to_string(),
        ],
        parameters: json!({
            "time": time, "response": response, "batch": batch, "model": model,
            "direction": direction, "spec_limit": spec, "confidence": confidence,
        }),
        names: Vec::new(),
    })
}

fn shape_shelf_life(values: &BTreeMap<String, String>, _: &Prepared) -> Result<Value, String> {
    let groups = count(values, "groups")?;
    let mut table = Vec::with_capacity(groups);
    for index in 1..=groups {
        table.push(json!({
            "label": text(values, &format!("group.{index}"))?,
            "n": figure(values, &format!("group_n.{index}"))?,
            "df_residual": figure(values, &format!("group_df.{index}"))?,
            "slope": figure(values, &format!("group_slope.{index}"))?,
            "mean_at_zero": figure(values, &format!("group_mean_at_zero.{index}"))?,
            "shelf_life": figure(values, &format!("group_shelf_life.{index}"))?,
            "extrapolated": figure(values, &format!("group_extrapolated.{index}"))?,
            "note": text(values, &format!("group_note.{index}"))?,
        }));
    }
    Ok(json!({
        "model": text(values, "model")?,
        "direction": text(values, "direction")?,
        "spec_limit": figure(values, "spec_limit")?,
        "confidence": figure(values, "confidence")?,
        "n": figure(values, "n")?,
        "max_time": figure(values, "max_time")?,
        "groups": table,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arguments_are_checked_before_r_starts() {
        let base = json!({"path": "s.csv", "time": "month", "response": "assay", "direction": "decreasing", "spec_limit": 95.0});
        assert!(prepare_shelf_life(&base).is_ok());
        let mut missing_limit = base.clone();
        missing_limit.as_object_mut().unwrap().remove("spec_limit");
        let error = prepare_shelf_life(&missing_limit).err().expect("refused");
        assert!(error.contains("spec_limit"), "{error}");
        let mut no_direction = base.clone();
        no_direction.as_object_mut().unwrap().remove("direction");
        let error = prepare_shelf_life(&no_direction).err().expect("refused");
        assert!(error.contains("direction"), "{error}");
        let mut wrong_direction = base.clone();
        wrong_direction["direction"] = json!("both");
        assert!(prepare_shelf_life(&wrong_direction).is_err());
        let mut pooled_alone = base.clone();
        pooled_alone["model"] = json!("pooled");
        let error = prepare_shelf_life(&pooled_alone).err().expect("refused");
        assert!(error.contains("batch"), "{error}");
        let mut bad_level = base.clone();
        bad_level["confidence"] = json!(1.5);
        assert!(prepare_shelf_life(&bad_level).is_err());
    }

    #[test]
    fn a_single_batch_is_sent_as_single_and_the_model_is_ignored() {
        let prepared = prepare_shelf_life(&json!({
            "path": "s.csv", "time": "month", "response": "assay", "direction": "increasing",
            "spec_limit": 110.0, "model": "separate",
        }))
        .unwrap();
        assert_eq!(prepared.r_args[3], "single");
        assert_eq!(prepared.r_args[5], "110");
        assert_eq!(prepared.r_args[6], "0.95");
    }

    /// The stability data of the reference tests: 3 batches at 6 time points. The expected shelf
    /// lives come from statsmodels 0.15 (the mean and its standard error from `get_prediction`) and
    /// SciPy 1.18 (the one-sided t quantile at 0.95 and the root of the bound against the limit
    /// 110, which is an increasing attribute).
    const BATCHES: &str = "batch,time,assay\n\
A,0,99.859\nA,3,101.463\nA,6,103.12\nA,9,104.873\nA,12,106.094\nA,18,109.154\n\
B,0,101.345\nB,3,102.354\nB,6,104.135\nB,9,106.5\nB,12,107.282\nB,18,110.742\n\
C,0,99.785\nC,3,101.567\nC,6,104.643\nC,9,106.743\nC,12,108.837\nC,18,114.768\n";

    fn draw(model: &str) -> Option<Result<Value, String>> {
        let rscript = super::super::super::find_rscript().ok()?;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stability.csv");
        std::fs::write(&path, BATCHES).unwrap();
        let prepared = prepare_shelf_life(&json!({
            "path": "stability.csv", "time": "time", "response": "assay", "batch": "batch",
            "model": model, "direction": "increasing", "spec_limit": 110.0,
        }))
        .unwrap();
        Some(super::super::run(
            &SHELF_LIFE,
            &rscript,
            Some(&path),
            &prepared,
            super::super::METHOD_TIMEOUT,
        ))
    }

    fn close(value: &Value, expected: f64) {
        let found = value.as_f64().unwrap_or(f64::NAN);
        assert!(
            (found - expected).abs() <= 1e-6 * expected.abs(),
            "{found} is not {expected}"
        );
    }

    #[test]
    fn the_pooled_model_gives_the_shelf_life_of_each_batch() {
        let Some(result) = draw("pooled") else {
            return;
        };
        let result = result.unwrap();
        assert_eq!(result["model"], "pooled");
        let groups = result["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 3);
        close(&groups[0]["slope"], 0.6269523809523885);
        close(&groups[0]["df_residual"], 14.0);
        close(&groups[0]["shelf_life"], 15.940225311362921);
        close(&groups[1]["shelf_life"], 13.99279029696868);
        close(&groups[2]["shelf_life"], 12.987599781776524);
        assert_eq!(groups[0]["extrapolated"].as_f64(), Some(0.0));
    }

    #[test]
    fn the_separate_model_gives_each_batch_its_own_line() {
        let Some(result) = draw("separate") else {
            return;
        };
        let result = result.unwrap();
        let groups = result["groups"].as_array().unwrap();
        close(&groups[0]["slope"], 0.5160095238095233);
        close(&groups[0]["df_residual"], 4.0);
        close(&groups[0]["shelf_life"], 18.885480677681215);
        // Batch A's shelf life lies beyond the last observed time (18), so it is flagged.
        assert_eq!(groups[0]["extrapolated"].as_f64(), Some(1.0));
        close(&groups[1]["shelf_life"], 15.5837457856615);
        close(&groups[2]["shelf_life"], 12.177266760604144);
    }
}
