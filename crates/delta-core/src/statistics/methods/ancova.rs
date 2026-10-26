//! `statistics_ancova`: test whether a numeric response against a covariate has the same slope
//! (`slopes`) or the same intercept (`intercepts`) in every group (ADR-0073). R fits the full and
//! the reduced linear model and compares them with the nested F test. Both comparisons are nested,
//! so the F value does not depend on the type of sums of squares.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::{choice, count, figure, registration, required_text, text, Method, Prepared};
use crate::capability::CapabilityRegistration;

const TESTS: [&str; 2] = ["slopes", "intercepts"];

/// Arguments: response column, covariate column, group column, test. The group column is read as
/// text and sorted as `statistics_anova` sorts it, so the first group is the reference.
const ANCOVA_SCRIPT: &str = r#"
data <- load_data()
y <- numeric_column(data, args[2])
x <- numeric_column(data, args[3])
g <- as.character(column(data, args[4]))
test <- args[5]
keep <- !is.na(y) & !is.na(x) & !is.na(g) & nzchar(g)
y <- y[keep]
x <- x[keep]
g <- factor(g[keep], levels = sort(unique(g[keep]), method = "radix"))
if (nlevels(g) < 2) stop("the group column needs at least two groups, found ", nlevels(g))
if (test == "slopes") {
  full <- quietly(lm(y ~ x * g))
  reduced <- quietly(lm(y ~ x + g))
} else {
  full <- quietly(lm(y ~ x + g))
  reduced <- quietly(lm(y ~ x))
}
if (full$df.residual <= 0) stop("there are too few rows for this comparison")
comparison <- quietly(anova(reduced, full))
cat("test: ", enc(test), "\n", sep = "")
cat("n: ", num(length(y)), "\n", "groups: ", num(nlevels(g)), "\n", sep = "")
cat("reference: ", enc(levels(g)[1]), "\n", sep = "")
for (i in seq_len(nlevels(g))) {
  cat("group.", i, ": ", enc(levels(g)[i]), "\n",
      "group_n.", i, ": ", num(sum(g == levels(g)[i])), "\n", sep = "")
}
cat("df_full: ", num(full$df.residual), "\n",
    "df_reduced: ", num(reduced$df.residual), "\n",
    "rss_full: ", num(sum(residuals(full)^2)), "\n",
    "rss_reduced: ", num(sum(residuals(reduced)^2)), "\n",
    "df_difference: ", num(comparison[["Df"]][2]), "\n",
    "f_value: ", num(comparison[["F"]][2]), "\n",
    "p_value: ", num(comparison[["Pr(>F)"]][2]), "\n", sep = "")
finish()
"#;

static ANCOVA: Method = Method {
    name: "ancova",
    script: ANCOVA_SCRIPT,
    reads_input: true,
    prepare: prepare_ancova,
    shape: shape_ancova,
};

pub(super) fn ancova_registration() -> CapabilityRegistration {
    registration(
        "statistics.ancova",
        "statistics_ancova",
        "Test whether the slopes or the intercepts of a numeric column against a covariate differ \
         between the groups named by another column of a CSV file in the workspace. `slopes` \
         compares a separate slope per group with one common slope; `intercepts` compares a \
         separate intercept per group with one common line. Returns each group's n, the residual \
         sums of squares and degrees of freedom of both models, and the nested F test (difference \
         in degrees of freedom, F and p value), computed by R. The first group (in alphabetical \
         order) is the reference. The assumptions (linearity, normal and equal-variance residuals) \
         are not checked.",
        json!({
            "type": "object", "required": ["path", "response", "covariate", "group"],
            "properties": {
                "path": {"type": "string", "description": "CSV file path, relative to the workspace."},
                "response": {"type": "string", "description": "The numeric response column."},
                "covariate": {"type": "string", "description": "The numeric covariate, such as time."},
                "group": {"type": "string", "description": "The column that names each row's group, such as the batch."},
                "test": {"type": "string", "enum": TESTS, "description": "What to compare. Default slopes."}
            }
        }),
        &ANCOVA,
    )
}

fn prepare_ancova(arguments: &Value) -> Result<Prepared, String> {
    let response = required_text(arguments, "response")?;
    let covariate = required_text(arguments, "covariate")?;
    let group = required_text(arguments, "group")?;
    let test = choice(arguments, "test", "slopes", &TESTS)?;
    Ok(Prepared {
        r_args: vec![
            response.clone(),
            covariate.clone(),
            group.clone(),
            test.clone(),
        ],
        parameters: json!({
            "response": response, "covariate": covariate, "group": group, "test": test,
        }),
        names: Vec::new(),
    })
}

