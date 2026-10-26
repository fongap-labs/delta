//! The experimental designs of `statistics_doe` (ADR-0071): two-level full and fractional factorials,
//! central composite and Box-Behnken response-surface designs, and D-optimal designs for a chosen
//! model. R lays out every run, in standard order and in the random order that the seed reproduces.
//! Rust checks the request before R starts, so a design that cannot be run is refused with the
//! reason, and it checks that R drew the number of runs the request implies.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::{
    choice, count, figure, flag, real, registration, text, whole_number, Method, Prepared,
    MAX_CENTER_POINTS, MAX_DESIGN_RUNS, MAX_FACTORS, MAX_REPLICATES,
};
use crate::capability::CapabilityRegistration;

const DESIGNS: [&str; 5] = [
    "full",
    "fractional",
    "central_composite",
    "box_behnken",
    "d_optimal",
];
/// A central composite design has 2^k corners and 2k axial points, so 5 factors is 42 runs.
const MAX_CCD_FACTORS: usize = 5;
const MIN_BOX_BEHNKEN_FACTORS: usize = 3;
const MAX_BOX_BEHNKEN_FACTORS: usize = 7;
/// A D-optimal design chooses among 3^k candidate points, so 5 factors is 243 candidates.
const MAX_OPTIMAL_FACTORS: usize = 5;
const MAX_OPTIMAL_RUNS: u64 = 60;

