use super::*;

/// 12 rows. The expected figures below were computed independently with SciPy 1.18 and
/// statsmodels 0.15 (ADR-0054 point 6), not with R.
const STUDY: &str = "y,post,x1,x2,g2,g3,c1,c2\n\
5.1,5.4,1,3,控制,a,u,p\n4.9,5.0,2,1,控制,b,u,p\n6.2,6.7,3,4,控制,c,v,q\n\
5.8,6.0,4,1,控制,a,v,q\n7.1,7.5,5,5,控制,b,u,p\n6.9,7.5,6,9,控制,c,v,q\n\
8.3,8.4,7,2,处理,a,u,q\n7.7,8.0,8,6,处理,b,v,q\n9.2,9.4,9,5,处理,c,u,p\n\
8.8,9.3,10,3,处理,a,v,q\n10.1,10.5,11,5,处理,b,u,p\n9.6,9.8,12,8,处理,c,v,q\n";

fn rscript_available() -> bool {
    std::process::Command::new("Rscript")
        .arg("--version")
        .output()
        .is_ok()
}

/// Run a method on `csv` with the Rscript on PATH, bypassing the version pin (the figures do not
/// depend on the R release). `None` when R is not installed.
fn call(
    method: &'static Method,
    csv: Option<&str>,
    arguments: Value,
) -> Option<Result<Value, String>> {
    if !rscript_available() {
        eprintln!("skipped: Rscript is not installed");
        return None;
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data.csv");
    if let Some(csv) = csv {
        std::fs::write(&path, csv).unwrap();
    }
    let prepared = match (method.prepare)(&arguments) {
        Ok(prepared) => prepared,
        Err(error) => return Some(Err(error)),
    };
    Some(run(
        method,
        Path::new("Rscript"),
        csv.map(|_| path.as_path()),
        &prepared,
        Duration::from_secs(60),
    ))
}

fn close(actual: &Value, expected: f64, tolerance: f64) {
    let actual = actual
        .as_f64()
        .unwrap_or_else(|| panic!("expected a number near {expected}, got {actual}"));
    let allowed = tolerance * expected.abs().max(1e-300) + 1e-15;
    assert!(
        (actual - expected).abs() <= allowed,
        "{actual} is not {expected} within {tolerance}"
    );
}

#[test]
fn every_method_is_registered_as_a_tool_the_product_offers() {
    let host = crate::capability::CapabilityHost::product_defaults().unwrap();
    let names: Vec<String> = host
        .tool_schemas()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_owned))
        .collect();
    for tool in [
        "statistics_ttest",
        "statistics_anova",
        "statistics_chisq",
        "statistics_correlation",
        "statistics_regression",
        "statistics_doe",
    ] {
        assert!(names.iter().any(|name| name == tool), "{tool}: {names:?}");
    }
}

#[test]
fn methods_that_read_a_file_are_reads_and_the_design_generator_has_no_side_effect() {
    for registration in registrations() {
        // statistics_record writes a result file, so it is the one that asks (see record.rs).
        if registration.tool_name == "statistics_record" {
            continue;
        }
        let expected = if registration.tool_name == "statistics_doe" {
            SideEffectClass::None
        } else {
            SideEffectClass::Read
        };
        assert_eq!(
            registration.side_effect, expected,
            "{}",
            registration.tool_name
        );
        assert_eq!(registration.metadata["requires_approval"], false);
        assert!(!registration.workspace_write);
    }
}

#[test]
fn text_from_r_is_percent_decoded_and_malformed_text_is_an_error() {
    assert_eq!(percent_decode("a%20b").unwrap(), "a b");
    assert_eq!(percent_decode("%E6%8E%A7%E5%88%B6").unwrap(), "控制");
    assert_eq!(percent_decode("%3A%25").unwrap(), ":%");
    assert!(percent_decode("%4").is_err());
    assert!(percent_decode("%zz").is_err());
    assert!(percent_decode("%FF").is_err());
}

#[test]
fn figures_that_are_not_numbers_read_as_null_and_garbage_is_an_error() {
    let values = parse_dcf("a: 1.5\nb: NA\nc: NaN\nd: Inf\ne: nope\n").unwrap();
    assert_eq!(figure(&values, "a").unwrap(), json!(1.5));
    for key in ["b", "c", "d"] {
        assert!(figure(&values, key).unwrap().is_null(), "{key}");
    }
    assert!(figure(&values, "e").is_err());
    assert!(figure(&values, "missing").is_err());
}

