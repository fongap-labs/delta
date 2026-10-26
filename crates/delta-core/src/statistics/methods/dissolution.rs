//! `statistics_f2`: the similarity factor f2 between two dissolution profiles (ADR-0078). The
//! caller gives the mean percent dissolved of the reference and the test product at the same time
//! points; R computes f2 = 50 · log10(100 / sqrt(1 + mean squared difference)). f2 is 100 for
//! identical profiles and falls as they diverge; 50 or more is the usual similarity limit.
//! The guidance conditions (at least three time points, at most one mean above 85 % per product,
//! variability limits on individual units) are reported, not enforced: the caller judges them.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::{figure, registration, required_text, Method, Prepared};
use crate::capability::CapabilityRegistration;

/// Arguments: reference column, test column. Rows with a missing value in either column are
/// dropped, so every time point that is compared has both values.
const F2_SCRIPT: &str = r#"
data <- load_data()
reference <- numeric_column(data, args[2])
test <- numeric_column(data, args[3])
keep <- !is.na(reference) & !is.na(test)
reference <- reference[keep]
test <- test[keep]
n <- length(reference)
if (n < 1) stop("no time point has both a reference and a test value")
msd <- sum((reference - test)^2) / n
f2 <- 50 * log10(100 / sqrt(1 + msd))
cat("points: ", num(n), "\n",
    "mean_squared_difference: ", num(msd), "\n",
    "max_absolute_difference: ", num(max(abs(reference - test))), "\n",
    "f2: ", num(f2), "\n",
    "reference_above_85: ", num(sum(reference > 85)), "\n",
    "test_above_85: ", num(sum(test > 85)), "\n", sep = "")
finish()
"#;

static F2: Method = Method {
    name: "f2",
    script: F2_SCRIPT,
    reads_input: true,
    prepare: prepare_f2,
    shape: shape_f2,
};

pub(super) fn f2_registration() -> CapabilityRegistration {
    registration(
        "statistics.f2",
        "statistics_f2",
        "Similarity factor f2 between the dissolution profiles of a reference and a test product, \
         from a CSV file in the workspace with one row per time point. The columns hold the mean \
         percent dissolved of each product at that time point. Returns f2 (50 or more is the usual \
         similarity limit), the number of time points, the mean squared difference, the largest \
         difference, and how many means of each product are above 85 %. Computed by R. The \
         guidance conditions (at least three time points, at most one mean above 85 % per product, \
         unit variability) are reported, not checked; the caller decides whether they apply.",
        json!({
            "type": "object", "required": ["path", "reference", "test"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "reference": {"type": "string", "description": "Column with the reference product's mean percent dissolved per time point."},
                "test": {"type": "string", "description": "Column with the test product's mean percent dissolved, same time points and order."}
            }
        }),
        &F2,
    )
}

fn prepare_f2(arguments: &Value) -> Result<Prepared, String> {
    let reference = required_text(arguments, "reference")?;
    let test = required_text(arguments, "test")?;
    Ok(Prepared {
        r_args: vec![reference.clone(), test.clone()],
        parameters: json!({"reference": reference, "test": test}),
        names: Vec::new(),
    })
}

fn shape_f2(values: &BTreeMap<String, String>, _: &Prepared) -> Result<Value, String> {
    let points = figure(values, "points")?;
    let f2 = figure(values, "f2")?;
    let note = if points.as_f64().unwrap_or(0.0) < 3.0 {
        "fewer than three time points: the usual f2 guidance does not apply"
    } else {
        "guidance conditions are not checked here; see the tool description"
    };
    Ok(json!({
        "f2": f2,
        "points": points,
        "mean_squared_difference": figure(values, "mean_squared_difference")?,
        "max_absolute_difference": figure(values, "max_absolute_difference")?,
        "reference_above_85": figure(values, "reference_above_85")?,
        "test_above_85": figure(values, "test_above_85")?,
        "note": note,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_columns_are_required() {
        let missing = prepare_f2(&json!({"path": "p.csv", "reference": "ref"}))
            .err()
            .expect("should be refused");
        assert!(missing.contains("test"), "{missing}");
    }

    #[test]
    fn the_columns_are_passed_as_arguments_in_order() {
        let prepared = prepare_f2(&json!({
            "path": "p.csv", "reference": "ref", "test": "tst",
        }))
        .unwrap();
        assert_eq!(prepared.r_args, ["ref", "tst"]);
    }

    /// Runs the R program on a small profile when the pinned R is there, as the other R tests do.
    /// The expected value is the closed form computed independently for R = (10, 40, 70) and
    /// T = (12, 38, 75): msd = 11, f2 = 50 · log10(100 / sqrt(12)) = 73.02046884880939.
    #[test]
    fn f2_matches_the_closed_form() {
        let Ok(rscript) = super::super::super::find_rscript() else {
            return;
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profiles.csv");
        std::fs::write(&path, "time,reference,test\n10,10,12\n20,40,38\n30,70,75\n").unwrap();
        let prepared = prepare_f2(&json!({
            "path": "profiles.csv", "reference": "reference", "test": "test",
        }))
        .unwrap();
        let result = super::super::run(
            &F2,
            &rscript,
            Some(&path),
            &prepared,
            super::super::METHOD_TIMEOUT,
        )
        .unwrap();
        let f2 = result["f2"].as_f64().unwrap();
        assert!((f2 - 73.02046884880939).abs() <= 1e-9, "{f2}");
        assert_eq!(result["points"].as_f64(), Some(3.0));
        assert_eq!(result["mean_squared_difference"].as_f64(), Some(11.0));
        assert_eq!(result["max_absolute_difference"].as_f64(), Some(5.0));
    }
}