/// Arguments after the input placeholder: design, seed, replicates, center points, randomize,
/// alpha rule, runs, model, generators, then low and high of each factor. Coded values are -1
/// (low), +1 (high) and 0 (center). `generators` is `g:f1,f2;...`, where `g` and `f` are 1-based
/// factor positions.
const DOE_SCRIPT: &str = r##"
design <- args[2]
seed <- as.integer(args[3])
replicates <- as.integer(args[4])
centers <- as.integer(args[5])
randomize <- args[6] == "true"
alpha_rule <- args[7]
runs_wanted <- as.integer(args[8])
model <- args[9]
generators <- args[10]
limits <- as.numeric(args[11:length(args)])
k <- length(limits) / 2
low <- limits[seq(1, by = 2, length.out = k)]
high <- limits[seq(2, by = 2, length.out = k)]
corners <- function(n) as.matrix(expand.grid(rep(list(c(-1, 1)), n)))
extra <- character(0)
if (design == "full") {
  coded <- corners(k)[rep(seq_len(2^k), replicates), , drop = FALSE]
  type <- rep("corner", nrow(coded))
} else if (design == "fractional") {
  pieces <- strsplit(generators, ";", fixed = TRUE)[[1]]
  generated <- as.integer(sub(":.*", "", pieces))
  sources <- lapply(strsplit(sub("^[0-9]+:", "", pieces), ",", fixed = TRUE), as.integer)
  base_k <- k - length(pieces)
  base <- corners(base_k)
  full_coded <- matrix(0, nrow = nrow(base), ncol = k)
  full_coded[, seq_len(base_k)] <- base
  for (g in seq_along(pieces)) {
    full_coded[, generated[g]] <- apply(base[, sources[[g]], drop = FALSE], 1, prod)
  }
  coded <- full_coded[rep(seq_len(nrow(full_coded)), replicates), , drop = FALSE]
  type <- rep("fraction", nrow(coded))
  bit <- function(i) bitwShiftL(1L, as.integer(i) - 1L)
  words <- integer(length(pieces))
  for (g in seq_along(pieces)) {
    word <- bit(generated[g])
    for (s in sources[[g]]) word <- bitwOr(word, bit(s))
    words[g] <- word
  }
  popcount <- function(w) sum(as.integer(intToBits(as.integer(w)))[seq_len(k)])
  defining <- integer(0)
  for (mask in seq_len(2^length(pieces) - 1)) {
    word <- 0L
    for (g in seq_along(pieces)) {
      if (bitwAnd(mask, bitwShiftL(1L, g - 1L)) != 0L) word <- bitwXor(word, words[g])
    }
    defining <- c(defining, word)
  }
  resolution <- min(vapply(defining, popcount, integer(1)))
  extra <- c(extra, paste0("resolution: ", resolution),
             paste0("defining_count: ", length(defining)))
  for (n in seq_along(defining)) {
    members <- which(as.integer(intToBits(defining[n]))[seq_len(k)] == 1L)
    extra <- c(extra, paste0("defining.", n, ": ", paste(members, collapse = "|")))
  }
} else if (design == "central_composite") {
  alpha <- if (alpha_rule == "face") 1 else (2^k)^(1/4)
  axial <- matrix(0, nrow = 2 * k, ncol = k)
  for (i in seq_len(k)) {
    axial[2 * i - 1, i] <- -alpha
    axial[2 * i, i] <- alpha
  }
  coded <- rbind(corners(k), axial)
  type <- c(rep("corner", 2^k), rep("axial", 2 * k))
  extra <- c(extra, paste0("alpha: ", num(alpha)))
} else if (design == "box_behnken") {
  rows <- list()
  for (i in seq_len(k - 1)) {
    for (j in (i + 1):k) {
      for (a in c(-1, 1)) {
        for (b in c(-1, 1)) {
          row <- numeric(k)
          row[i] <- a
          row[j] <- b
          rows[[length(rows) + 1]] <- row
        }
      }
    }
  }
  coded <- do.call(rbind, rows)
  type <- rep("edge", nrow(coded))
} else if (design == "d_optimal") {
  candidates <- as.matrix(expand.grid(rep(list(c(-1, 0, 1)), k)))
  model_matrix <- function(x) {
    columns <- list(rep(1, nrow(x)))
    for (i in seq_len(k)) columns[[length(columns) + 1]] <- x[, i]
    if (model != "linear") {
      for (i in seq_len(k - 1)) {
        for (j in (i + 1):k) columns[[length(columns) + 1]] <- x[, i] * x[, j]
      }
    }
    if (model == "quadratic") {
      for (i in seq_len(k)) columns[[length(columns) + 1]] <- x[, i]^2
    }
    do.call(cbind, columns)
  }
  Xc <- model_matrix(candidates)
  set.seed(seed)
  best_det <- -Inf
  best <- NULL
  for (restart in seq_len(10)) {
    picked <- sample(nrow(Xc), runs_wanted, replace = TRUE)
    current <- det(crossprod(Xc[picked, , drop = FALSE]))
    repeat {
      improved <- FALSE
      for (position in seq_len(runs_wanted)) {
        rest <- crossprod(Xc[picked[-position], , drop = FALSE])
        for (candidate in seq_len(nrow(Xc))) {
          row <- Xc[candidate, ]
          trial <- det(rest + outer(row, row))
          if (trial > current * (1 + 1e-9) + 1e-12) {
            picked[position] <- candidate
            current <- trial
            improved <- TRUE
          }
        }
      }
      if (!improved) break
    }
    if (current > best_det) {
      best_det <- current
      best <- picked
    }
  }
  if (!(best_det > 1e-9)) stop("the model cannot be estimated with this many runs; increase runs")
  coded <- candidates[sort(best), , drop = FALSE]
  type <- rep("design", nrow(coded))
  terms <- ncol(Xc)
  efficiency <- 100 * best_det^(1 / terms) / runs_wanted
  extra <- c(extra, paste0("d_efficiency: ", num(efficiency)), paste0("model: ", model),
             paste0("model_terms: ", terms), paste0("candidates: ", nrow(candidates)))
}
if (centers > 0) {
  coded <- rbind(coded, matrix(0, nrow = centers, ncol = k))
  type <- c(type, rep("center", centers))
}
standard <- seq_len(nrow(coded))
if (randomize) {
  set.seed(seed)
  order <- sample(nrow(coded))
} else {
  order <- standard
}
cat("design: ", design, "\n", sep = "")
cat("runs: ", num(nrow(coded)), "\n", sep = "")
for (line in extra) cat(line, "\n", sep = "")
for (position in seq_along(order)) {
  i <- order[position]
  centre <- (low + high) / 2
  actual <- centre + coded[i, ] * (high - low) / 2
  cat("run.", position, ": ", i, "|", type[i], "|",
      paste(num(coded[i, ]), collapse = "|"), "|",
      paste(num(actual), collapse = "|"), "\n", sep = "")
}
cat("r_version: ", paste(R.version$major, R.version$minor, sep = "."), "\n", sep = "")
"##;