#[test]
fn model_terms_are_reported_in_the_users_column_names() {
    let names = vec!["温度".to_owned(), "压力".to_owned()];
    assert_eq!(term_label("(Intercept)", &names).unwrap(), "(Intercept)");
    assert_eq!(term_label("x2", &names).unwrap(), "压力");
    assert_eq!(term_label("x1:x2", &names).unwrap(), "温度:压力");
    assert!(term_label("x3", &names).is_err());
    assert!(term_label("x0", &names).is_err());
    assert!(term_label("temperature", &names).is_err());
}

#[test]
fn arguments_that_could_not_be_run_are_refused_before_r_starts() {
    let refused = |result: Result<Prepared, String>, needle: &str| {
        let error = result.err().expect("should be refused");
        assert!(error.contains(needle), "{error}");
    };
    refused(prepare_ttest(&json!({"column": "y"})), "kind");
    refused(
        prepare_ttest(&json!({"kind": "two_sample", "column": "y"})),
        "group",
    );
    refused(
        prepare_ttest(&json!({"kind": "paired", "column": "y"})),
        "column_b",
    );
    refused(
        prepare_ttest(&json!({"kind": "one_sample", "column": "y", "conf_level": 1})),
        "conf_level",
    );
    refused(
        prepare_ttest(&json!({"kind": "one_sample", "column": "y", "alternative": "both"})),
        "alternative",
    );
    refused(
        prepare_ttest(&json!({"kind": "one_sample", "column": "y", "mu": "zero"})),
        "mu",
    );
    refused(prepare_anova(&json!({"response": "y"})), "group");
    refused(prepare_chisq(&json!({"column_a": "a"})), "column_b");
    refused(
        prepare_correlation(&json!({"column_a": "a", "column_b": "b", "method": "cosine"})),
        "method",
    );
    refused(prepare_regression(&json!({"response": "y"})), "predictors");
    refused(
        prepare_regression(&json!({"response": "y", "predictors": []})),
        "predictors",
    );
    refused(
        prepare_regression(&json!({"response": "y", "predictors": ["a", "a"]})),
        "twice",
    );
    refused(
        prepare_regression(&json!({"response": "y", "predictors": ["y"]})),
        "response",
    );
    let nine: Vec<String> = (0..9).map(|n| format!("x{n}")).collect();
    refused(
        prepare_regression(&json!({"response": "y", "predictors": nine})),
        "predictors",
    );
}

#[test]
fn a_column_name_reaches_r_as_an_argument_and_never_as_code() {
    let prepared = prepare_regression(&json!({
        "response": "y\"); system(\"echo hi\"); (\"",
        "predictors": ["x1 + x2", "-1"],
    }))
    .unwrap();
    assert!(prepared.r_args.iter().any(|arg| arg.contains("system(")));
    // The program text is fixed: it holds no column name.
    assert!(!format!("{PRELUDE}{REGRESSION_SCRIPT}").contains("system("));
}

#[test]
fn design_arguments_are_checked() {
    let factor = |name: &str, low: f64, high: f64| json!({"name": name, "low": low, "high": high});
    let refused = |arguments: Value, needle: &str| {
        let error = prepare_doe(&arguments).err().expect("should be refused");
        assert!(error.contains(needle), "{error}");
    };
    refused(json!({}), "factors");
    refused(
        json!({"factors": [factor("a", 0.0, 1.0)]}),
        "between 2 and 8",
    );
    refused(
        json!({"factors": [factor("a", 0.0, 1.0), factor("a", 0.0, 1.0)]}),
        "twice",
    );
    refused(
        json!({"factors": [factor("a", 1.0, 1.0), factor("b", 0.0, 1.0)]}),
        "low below high",
    );
    refused(
        json!({"factors": [factor("a", 0.0, 1.0), factor("b", 0.0, 1.0)], "replicates": 0}),
        "replicates",
    );
    refused(
        json!({"factors": [factor("a", 0.0, 1.0), factor("b", 0.0, 1.0)], "center_points": 11}),
        "center_points",
    );
    let many: Vec<Value> = (0..8).map(|n| factor(&format!("f{n}"), 0.0, 1.0)).collect();
    refused(json!({"factors": many, "replicates": 5}), "1024");
    let seeded = prepare_doe(&json!({
        "factors": [factor("a", 0.0, 1.0), factor("b", 0.0, 1.0)], "seed": 7
    }))
    .unwrap();
    assert_eq!(seeded.parameters["seed"], 7);
    let unseeded = prepare_doe(&json!({
        "factors": [factor("a", 0.0, 1.0), factor("b", 0.0, 1.0)]
    }))
    .unwrap();
    assert!(unseeded.parameters["seed"].as_u64().is_some());
}