fn shape_ancova(values: &BTreeMap<String, String>, _: &Prepared) -> Result<Value, String> {
    let groups = count(values, "groups")?;
    let mut table = Vec::with_capacity(groups);
    for index in 1..=groups {
        table.push(json!({
            "label": text(values, &format!("group.{index}"))?,
            "n": figure(values, &format!("group_n.{index}"))?,
        }));
    }
    Ok(json!({
        "test": text(values, "test")?,
        "n": figure(values, "n")?,
        "groups": table,
        "reference": text(values, "reference")?,
        "df_full": figure(values, "df_full")?,
        "df_reduced": figure(values, "df_reduced")?,
        "rss_full": figure(values, "rss_full")?,
        "rss_reduced": figure(values, "rss_reduced")?,
        "df_difference": figure(values, "df_difference")?,
        "f_value": figure(values, "f_value")?,
        "p_value": figure(values, "p_value")?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The data of the reference tests: 3 batches at 6 time points. The expected values come from
    /// statsmodels 0.15 (formulas `assay ~ time*C(batch)` against `assay ~ time + C(batch)`, and
    /// `assay ~ time + C(batch)` against `assay ~ time`), with SciPy 1.18 for the p values.
    const BATCHES: &str = "batch,time,assay\n\
A,0,99.859\nA,3,101.463\nA,6,103.12\nA,9,104.873\nA,12,106.094\nA,18,109.154\n\
B,0,101.345\nB,3,102.354\nB,6,104.135\nB,9,106.5\nB,12,107.282\nB,18,110.742\n\
C,0,99.785\nC,3,101.567\nC,6,104.643\nC,9,106.743\nC,12,108.837\nC,18,114.768\n";

    #[test]
    fn a_test_name_other_than_slopes_or_intercepts_is_refused() {
        let error = prepare_ancova(&json!({
            "path": "b.csv", "response": "assay", "covariate": "time", "group": "batch",
            "test": "interaction",
        }))
        .err()
        .expect("should be refused");
        assert!(error.contains("test"), "{error}");
        let missing = prepare_ancova(&json!({
            "path": "b.csv", "response": "assay", "group": "batch",
        }))
        .err()
        .expect("should be refused");
        assert!(missing.contains("covariate"), "{missing}");
    }

    #[test]
    fn the_tests_default_to_slopes_and_pass_the_names_as_arguments() {
        let prepared = prepare_ancova(&json!({
            "path": "b.csv", "response": "assay", "covariate": "time", "group": "batch",
        }))
        .unwrap();
        assert_eq!(prepared.parameters["test"], "slopes");
        assert_eq!(prepared.r_args, ["assay", "time", "batch", "slopes"]);
    }

    /// Runs the R program on the reference data when the pinned R is there, as the other R tests do.
    fn draw(test: &str) -> Option<Result<Value, String>> {
        let rscript = super::super::super::find_rscript().ok()?;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("batches.csv");
        std::fs::write(&path, BATCHES).unwrap();
        let prepared = prepare_ancova(&json!({
            "path": "batches.csv", "response": "assay", "covariate": "time", "group": "batch",
            "test": test,
        }))
        .unwrap();
        Some(super::super::run(
            &ANCOVA,
            &rscript,
            Some(&path),
            &prepared,
            super::super::METHOD_TIMEOUT,
        ))
    }

    fn close(value: &Value, expected: f64, relative: f64) {
        let found = value.as_f64().unwrap_or(f64::NAN);
        assert!(
            (found - expected).abs() <= relative * expected.abs().max(1e-300),
            "{found} is not {expected}"
        );
    }

    #[test]
    fn the_slopes_test_matches_statsmodels() {
        let Some(result) = draw("slopes") else {
            return;
        };
        let result = result.unwrap();
        assert_eq!(result["reference"], "A");
        assert_eq!(result["n"].as_f64(), Some(18.0));
        assert_eq!(result["df_full"].as_f64(), Some(12.0));
        assert_eq!(result["df_reduced"].as_f64(), Some(14.0));
        assert_eq!(result["df_difference"].as_f64(), Some(2.0));
        close(&result["rss_full"], 1.5052527714285762, 1e-9);
        close(&result["rss_reduced"], 14.570808238095276, 1e-9);
        close(&result["f_value"], 52.07984618131623, 1e-9);
        close(&result["p_value"], 1.2154994236589666e-06, 1e-7);
    }

    #[test]
    fn the_intercepts_test_matches_statsmodels() {
        let Some(result) = draw("intercepts") else {
            return;
        };
        let result = result.unwrap();
        assert_eq!(result["df_full"].as_f64(), Some(14.0));
        assert_eq!(result["df_reduced"].as_f64(), Some(16.0));
        close(&result["rss_full"], 14.570808238095276, 1e-9);
        close(&result["rss_reduced"], 26.538066571428608, 1e-9);
        close(&result["f_value"], 5.749221797752792, 1e-9);
        close(&result["p_value"], 0.015041825424551385, 1e-7);
    }
}