pub(super) static DOE: Method = Method {
    name: "doe",
    script: DOE_SCRIPT,
    reads_input: false,
    prepare: prepare_doe,
    shape: shape_doe,
};

pub(super) fn doe_registration() -> CapabilityRegistration {
    registration(
        "statistics.doe",
        "statistics_doe",
        "Plan an experiment and return its runs: a two-level full or fractional factorial, a \
         central composite or Box-Behnken response-surface design, or a D-optimal design for a \
         linear, interaction or quadratic model. Runs come in a random order that the seed \
         reproduces; `run_order` is the order to run them. Coded values are -1, 0 and +1 and the \
         actual values come from each factor's low and high level. A fractional design takes \
         generators: each names a later factor as the product of earlier factors. Results add the \
         resolution and defining relation (fractional), alpha (central composite) or D-efficiency \
         (D-optimal). R computes the design. Run the experiment, enter the responses in a CSV file \
         with the coded columns, and analyse it with statistics_regression.",
        json!({
            "type": "object", "required": ["factors"],
            "properties": {
                "factors": {
                    "type": "array", "description": "2 to 8 factors. For a fractional design the generated factors are the last ones, in order.",
                    "items": {
                        "type": "object", "required": ["name", "low", "high"],
                        "properties": {
                            "name": {"type": "string"},
                            "low": {"type": "number"},
                            "high": {"type": "number"}
                        }
                    }
                },
                "design": {"type": "string", "enum": DESIGNS, "description": "Default full."},
                "generators": {
                    "type": "array", "description": "fractional only: one per generated factor, in order. Each names the factor and the base factors whose product it takes.",
                    "items": {
                        "type": "object", "required": ["factor", "from"],
                        "properties": {
                            "factor": {"type": "string"},
                            "from": {"type": "array", "items": {"type": "string"}}
                        }
                    }
                },
                "alpha": {"type": "string", "enum": ["rotatable", "face"], "description": "central_composite only. rotatable (default) puts the axial points at (2^k)^(1/4); face puts them on the faces, at 1."},
                "runs": {"type": "integer", "description": "d_optimal only: how many runs to choose, 2 to 60."},
                "model": {"type": "string", "enum": ["linear", "interaction", "quadratic"], "description": "d_optimal only. Default quadratic."},
                "replicates": {"type": "integer", "description": "How many times every corner is run, 1 to 10. Full and fractional only. Default 1."},
                "center_points": {"type": "integer", "description": "Runs at the middle of every factor, 0 to 10. Default 0, or 3 for central_composite and box_behnken."},
                "randomize": {"type": "boolean", "description": "Randomize the run order. Default true."},
                "seed": {"type": "integer", "description": "Seed of the randomization and of the D-optimal search. Chosen and returned when omitted."}
            }
        }),
        &DOE,
    )
}