#[test]
fn one_sample_t_test_matches_scipy() {
    let Some(result) = call(
        &TTEST,
        Some(STUDY),
        json!({"path": "data.csv", "kind": "one_sample", "column": "y", "mu": 5}),
    ) else {
        return;
    };
    let result = result.unwrap();
    close(&result["statistic"], 4.873617552725215, 1e-9);
    close(&result["df"], 11.0, 1e-12);
    close(&result["p_value"], 0.0004918624151844586, 1e-7);
    close(&result["conf_low"], 6.3572597973079095, 1e-9);
    close(&result["conf_high"], 8.592740202692088, 1e-9);
    close(&result["estimate"], 7.475, 1e-12);
    close(&result["n"], 12.0, 1e-12);
    assert_eq!(result["provenance"]["method"], "ttest");
    assert_eq!(result["provenance"]["parameters"]["mu"], 5.0);
}

#[test]
fn two_sample_t_test_matches_scipy_and_reports_the_groups_by_their_labels() {
    let Some(welch) = call(
        &TTEST,
        Some(STUDY),
        json!({"path": "data.csv", "kind": "two_sample", "column": "y", "group": "g2"}),
    ) else {
        return;
    };
    let welch = welch.unwrap();
    close(&welch["statistic"], 5.736000464021203, 1e-9);
    close(&welch["df"], 9.98524753299581, 1e-9);
    close(&welch["p_value"], 0.00018981422508811675, 1e-7);
    close(&welch["conf_low"], 1.8038483875047215, 1e-9);
    close(&welch["conf_high"], 4.09615161249528, 1e-9);
    close(&welch["estimate"], 2.95, 1e-9);
    // Groups are ordered by their bytes, so 处理 comes before 控制 whatever the locale.
    assert_eq!(welch["groups"][0]["label"], "处理");
    assert_eq!(welch["groups"][1]["label"], "控制");
    close(&welch["groups"][0]["n"], 6.0, 1e-12);
    assert!(welch["method"].as_str().unwrap().contains("Welch"));

    let student = call(
        &TTEST,
        Some(STUDY),
        json!({
            "path": "data.csv", "kind": "two_sample", "column": "y", "group": "g2",
            "equal_variance": true
        }),
    )
    .unwrap()
    .unwrap();
    close(&student["statistic"], 5.7360004640212034, 1e-9);
    close(&student["df"], 10.0, 1e-12);
    close(&student["p_value"], 0.00018874982290683078, 1e-7);
    assert!(!student["method"].as_str().unwrap().contains("Welch"));
}

#[test]
fn a_two_sample_test_needs_exactly_two_groups() {
    let Some(result) = call(
        &TTEST,
        Some(STUDY),
        json!({"path": "data.csv", "kind": "two_sample", "column": "y", "group": "g3"}),
    ) else {
        return;
    };
    let error = result.unwrap_err();
    assert!(error.contains("exactly two groups, found 3"), "{error}");
}

#[test]
fn paired_t_test_matches_scipy() {
    let Some(result) = call(
        &TTEST,
        Some(STUDY),
        json!({"path": "data.csv", "kind": "paired", "column": "y", "column_b": "post"}),
    ) else {
        return;
    };
    let result = result.unwrap();
    close(&result["statistic"], -6.679668968451189, 1e-6);
    close(&result["df"], 11.0, 1e-12);
    close(&result["p_value"], 3.469658369731304e-05, 1e-4);
    close(&result["estimate"], -0.3166666666666665, 1e-6);
    close(&result["n"], 12.0, 1e-12);
}

#[test]
fn a_one_sided_test_and_a_confidence_level_are_honoured() {
    let Some(result) = call(
        &TTEST,
        Some(STUDY),
        json!({
            "path": "data.csv", "kind": "one_sample", "column": "y", "mu": 5,
            "alternative": "greater", "conf_level": 0.9
        }),
    ) else {
        return;
    };
    let result = result.unwrap();
    // One-sided p is half the two-sided p when the statistic is positive.
    close(&result["p_value"], 0.0004918624151844586 / 2.0, 1e-6);
    assert_eq!(result["alternative"], "greater");
    close(&result["conf_level"], 0.9, 1e-12);
    // A one-sided interval is open on one side: R reports Inf, which is returned as null.
    assert!(result["conf_high"].is_null());
    // Its lower bound at 90% is above the two-sided 95% bound.
    assert!(result["conf_low"].as_f64().unwrap() > 6.3572597973079095);
}