pub(super) fn prepare_doe(arguments: &Value) -> Result<Prepared, String> {
    let list = arguments
        .get("factors")
        .and_then(Value::as_array)
        .ok_or_else(|| "factors is required: a list of {name, low, high}".to_owned())?;
    if !(2..=MAX_FACTORS).contains(&list.len()) {
        return Err(format!("factors needs between 2 and {MAX_FACTORS} entries"));
    }
    let mut names: Vec<String> = Vec::with_capacity(list.len());
    let mut limits = Vec::with_capacity(list.len() * 2);
    let mut factors = Vec::with_capacity(list.len());
    for factor in list {
        let name = factor
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty() && name.chars().count() <= 64)
            .ok_or_else(|| "every factor needs a name of 1 to 64 characters".to_owned())?;
        if names.iter().any(|seen| seen == name) {
            return Err(format!("the factor {name} appears twice"));
        }
        let low = real(factor, "low", f64::NAN)
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("the factor {name} needs a numeric low level"))?;
        let high = real(factor, "high", f64::NAN)
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("the factor {name} needs a numeric high level"))?;
        if low >= high {
            return Err(format!("the factor {name} needs low below high"));
        }
        names.push(name.to_owned());
        limits.push(low.to_string());
        limits.push(high.to_string());
        factors.push(json!({"name": name, "low": low, "high": high}));
    }
    let k = names.len();
    let design = choice(arguments, "design", "full", &DESIGNS)?;
    let replicates = whole_number(arguments, "replicates", 1, (1, MAX_REPLICATES))?;
    let default_centers = if matches!(design.as_str(), "central_composite" | "box_behnken") {
        3
    } else {
        0
    };
    let center_points = whole_number(
        arguments,
        "center_points",
        default_centers,
        (0, MAX_CENTER_POINTS),
    )?;
    let randomize = flag(arguments, "randomize", true)?;
    let seed = match arguments.get("seed") {
        None | Some(Value::Null) => seed_from_clock(),
        Some(_) => whole_number(arguments, "seed", 0, (0, i32::MAX as u64))?,
    };
    let alpha = choice(arguments, "alpha", "rotatable", &["rotatable", "face"])?;
    let model = choice(
        arguments,
        "model",
        "quadratic",
        &["linear", "interaction", "quadratic"],
    )?;
    let runs_asked = whole_number(arguments, "runs", 0, (0, MAX_OPTIMAL_RUNS))?;

    let given = |key: &str| arguments.get(key).is_some_and(|value| !value.is_null());
    for (key, only_for) in [
        ("alpha", "central_composite"),
        ("runs", "d_optimal"),
        ("model", "d_optimal"),
        ("generators", "fractional"),
    ] {
        if given(key) && design.as_str() != only_for {
            return Err(format!("{key} does not apply to a {design} design"));
        }
    }

    let mut parameters = json!({
        "design": design, "factors": factors, "replicates": replicates,
        "center_points": center_points, "randomize": randomize, "seed": seed,
    });
    let mut generators = String::new();
    let runs = match design.as_str() {
        "full" => (1u64 << k) * replicates + center_points,
        "fractional" => {
            let parsed = parse_generators(arguments, &names)?;
            generators = encode_generators(&parsed);
            parameters["generators"] = json!(describe_generators(&parsed, &names));
            (1u64 << (k - parsed.len())) * replicates + center_points
        }
        "central_composite" => {
            if k > MAX_CCD_FACTORS {
                return Err(format!(
                    "a central_composite design takes 2 to {MAX_CCD_FACTORS} factors"
                ));
            }
            no_replicates(replicates, &design)?;
            parameters["alpha"] = json!(alpha);
            (1u64 << k) + 2 * k as u64 + center_points
        }
        "box_behnken" => {
            if !(MIN_BOX_BEHNKEN_FACTORS..=MAX_BOX_BEHNKEN_FACTORS).contains(&k) {
                return Err(format!(
                    "a box_behnken design takes {MIN_BOX_BEHNKEN_FACTORS} to {MAX_BOX_BEHNKEN_FACTORS} factors"
                ));
            }
            no_replicates(replicates, &design)?;
            (k as u64 * (k as u64 - 1) / 2) * 4 + center_points
        }
        _ => {
            if k > MAX_OPTIMAL_FACTORS {
                return Err(format!(
                    "a d_optimal design takes 2 to {MAX_OPTIMAL_FACTORS} factors"
                ));
            }
            if runs_asked < 2 {
                return Err(format!(
                    "runs is required for a d_optimal design: a number from 2 to {MAX_OPTIMAL_RUNS}"
                ));
            }
            if replicates != 1 || center_points != 0 {
                return Err(
                    "replicates and center_points do not apply to a d_optimal design".to_owned(),
                );
            }
            let terms = model_terms(&model, k);
            if runs_asked < terms {
                return Err(format!(
                    "runs must be at least {terms}, the number of terms in the {model} model"
                ));
            }
            parameters["model"] = json!(model);
            runs_asked
        }
    };
    if runs > MAX_DESIGN_RUNS as u64 {
        return Err(format!(
            "this design has {runs} runs; the most is {MAX_DESIGN_RUNS}"
        ));
    }
    parameters["expected_runs"] = json!(runs);

    let mut r_args = vec![
        design.clone(),
        seed.to_string(),
        replicates.to_string(),
        center_points.to_string(),
        randomize.to_string(),
        alpha,
        runs_asked.to_string(),
        model,
        generators,
    ];
    r_args.extend(limits);
    Ok(Prepared {
        r_args,
        parameters,
        names,
    })
}

fn no_replicates(replicates: u64, design: &str) -> Result<(), String> {
    if replicates != 1 {
        return Err(format!(
            "replicates does not apply to a {design} design; use center_points for repeated centre runs"
        ));
    }
    Ok(())
}

/// Runs a D-optimal design of `k` factors needs for `model`: the constant, the main effects,
/// and for interaction and quadratic models the two-factor products and the squares.
fn model_terms(model: &str, k: usize) -> u64 {
    let interactions = k * (k - 1) / 2;
    let terms = match model {
        "linear" => 1 + k,
        "interaction" => 1 + k + interactions,
        _ => 1 + 2 * k + interactions,
    };
    terms as u64
}

/// The generators of a fractional design, checked against the factor list. Each returns the
/// generated factor's position and the base factors its column is the product of (0-based).
fn parse_generators(
    arguments: &Value,
    names: &[String],
) -> Result<Vec<(usize, Vec<usize>)>, String> {
    let list = arguments
        .get("generators")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "generators is required for a fractional design: a list of {factor, from}".to_owned()
        })?;
    let k = names.len();
    let p = list.len();
    if p == 0 {
        return Err("a fractional design needs at least one generator".to_owned());
    }
    if k < p + 2 {
        return Err(format!(
            "{p} generators leave fewer than two base factors; use fewer generators or more factors"
        ));
    }
    let base = k - p;
    let index_of = |name: &str| names.iter().position(|known| known == name);
    let mut parsed = Vec::with_capacity(p);
    for (g, generator) in list.iter().enumerate() {
        let factor = generator
            .get("factor")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let generated = index_of(factor)
            .ok_or_else(|| format!("generator {}: {factor} is not a factor", g + 1))?;
        if generated != base + g {
            return Err(format!(
                "the generators define the last {p} factors in order: generator {} must define {}",
                g + 1,
                names[base + g]
            ));
        }
        let from_list = generator
            .get("from")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .ok_or_else(|| format!("generator {}: from needs at least one base factor", g + 1))?;
        let mut from = Vec::with_capacity(from_list.len());
        for item in from_list {
            let name = item.as_str().unwrap_or_default();
            let index = index_of(name)
                .ok_or_else(|| format!("generator {}: {name} is not a factor", g + 1))?;
            if index >= base {
                return Err(format!(
                    "generator {}: {name} is a generated factor; a generator takes the product of base factors, the first {base}",
                    g + 1
                ));
            }
            if from.contains(&index) {
                return Err(format!("generator {}: {name} appears twice", g + 1));
            }
            from.push(index);
        }
        parsed.push((generated, from));
    }
    Ok(parsed)
}