#[test]
fn one_way_anova_matches_scipy_and_hand_computed_sums_of_squares() {
    let Some(result) = call(
        &ANOVA,
        Some(STUDY),
        json!({"path": "data.csv", "response": "y", "group": "g3"}),
    ) else {
        return;
    };
    let result = result.unwrap();
    close(&result["f_value"], 0.2667444574095681, 1e-9);
    close(&result["p_value"], 0.771716695221694, 1e-8);
    close(&result["ss_between"], 1.904999999999999, 1e-9);
    close(&result["ss_within"], 32.1375, 1e-9);
    close(&result["df_between"], 2.0, 1e-12);
    close(&result["df_within"], 9.0, 1e-12);
    close(&result["ms_between"], 1.904999999999999 / 2.0, 1e-9);
    close(&result["ms_within"], 32.1375 / 9.0, 1e-9);
    let groups = result["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0]["label"], "a");
    for (group, (mean, sd)) in groups.iter().zip([
        (7.0, 1.823915202707261),
        (7.449999999999999, 2.1377558326431947),
        (7.975, 1.6780444968275818),
    ]) {
        close(&group["n"], 4.0, 1e-12);
        close(&group["mean"], mean, 1e-9);
        close(&group["sd"], sd, 1e-9);
    }
}

#[test]
fn anova_refuses_what_it_cannot_estimate() {
    let Some(one_group) = call(
        &ANOVA,
        Some("y,g\n1,a\n2,a\n3,a\n"),
        json!({"path": "data.csv", "response": "y", "group": "g"}),
    ) else {
        return;
    };
    assert!(one_group.unwrap_err().contains("at least two groups"));
    let singles = call(
        &ANOVA,
        Some("y,g\n1,a\n2,b\n3,c\n"),
        json!({"path": "data.csv", "response": "y", "group": "g"}),
    )
    .unwrap();
    assert!(singles.unwrap_err().contains("one observation"));
}

#[test]
fn chi_squared_matches_scipy_with_and_without_the_continuity_correction() {
    let Some(corrected) = call(
        &CHISQ,
        Some(STUDY),
        json!({"path": "data.csv", "column_a": "c1", "column_b": "c2"}),
    ) else {
        return;
    };
    let corrected = corrected.unwrap();
    close(&corrected["statistic"], 5.485714285714286, 1e-9);
    close(&corrected["p_value"], 0.019172484755223134, 1e-7);
    close(&corrected["df"], 1.0, 1e-12);
    close(&corrected["min_expected"], 2.5, 1e-12);
    close(&corrected["n"], 12.0, 1e-12);
    // An expected count under 5 makes the approximation unreliable, and R says so.
    let warnings = corrected["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("approximation")),
        "{warnings:?}"
    );

    let plain = call(
        &CHISQ,
        Some(STUDY),
        json!({"path": "data.csv", "column_a": "c1", "column_b": "c2", "correct": false}),
    )
    .unwrap()
    .unwrap();
    close(&plain["statistic"], 8.571428571428571, 1e-9);
    close(&plain["p_value"], 0.0034147911781178208, 1e-7);
}

#[test]
fn correlation_matches_scipy_for_each_method() {
    let Some(pearson) = call(
        &CORRELATION,
        Some(STUDY),
        json!({"path": "data.csv", "column_a": "x1", "column_b": "y"}),
    ) else {
        return;
    };
    let pearson = pearson.unwrap();
    close(&pearson["estimate"], 0.9652923363647664, 1e-9);
    close(&pearson["p_value"], 3.7418627646684637e-07, 1e-6);
    close(&pearson["conf_low"], 0.8775238893777706, 1e-8);
    close(&pearson["conf_high"], 0.9904832414870861, 1e-8);
    assert_eq!(pearson["statistic_name"], "t");

    let spearman = call(
        &CORRELATION,
        Some(STUDY),
        json!({"path": "data.csv", "column_a": "x1", "column_b": "y", "method": "spearman"}),
    )
    .unwrap()
    .unwrap();
    close(&spearman["estimate"], 0.9580419580419581, 1e-9);
    // R's exact p value and SciPy's t approximation differ a little; both are far below 1e-4.
    assert!(spearman["p_value"].as_f64().unwrap() < 1e-4);
    assert!(spearman["conf_low"].is_null());

    let kendall = call(
        &CORRELATION,
        Some(STUDY),
        json!({"path": "data.csv", "column_a": "x1", "column_b": "y", "method": "kendall"}),
    )
    .unwrap()
    .unwrap();
    close(&kendall["estimate"], 0.8181818181818181, 1e-9);
    close(&kendall["p_value"], 4.4129288920955584e-05, 1e-4);
}

#[test]
fn ties_make_r_warn_and_the_warning_is_returned() {
    let Some(result) = call(
        &CORRELATION,
        Some("a,b\n1,1\n1,2\n2,3\n2,3\n3,5\n4,4\n"),
        json!({"path": "data.csv", "column_a": "a", "column_b": "b", "method": "spearman"}),
    ) else {
        return;
    };
    let result = result.unwrap();
    let warnings = result["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("ties")),
        "{warnings:?}"
    );
}

#[test]
fn regression_matches_statsmodels() {
    let Some(result) = call(
        &REGRESSION,
        Some(STUDY),
        json!({"path": "data.csv", "response": "y", "predictors": ["x1", "x2"]}),
    ) else {
        return;
    };
    let result = result.unwrap();
    let coefficients = result["coefficients"].as_array().unwrap();
    assert_eq!(coefficients.len(), 3);
    let expected = [
        (
            "(Intercept)",
            4.465901122019636,
            0.34144556975369605,
            13.079393957992023,
            3.685151056406324e-07,
        ),
        (
            "x1",
            0.47984747545582057,
            0.048600478717291624,
            9.87330759120887,
            3.978465247897155e-06,
        ),
        (
            "x2",
            -0.025363779803646458,
            0.06913552461879229,
            -0.3668704322922303,
            0.7221930696115424,
        ),
    ];
    for (row, (term, estimate, error, t, p)) in coefficients.iter().zip(expected) {
        assert_eq!(row["term"], term);
        close(&row["estimate"], estimate, 1e-9);
        close(&row["std_error"], error, 1e-9);
        close(&row["t_value"], t, 1e-9);
        close(&row["p_value"], p, 1e-6);
    }
    close(&result["r_squared"], 0.9327943470752311, 1e-9);
    close(&result["adj_r_squared"], 0.9178597575363936, 1e-9);
    close(&result["sigma"], 0.5041878441493192, 1e-9);
    close(&result["f_statistic"], 62.458653091837, 1e-9);
    close(&result["f_p_value"], 5.288422221482126e-06, 1e-6);
    close(&result["df_residual"], 9.0, 1e-12);
    close(&result["n"], 12.0, 1e-12);
    assert!(result["aliased_terms"].as_array().unwrap().is_empty());
}

#[test]
fn regression_with_interactions_matches_statsmodels_and_names_terms_by_column() {
    let csv = "重量 (g),温度: ℃,产量\n\
1,3,5.1\n2,1,4.9\n3,4,6.2\n4,1,5.8\n5,5,7.1\n6,9,6.9\n\
7,2,8.3\n8,6,7.7\n9,5,9.2\n10,3,8.8\n11,5,10.1\n12,8,9.6\n";
    let Some(result) = call(
        &REGRESSION,
        Some(csv),
        json!({
            "path": "data.csv", "response": "产量",
            "predictors": ["重量 (g)", "温度: ℃"], "interactions": true
        }),
    ) else {
        return;
    };
    let result = result.unwrap();
    let coefficients = result["coefficients"].as_array().unwrap();
    let terms: Vec<&str> = coefficients
        .iter()
        .map(|row| row["term"].as_str().unwrap())
        .collect();
    assert_eq!(
        terms,
        ["(Intercept)", "重量 (g)", "温度: ℃", "重量 (g):温度: ℃"]
    );
    for (row, (estimate, error, p)) in coefficients.iter().zip([
        (
            3.843478479122833,
            0.5628212708550556,
            0.00013381955035748943,
        ),
        (
            0.5942405476688524,
            0.09622220971772323,
            0.00026639542985308515,
        ),
        (0.1471206281750409, 0.14321366415123618, 0.3343439661096598),
        (
            -0.02686282766171816,
            0.019785757004538274,
            0.21161763822449592,
        ),
    ]) {
        close(&row["estimate"], estimate, 1e-8);
        close(&row["std_error"], error, 1e-8);
        close(&row["p_value"], p, 1e-6);
    }
    close(&result["r_squared"], 0.9453796247522599, 1e-9);
}

#[test]
fn a_collinear_predictor_is_reported_as_aliased_not_invented() {
    let csv = "y,a,b\n1,1,2\n3,2,4\n2,3,6\n5,4,8\n4,5,10\n7,6,12\n";
    let Some(result) = call(
        &REGRESSION,
        Some(csv),
        json!({"path": "data.csv", "response": "y", "predictors": ["a", "b"]}),
    ) else {
        return;
    };
    let result = result.unwrap();
    assert_eq!(result["aliased_terms"], json!(["b"]));
    let b = &result["coefficients"][2];
    assert_eq!(b["term"], "b");
    assert!(b["estimate"].is_null() && b["p_value"].is_null());
}

#[test]
fn rows_with_a_missing_or_non_numeric_value_are_left_out_and_counted() {
    let csv = "y,x\n1,1\n2,2\n3,3\n4,\n5,oops\n6,6\n8,8\n";
    let Some(result) = call(
        &REGRESSION,
        Some(csv),
        json!({"path": "data.csv", "response": "y", "predictors": ["x"]}),
    ) else {
        return;
    };
    close(&result.unwrap()["n"], 5.0, 1e-12);
}

#[test]
fn a_missing_column_is_an_error_that_names_it() {
    let Some(result) = call(
        &CORRELATION,
        Some(STUDY),
        json!({"path": "data.csv", "column_a": "x1", "column_b": "nope"}),
    ) else {
        return;
    };
    assert!(result.unwrap_err().contains("column not found: nope"));
}

#[test]
fn a_result_carries_the_hash_of_the_exact_program_and_of_the_input() {
    let Some(result) = call(
        &CORRELATION,
        Some(STUDY),
        json!({"path": "data.csv", "column_a": "x1", "column_b": "y"}),
    ) else {
        return;
    };
    let provenance = &result.unwrap()["provenance"];
    assert_eq!(provenance["engine"], "R");
    assert_eq!(
        provenance["script_sha256"],
        sha256_hex(format!("{PRELUDE}{CORRELATION_SCRIPT}").as_bytes())
    );
    assert_eq!(provenance["input_sha256"], sha256_hex(STUDY.as_bytes()));
    assert_eq!(provenance["method"], "correlation");
    assert!(!provenance["r_version"].as_str().unwrap().is_empty());
    assert_eq!(provenance["packages"], json!(["base", "stats", "utils"]));
    assert_eq!(provenance["parameters"]["method"], "pearson");
}

fn factors() -> Value {
    json!([
        {"name": "温度", "low": 20, "high": 40},
        {"name": "pH", "low": 5, "high": 9}
    ])
}

#[test]
fn an_unrandomized_design_lists_the_corners_with_the_first_factor_changing_fastest() {
    let Some(result) = call(
        &DOE,
        None,
        json!({"factors": factors(), "randomize": false, "center_points": 1}),
    ) else {
        return;
    };
    let result = result.unwrap();
    assert_eq!(result["factors"], json!(["温度", "pH"]));
    let runs = result["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 5);
    let coded: Vec<Value> = runs.iter().map(|run| run["coded"].clone()).collect();
    assert_eq!(
        coded,
        [
            json!([-1.0, -1.0]),
            json!([1.0, -1.0]),
            json!([-1.0, 1.0]),
            json!([1.0, 1.0]),
            json!([0.0, 0.0])
        ]
    );
    assert_eq!(runs[0]["actual"], json!([20.0, 5.0]));
    assert_eq!(runs[3]["actual"], json!([40.0, 9.0]));
    assert_eq!(runs[4]["actual"], json!([30.0, 7.0]));
    assert_eq!(runs[4]["point_type"], "center");
    assert_eq!(runs[0]["point_type"], "corner");
    for (position, run) in runs.iter().enumerate() {
        assert_eq!(run["run_order"], position + 1);
        assert_eq!(run["standard_order"], position + 1);
    }
    assert_eq!(result["randomized"], false);
    assert!(result["provenance"]["input_sha256"].is_null());
}

#[test]
fn a_randomized_design_is_a_reproducible_permutation_of_every_run() {
    let arguments = json!({
        "factors": factors(), "replicates": 3, "center_points": 2, "seed": 20260101
    });
    let Some(first) = call(&DOE, None, arguments.clone()) else {
        return;
    };
    let first = first.unwrap();
    let second = call(&DOE, None, arguments).unwrap().unwrap();
    assert_eq!(first["runs"], second["runs"]);
    assert_eq!(first["seed"], 20260101);
    let runs = first["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 4 * 3 + 2);
    let mut standard: Vec<u64> = runs
        .iter()
        .map(|run| run["standard_order"].as_u64().unwrap())
        .collect();
    standard.sort_unstable();
    assert_eq!(standard, (1..=14).collect::<Vec<u64>>());
    let corner_runs = runs
        .iter()
        .filter(|run| run["point_type"] == "corner")
        .count();
    assert_eq!(corner_runs, 12);
    // Each corner appears once per replicate.
    for corner in [
        json!([-1.0, -1.0]),
        json!([1.0, -1.0]),
        json!([-1.0, 1.0]),
        json!([1.0, 1.0]),
    ] {
        let seen = runs.iter().filter(|run| run["coded"] == corner).count();
        assert_eq!(seen, 3, "{corner}");
    }
    let other = call(
        &DOE,
        None,
        json!({"factors": factors(), "replicates": 3, "center_points": 2, "seed": 1}),
    )
    .unwrap()
    .unwrap();
    assert_ne!(first["runs"], other["runs"]);
}

#[test]
fn a_design_analysed_with_regression_recovers_the_effects() {
    // y = 10 + 3a + 2b + 1ab on the coded corners, so the coefficients must be exact.
    let Some(design) = call(
        &DOE,
        None,
        json!({"factors": factors(), "randomize": false, "replicates": 2}),
    ) else {
        return;
    };
    let design = design.unwrap();
    let mut csv = String::from("a,b,y\n");
    for run in design["runs"].as_array().unwrap() {
        let (a, b) = (
            run["coded"][0].as_f64().unwrap(),
            run["coded"][1].as_f64().unwrap(),
        );
        // The sign alternates by replicate, so the noise is orthogonal to every effect.
        let replicate = (run["standard_order"].as_u64().unwrap() - 1) / 4;
        let noise = if replicate == 0 { 0.1 } else { -0.1 };
        csv.push_str(&format!(
            "{a},{b},{}\n",
            10.0 + 3.0 * a + 2.0 * b + a * b + noise
        ));
    }
    let fit = call(
        &REGRESSION,
        Some(&csv),
        json!({"path": "data.csv", "response": "y", "predictors": ["a", "b"], "interactions": true}),
    )
    .unwrap()
    .unwrap();
    let estimates: Vec<f64> = fit["coefficients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["estimate"].as_f64().unwrap())
        .collect();
    for (estimate, expected) in estimates.iter().zip([10.0, 3.0, 2.0, 1.0]) {
        assert!((estimate - expected).abs() < 1e-9, "{estimates:?}");
    }
}

#[test]
fn r_that_runs_past_the_limit_is_stopped() {
    if !rscript_available() {
        eprintln!("skipped: Rscript is not installed");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data.csv");
    std::fs::write(&path, STUDY).unwrap();
    let prepared = prepare_anova(&json!({"response": "y", "group": "g3"})).unwrap();
    let error = run(
        &ANOVA,
        Path::new("Rscript"),
        Some(&path),
        &prepared,
        Duration::from_millis(1),
    )
    .unwrap_err();
    assert!(error.contains("time limit"), "{error}");
}

// --- statistics_record ---

#[test]
fn the_record_tool_writes_to_the_workspace_so_it_asks_first() {
    let registration = record::registration();
    assert_eq!(registration.side_effect, SideEffectClass::LocalWrite);
    assert_eq!(registration.metadata["requires_approval"], true);
    assert!(registration.workspace_write);
}

#[test]
fn every_finite_number_of_a_result_gets_a_dotted_key_and_the_rest_is_left_out() {
    let result = json!({
        "p_value": 0.02, "df": 11, "method": "Welch", "missing": null,
        "groups": [{"label": "a", "mean": 1.5}, {"label": "b", "mean": 2.5}],
        "coefficients": [{"term": "(Intercept)", "estimate": 4.0, "p_value": null}],
        "warnings": ["a warning with 3 in it"],
        "provenance": {"r_version": "4.6.0", "parameters": {"mu": 5.0}}
    });
    let values = record::flatten(&result);
    let keys: Vec<&str> = values.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "coefficients.1.estimate",
            "df",
            "groups.1.mean",
            "groups.2.mean",
            "p_value"
        ]
    );
    assert_eq!(values["groups.2.mean"]["value"], 2.5);
}

#[test]
fn a_result_path_must_be_a_relative_json_file_inside_the_workspace() {
    for good in ["result.json", "out/t.json", "out/deep/T.JSON"] {
        assert!(record::valid_result_path(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "result.csv",
        "/etc/x.json",
        "../x.json",
        "out/../../x.json",
        "noextension",
        "out/",
    ] {
        assert!(record::valid_result_path(bad).is_err(), "{bad}");
    }
}

fn record_call(workspace: &Path, arguments: Value) -> crate::runtime::ToolResult {
    use crate::runtime::{ToolCall, ToolExecutionContext, ToolExecutor};
    let host = crate::capability::CapabilityHost::product_defaults().unwrap();
    host.execute(
        &ToolCall {
            id: "c".into(),
            name: "statistics_record".into(),
            arguments,
        },
        &ToolExecutionContext {
            session_id: "s".into(),
            run_id: "r".into(),
            workspace: Some(workspace.to_string_lossy().to_string()),
            timeout: Duration::from_secs(60),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: Default::default(),
            proxy: None,
        },
    )
}

#[test]
fn the_record_tool_stages_a_result_file_with_the_numbers_r_computed() {
    if !rscript_available() {
        eprintln!("skipped: Rscript is not installed");
        return;
    }
    let workspace = tempfile::tempdir().unwrap();
    let input = workspace.path().join("data.csv");
    std::fs::write(&input, STUDY).unwrap();
    // The R release pin is the product's rule and is tested on its own; the figures do not depend on it.
    let (output, bytes) = record::build(
        &TTEST,
        Path::new("Rscript"),
        &input,
        &json!({"path": "data.csv", "kind": "one_sample", "column": "y", "mu": 5}),
    )
    .unwrap();
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(file["format"], "delta.result");
    assert_eq!(file["method"], "ttest");
    // The same figures the plain tool reports (SciPy: t = 4.8736…, p = 0.00049…).
    close(
        &file["values"]["statistic"]["value"],
        4.873617552725215,
        1e-9,
    );
    close(
        &file["values"]["p_value"]["value"],
        0.0004918624151844586,
        1e-7,
    );
    close(
        &file["values"]["conf_low"]["value"],
        6.3572597973079095,
        1e-9,
    );
    assert_eq!(file["provenance"]["method"], "ttest");
    assert_eq!(
        file["provenance"]["input_sha256"],
        sha256_hex(STUDY.as_bytes())
    );
    // What the model is told is the result itself plus the keys it can cite.
    close(&output["p_value"], 0.0004918624151844586, 1e-7);
    assert!(output["keys"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key == "p_value"));
}

#[test]
fn through_the_host_the_record_tool_holds_to_the_pinned_r_release() {
    if !rscript_available() {
        return;
    }
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("data.csv"), STUDY).unwrap();
    let result = record_call(
        workspace.path(),
        json!({
            "method": "ttest", "path": "data.csv", "result_path": "out/t.json",
            "options": {"kind": "one_sample", "column": "y"}
        }),
    );
    // Either the pinned R is installed and a file is staged, or nothing is computed and nothing is written.
    match result.error {
        None => assert_eq!(result.staged_artifacts[0].relative_path, "out/t.json"),
        Some(error) => {
            assert!(error.contains("R 4.6.0"), "{error}");
            assert!(result.staged_artifacts.is_empty());
        }
    }
}

#[test]
fn a_recorded_result_file_is_what_a_study_round_can_point_at() {
    if !rscript_available() {
        eprintln!("skipped: Rscript is not installed");
        return;
    }
    let workspace = tempfile::tempdir().unwrap();
    let input = workspace.path().join("data.csv");
    std::fs::write(&input, STUDY).unwrap();
    let (output, bytes) = record::build(
        &CORRELATION,
        Path::new("Rscript"),
        &input,
        &json!({"path": "data.csv", "column_a": "x1", "column_b": "y"}),
    )
    .unwrap();
    let estimate = output["estimate"].as_f64().unwrap();
    let sha = sha256_hex(&bytes);
    let study: crate::study::Study = serde_json::from_value(json!({
        "schema_version": 1, "objective": "o",
        "responses": [{"name": "r", "criterion": "c"}],
        "factors": [{"name": "f", "low": 1.0, "high": 2.0}],
        "allowed_adjustments": {"factors": [{"name": "f", "min": 0.0, "max": 3.0}]},
        "stopping_rule": "s",
        "analysis_plan": {"model": "m", "significance_level": 0.05, "confidence_level": 0.95},
        "sequential": {"rule": {"response": "r", "direction": "at_least", "threshold": 0.9, "consecutive_rounds": 1, "max_rounds": 3}},
        "rounds": [{
            "number": 1, "factors": [{"name": "f", "low": 1.0, "high": 2.0}],
            "results": {"r": {"value": estimate, "source": {"artifact": "round1.json", "sha256": sha, "key": "estimate"}}}
        }]
    }))
    .unwrap();
    let checks = crate::study::verify::verify_sources(&study, &|_: &str| Ok(bytes.clone()));
    assert!(checks[0].is_verified, "{:?}", checks[0]);
    assert_eq!(
        crate::study::sequential::evaluate(&study).unwrap().status,
        crate::study::sequential::Status::StoppedMet
    );
}

#[test]
fn the_record_tool_refuses_what_it_cannot_do_before_it_runs_anything() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("data.csv"), STUDY).unwrap();
    let call = |arguments: Value| record_call(workspace.path(), arguments).error;
    let ok_options = json!({"kind": "one_sample", "column": "y"});
    assert!(call(json!({"path": "data.csv", "result_path": "a.json"}))
        .unwrap()
        .contains("method"));
    assert!(
        call(json!({"method": "doe", "path": "data.csv", "result_path": "a.json"}))
            .unwrap()
            .contains("must be one of")
    );
    assert!(call(json!({"method": "ttest", "path": "data.csv", "result_path": "a.csv", "options": ok_options})).unwrap().contains("result_path"));
    assert!(call(json!({"method": "ttest", "path": "data.csv", "result_path": "../a.json", "options": ok_options})).unwrap().contains("result_path"));
    assert!(call(json!({"method": "ttest", "path": "../data.csv", "result_path": "a.json", "options": ok_options})).is_some());
    // The method's own argument checks still apply.
    assert!(call(json!({"method": "ttest", "path": "data.csv", "result_path": "a.json", "options": {"kind": "one_sample"}})).unwrap().contains("column"));
}