/// The generators as R reads them: `position:position,position;...`, 1-based.
fn encode_generators(parsed: &[(usize, Vec<usize>)]) -> String {
    parsed
        .iter()
        .map(|(generated, from)| {
            let sources: Vec<String> = from.iter().map(|index| (index + 1).to_string()).collect();
            format!("{}:{}", generated + 1, sources.join(","))
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The generators as the answer names them, for example `D = A*B*C`.
fn describe_generators(parsed: &[(usize, Vec<usize>)], names: &[String]) -> Vec<String> {
    parsed
        .iter()
        .map(|(generated, from)| {
            let sources: Vec<&str> = from.iter().map(|index| names[*index].as_str()).collect();
            format!("{} = {}", names[*generated], sources.join("*"))
        })
        .collect()
}

/// A seed for a design the caller did not give one. It is returned with the design.
fn seed_from_clock() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos() as u64 ^ elapsed.as_secs())
        .unwrap_or(1);
    nanos % (i32::MAX as u64)
}

fn shape_doe(values: &BTreeMap<String, String>, prepared: &Prepared) -> Result<Value, String> {
    let runs = count(values, "runs")?;
    let expected = prepared.parameters["expected_runs"]
        .as_u64()
        .ok_or_else(|| "the design has no expected run count".to_owned())?;
    if runs as u64 != expected {
        return Err(format!("R drew {runs} runs, but the design has {expected}"));
    }
    let width = prepared.names.len();
    let mut table = Vec::with_capacity(runs);
    for position in 1..=runs {
        let line = values
            .get(&format!("run.{position}"))
            .ok_or_else(|| format!("R did not report run.{position}"))?;
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() != 2 + width * 2 {
            return Err(format!("R reported a malformed run: {line}"));
        }
        let standard: u64 = fields[0]
            .parse()
            .map_err(|_| format!("R reported a malformed run: {line}"))?;
        let number = |field: &str| -> Result<f64, String> {
            field
                .parse::<f64>()
                .map_err(|_| format!("R reported a malformed run: {line}"))
        };
        let coded = fields[2..2 + width]
            .iter()
            .map(|field| number(field))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = fields[2 + width..]
            .iter()
            .map(|field| number(field))
            .collect::<Result<Vec<_>, _>>()?;
        table.push(json!({
            "run_order": position,
            "standard_order": standard,
            "point_type": fields[1],
            "coded": coded,
            "actual": actual,
        }));
    }
    let mut result = Map::new();
    result.insert("design".to_owned(), json!(text(values, "design")?));
    result.insert("factors".to_owned(), json!(prepared.names));
    result.insert("runs".to_owned(), Value::Array(table));
    result.insert("seed".to_owned(), prepared.parameters["seed"].clone());
    result.insert(
        "randomized".to_owned(),
        prepared.parameters["randomize"].clone(),
    );
    if values.contains_key("alpha") {
        result.insert("alpha".to_owned(), figure(values, "alpha")?);
    }
    if values.contains_key("resolution") {
        result.insert("resolution".to_owned(), figure(values, "resolution")?);
        let defining_count = count(values, "defining_count")?;
        let mut relation = Vec::with_capacity(defining_count);
        for number in 1..=defining_count {
            let line = values
                .get(&format!("defining.{number}"))
                .ok_or_else(|| format!("R did not report defining.{number}"))?;
            let members = line
                .split('|')
                .map(|field| {
                    field
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| index.checked_sub(1))
                        .and_then(|index| prepared.names.get(index))
                        .cloned()
                        .ok_or_else(|| format!("R reported a malformed word: {line}"))
                })
                .collect::<Result<Vec<String>, String>>()?;
            relation.push(json!(members.join("*")));
        }
        result.insert("defining_relation".to_owned(), Value::Array(relation));
    }
    if values.contains_key("d_efficiency") {
        result.insert("d_efficiency".to_owned(), figure(values, "d_efficiency")?);
        result.insert("model".to_owned(), json!(text(values, "model")?));
        result.insert(
            "model_terms".to_owned(),
            json!(count(values, "model_terms")?),
        );
    }
    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factor(name: &str, low: f64, high: f64) -> Value {
        json!({"name": name, "low": low, "high": high})
    }

    fn factors(names: &[&str]) -> Vec<Value> {
        names.iter().map(|name| factor(name, -1.0, 1.0)).collect()
    }

    fn refused(arguments: Value, needle: &str) {
        let error = prepare_doe(&arguments).err().expect("should be refused");
        assert!(error.contains(needle), "{error}");
    }

    #[test]
    fn a_full_design_keeps_its_runs_and_its_default_centres() {
        let plain = prepare_doe(&json!({"factors": factors(&["a", "b"]), "seed": 1})).unwrap();
        assert_eq!(plain.parameters["expected_runs"], 4);
        assert_eq!(plain.parameters["design"], "full");
        let centred = prepare_doe(&json!({
            "factors": factors(&["a", "b"]), "center_points": 2, "seed": 1
        }))
        .unwrap();
        assert_eq!(centred.parameters["expected_runs"], 6);
    }

    #[test]
    fn a_fractional_design_names_its_generators_and_runs_the_base_factors() {
        let prepared = prepare_doe(&json!({
            "design": "fractional",
            "factors": factors(&["A", "B", "C", "D"]),
            "generators": [{"factor": "D", "from": ["A", "B", "C"]}],
            "seed": 1,
        }))
        .unwrap();
        assert_eq!(prepared.parameters["expected_runs"], 8);
        assert_eq!(prepared.parameters["generators"], json!(["D = A*B*C"]));
        assert_eq!(prepared.r_args[8], "4:1,2,3");
    }

    #[test]
    fn a_fractional_design_checks_its_generators() {
        let four = factors(&["A", "B", "C", "D"]);
        let fractional = |generators: Value| json!({"design": "fractional", "factors": four, "generators": generators, "seed": 1});
        refused(
            json!({"design": "fractional", "factors": four, "seed": 1}),
            "generators is required",
        );
        refused(fractional(json!([])), "at least one generator");
        refused(
            fractional(json!([{"factor": "C", "from": ["A", "B"]}])),
            "must define D",
        );
        refused(
            fractional(json!([{"factor": "D", "from": ["A", "D"]}])),
            "generated factor",
        );
        refused(
            fractional(json!([{"factor": "D", "from": ["A", "A"]}])),
            "appears twice",
        );
        refused(
            fractional(json!([{"factor": "D", "from": ["Z"]}])),
            "is not a factor",
        );
        let three = factors(&["A", "B", "C"]);
        refused(
            json!({
                "design": "fractional", "factors": three, "seed": 1,
                "generators": [{"factor": "B", "from": ["A"]}, {"factor": "C", "from": ["A"]}]
            }),
            "fewer than two base factors",
        );
    }

    #[test]
    fn a_central_composite_design_has_corners_axial_points_and_three_centres() {
        let prepared = prepare_doe(&json!({
            "design": "central_composite", "factors": factors(&["a", "b", "c"]), "seed": 1
        }))
        .unwrap();
        // 2^3 corners, 2*3 axial points, 3 centre points.
        assert_eq!(prepared.parameters["expected_runs"], 17);
        assert_eq!(prepared.parameters["alpha"], "rotatable");
        refused(
            json!({"design": "central_composite", "factors": factors(&["a", "b", "c", "d", "e", "f"])}),
            "2 to 5 factors",
        );
        refused(
            json!({"design": "central_composite", "factors": factors(&["a", "b"]), "replicates": 2}),
            "replicates does not apply",
        );
        refused(
            json!({"design": "full", "factors": factors(&["a", "b"]), "alpha": "face"}),
            "alpha does not apply",
        );
    }

    #[test]
    fn a_box_behnken_design_needs_three_to_seven_factors() {
        let prepared = prepare_doe(&json!({
            "design": "box_behnken", "factors": factors(&["a", "b", "c"]), "seed": 1
        }))
        .unwrap();
        // 3 pairs of factors, 4 runs each, and 3 centre points.
        assert_eq!(prepared.parameters["expected_runs"], 15);
        refused(
            json!({"design": "box_behnken", "factors": factors(&["a", "b"])}),
            "3 to 7 factors",
        );
    }

    #[test]
    fn a_d_optimal_design_needs_runs_enough_for_its_model() {
        let two = factors(&["a", "b"]);
        refused(
            json!({"design": "d_optimal", "factors": two}),
            "runs is required",
        );
        // A quadratic model of two factors has 6 terms.
        refused(
            json!({"design": "d_optimal", "factors": two, "runs": 5}),
            "at least 6",
        );
        let prepared = prepare_doe(&json!({
            "design": "d_optimal", "factors": two, "runs": 10, "seed": 3
        }))
        .unwrap();
        assert_eq!(prepared.parameters["expected_runs"], 10);
        assert_eq!(prepared.parameters["model"], "quadratic");
        refused(
            json!({"design": "d_optimal", "factors": two, "runs": 10, "center_points": 2}),
            "do not apply to a d_optimal",
        );
        refused(
            json!({"factors": two, "runs": 10}),
            "runs does not apply to a full design",
        );
    }

    #[test]
    fn an_unknown_design_is_refused() {
        refused(
            json!({"design": "latin_square", "factors": factors(&["a", "b"])}),
            "design",
        );
    }

    /// Draws a design with the pinned R when there is one, as the other R tests do.
    fn draw(arguments: Value) -> Option<Result<Value, String>> {
        let rscript = super::super::super::find_rscript().ok()?;
        let prepared = match prepare_doe(&arguments) {
            Ok(prepared) => prepared,
            Err(error) => return Some(Err(error)),
        };
        Some(super::super::run(
            &DOE,
            &rscript,
            None,
            &prepared,
            super::super::METHOD_TIMEOUT,
        ))
    }

    #[test]
    fn a_fractional_design_has_the_resolution_its_generators_give() {
        let Some(result) = draw(json!({
            "design": "fractional",
            "factors": factors(&["A", "B", "C", "D"]),
            "generators": [{"factor": "D", "from": ["A", "B", "C"]}],
            "seed": 5,
        })) else {
            return;
        };
        let result = result.unwrap();
        assert_eq!(result["runs"].as_array().unwrap().len(), 8);
        // D = ABC gives the defining relation I = ABCD, so the design is of resolution IV.
        assert_eq!(result["resolution"].as_f64(), Some(4.0));
        assert_eq!(result["defining_relation"], json!(["A*B*C*D"]));
    }

    #[test]
    fn a_central_composite_design_uses_the_rotatable_axial_distance() {
        let Some(result) = draw(json!({
            "design": "central_composite", "factors": factors(&["a", "b"]), "seed": 5
        })) else {
            return;
        };
        let result = result.unwrap();
        assert_eq!(result["runs"].as_array().unwrap().len(), 11);
        let alpha = result["alpha"].as_f64().unwrap();
        assert!((alpha - 2f64.sqrt()).abs() < 1e-12, "{alpha}");
    }

    #[test]
    fn a_box_behnken_design_has_its_edge_and_centre_runs() {
        let Some(result) = draw(json!({
            "design": "box_behnken", "factors": factors(&["a", "b", "c"]), "seed": 5
        })) else {
            return;
        };
        let result = result.unwrap();
        let runs = result["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 15);
        let edges = runs
            .iter()
            .filter(|run| run["point_type"] == "edge")
            .count();
        assert_eq!(edges, 12);
    }

    #[test]
    fn a_d_optimal_design_draws_its_runs_from_the_candidates() {
        let Some(result) = draw(json!({
            "design": "d_optimal", "factors": factors(&["a", "b"]), "runs": 10, "seed": 9
        })) else {
            return;
        };
        let result = result.unwrap();
        let runs = result["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 10);
        assert!(result["d_efficiency"].as_f64().unwrap() > 0.0);
        for run in runs {
            for value in run["coded"].as_array().unwrap() {
                let value = value.as_f64().unwrap();
                assert!([-1.0, 0.0, 1.0].contains(&value), "{value}");
            }
        }
    }
}
