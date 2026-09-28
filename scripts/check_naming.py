from __future__ import annotations

import ast
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BANNED = {"impl", "helper", "helpers", "common", "misc", "shared", "new", "final", "latest", "temp", "tmp"}
BOOL_HINTS = ("enabled", "disabled", "required", "available", "active", "visible", "writable", "replace")
BOOL_PREFIXES = ("is_", "has_", "can_", "should_")
SKIP_DIRS = {".git", ".venv", "node_modules", "target", "dist", "build", "__pycache__"}


def parts(name: str) -> list[str]:
    return [p for p in re.split(r"[-_]+", name.strip("_-")) if p]


def is_skipped(path: Path) -> bool:
    return any(part in SKIP_DIRS for part in path.parts)


def add(errors: list[str], path: Path, message: str) -> None:
    errors.append(f"{path.relative_to(ROOT)}: {message}")


def check_files(errors: list[str]) -> None:
    workflows = ROOT / ".github" / "workflows"
    if workflows.exists():
        for path in workflows.iterdir():
            if path.suffix not in {".yml", ".yaml"}:
                continue
            stem = path.stem
            if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+){0,2}", stem):
                add(errors, path, "workflow must be kebab-case with at most three segments")

    for path in ROOT.rglob("*"):
        if not path.is_file() or is_skipped(path):
            continue

        if path.suffix.lower() in {".sh", ".ps1"}:
            if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+){0,2}", path.stem):
                add(errors, path, "shell file must be kebab-case with at most three segments")

        if "docs" in path.parts and "governance" in path.parts and path.suffix == ".md":
            if path.name in {"README.md", "README_ZH.md"}:
                continue
            if not re.fullmatch(r"[A-Z0-9]+(?:_[A-Z0-9]+){0,2}\.md", path.name):
                add(errors, path, "governance document must be UPPER_SNAKE_CASE with at most three segments")

        if path.suffix in {".py", ".rs"}:
            stem = path.stem
            if stem in {"__init__", "main", "lib", "build"}:
                continue
            if path.name.startswith("test_") and path.suffix == ".py":
                payload = path.stem[5:]
                if len(parts(payload)) > 3:
                    add(errors, path, "pytest filename payload must have at most three segments")
                continue
            tokens = parts(stem)
            if len(tokens) > 3:
                add(errors, path, "module filename must have at most three segments")
            bad = BANNED.intersection(tokens)
            if bad:
                add(errors, path, f"module filename contains banned term(s): {', '.join(sorted(bad))}")


def check_python(errors: list[str]) -> None:
    roots = [ROOT / "delta_extension_api", ROOT / "integrations", ROOT / "packages", ROOT / "advanced"]
    for base in roots:
        if not base.exists():
            continue
        for path in base.rglob("*.py"):
            if is_skipped(path) or path.name.startswith("test_") or "tests" in path.parts:
                continue
            try:
                tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
            except SyntaxError:
                continue
            for node in ast.walk(tree):
                if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                name = node.name.lstrip("_")
                if name and not (node.name.startswith("__") and node.name.endswith("__")):
                    if len(parts(name)) > 3:
                        add(errors, path, f"function '{node.name}' exceeds three segments")
                    if not node.name.startswith("_") and BANNED.intersection(parts(name)):
                        add(errors, path, f"function '{node.name}' contains a banned naming term")
                args = [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]
                if node.args.vararg:
                    args.append(node.args.vararg)
                if node.args.kwarg:
                    args.append(node.args.kwarg)
                for arg in args:
                    arg_name = arg.arg.lstrip("_")
                    if arg_name in {"self", "cls"}:
                        continue
                    if len(parts(arg_name)) > 3:
                        add(errors, path, f"parameter '{arg.arg}' exceeds three segments")
                    annotation = ast.unparse(arg.annotation) if arg.annotation is not None else ""
                    bool_types = {"bool", "bool | None", "None | bool", "Optional[bool]", "typing.Optional[bool]"}
                    hinted_bool = not annotation and (
                        arg_name in BOOL_HINTS
                        or any(arg_name.endswith("_" + hint) for hint in BOOL_HINTS)
                    )
                    looks_bool = annotation in bool_types or hinted_bool
                    if looks_bool and not arg_name.startswith(BOOL_PREFIXES):
                        add(errors, path, f"boolean parameter '{arg.arg}' must start with is_/has_/can_/should_")

                for child in ast.walk(node):
                    if isinstance(child, (ast.Assign, ast.AnnAssign)):
                        value = child.value
                        is_bool_value = isinstance(value, ast.Constant) and isinstance(value.value, bool)
                        annotation = ast.unparse(child.annotation) if isinstance(child, ast.AnnAssign) and child.annotation is not None else ""
                        is_bool_annotation = annotation in {"bool", "bool | None", "None | bool", "Optional[bool]", "typing.Optional[bool]"}
                        if not (is_bool_value or is_bool_annotation):
                            continue
                        targets = child.targets if isinstance(child, ast.Assign) else [child.target]
                        for target in targets:
                            variable = ""
                            if isinstance(target, ast.Name):
                                variable = target.id
                            elif isinstance(target, ast.Attribute) and isinstance(target.value, ast.Name) and target.value.id == "self":
                                variable = target.attr
                            variable = variable.lstrip("_")
                            if variable and not variable.startswith(BOOL_PREFIXES):
                                add(errors, path, f"boolean variable '{variable}' must start with is_/has_/can_/should_")


def camel_parts(name: str) -> list[str]:
    spaced = re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", name)
    return [part for part in spaced.replace("_", " ").split() if part]


def _strip_rust_tests(text: str) -> str:
    """Exclude inline Rust test sections from production naming analysis.

    Delta keeps unit-test modules at the end of source files. The shared
    convention governs test filenames; descriptive test function names are not
    part of the runtime naming surface.
    """
    marker = text.find("#[cfg(test)]")
    return text if marker < 0 else text[:marker]


def check_rust(errors: list[str]) -> None:
    rust_paths = [
        path
        for path in ROOT.rglob("*.rs")
        if not is_skipped(path)
        and "tests" not in path.parts
        and not path.name.startswith("test_")
    ]
    fn_pattern = re.compile(
        r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-z_][a-z0-9_]*)"
    )
    bool_param = re.compile(r"\b([a-z][a-z0-9_]*)\s*:\s*(?:Option\s*<\s*)?bool\s*>?")
    bool_local = re.compile(
        r"\blet\s+(?:mut\s+)?([a-z_][a-z0-9_]*)\s*(?::\s*bool)?\s*=\s*(?:true|false)\b"
    )

    for path in rust_paths:
        text = _strip_rust_tests(path.read_text(encoding="utf-8"))
        for match in fn_pattern.finditer(text):
            name = match.group(1)
            if len(parts(name)) > 3:
                add(errors, path, f"Rust function '{name}' exceeds three segments")

            block = _parameter_block(text, match.start())
            for param in _split_parameters(block):
                typed_bool = bool_param.search(param)
                if typed_bool:
                    param_name = typed_bool.group(1)
                    if not param_name.startswith(BOOL_PREFIXES):
                        add(
                            errors,
                            path,
                            f"boolean Rust parameter '{param_name}' must start with is_/has_/can_/should_",
                        )

        for name in sorted(set(bool_local.findall(text))):
            if not name.startswith(BOOL_PREFIXES):
                add(
                    errors,
                    path,
                    f"boolean Rust variable '{name}' must start with is_/has_/can_/should_",
                )


def _parameter_block(text: str, start: int) -> str:
    open_index = text.find("(", start)
    if open_index < 0:
        return ""
    depth = 0
    for index in range(open_index, len(text)):
        char = text[index]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[open_index + 1 : index]
    return ""


def _split_parameters(block: str) -> list[str]:
    params: list[str] = []
    start = 0
    round_depth = square_depth = brace_depth = angle_depth = 0
    for index, char in enumerate(block):
        if char == "(":
            round_depth += 1
        elif char == ")":
            round_depth = max(0, round_depth - 1)
        elif char == "[":
            square_depth += 1
        elif char == "]":
            square_depth = max(0, square_depth - 1)
        elif char == "{":
            brace_depth += 1
        elif char == "}":
            brace_depth = max(0, brace_depth - 1)
        elif char == "<":
            angle_depth += 1
        elif char == ">":
            angle_depth = max(0, angle_depth - 1)
        elif (
            char == ","
            and round_depth == 0
            and square_depth == 0
            and brace_depth == 0
            and angle_depth == 0
        ):
            params.append(block[start:index].strip())
            start = index + 1
    tail = block[start:].strip()
    if tail:
        params.append(tail)
    return params


def _check_ts_parameters(errors: list[str], path: Path, block: str) -> None:
    for param in _split_parameters(block):
        typed_bool = re.match(r"([A-Za-z][A-Za-z0-9]*)\??\s*:\s*boolean\b", param)
        default_bool = re.match(r"([A-Za-z][A-Za-z0-9]*)\s*=\s*(?:true|false)\b", param)
        match = typed_bool or default_bool
        if not match:
            continue
        name = match.group(1)
        if not name.startswith(("is", "has", "can", "should")):
            add(
                errors,
                path,
                f"boolean TypeScript parameter '{name}' must start with is/has/can/should",
            )


def check_typescript(errors: list[str]) -> None:
    paths = [
        path
        for suffix in ("*.ts", "*.tsx")
        for path in ROOT.rglob(suffix)
        if not is_skipped(path)
        and "tests" not in path.parts
        and ".test." not in path.name
        and ".spec." not in path.name
        and not path.name.endswith(".d.ts")
    ]
    function_name = re.compile(
        r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z][A-Za-z0-9]*)"
    )
    arrow_name = re.compile(
        r"(?m)^\s*(?:export\s+)?const\s+([A-Za-z][A-Za-z0-9]*)\s*=\s*(?:async\s*)?\("
    )
    bool_local = re.compile(
        r"\b(?:let|const)\s+([A-Za-z][A-Za-z0-9]*)\s*=\s*(?:true|false)\b"
    )
    bool_state = re.compile(
        r"\bconst\s*\[\s*([A-Za-z][A-Za-z0-9]*)\s*,[^\]]+\]\s*=\s*useState(?:<boolean>)?\(\s*(?:true|false)\s*\)"
    )

    for path in paths:
        text = path.read_text(encoding="utf-8")

        for pattern in (function_name, arrow_name):
            for match in pattern.finditer(text):
                name = match.group(1)
                if len(camel_parts(name)) > 3:
                    add(errors, path, f"TypeScript function '{name}' exceeds three segments")
                _check_ts_parameters(errors, path, _parameter_block(text, match.start()))

        for name in bool_local.findall(text):
            if not name.startswith(("is", "has", "can", "should")):
                add(
                    errors,
                    path,
                    f"boolean TypeScript variable '{name}' must start with is/has/can/should",
                )

        for name in bool_state.findall(text):
            if not name.startswith(("is", "has", "can", "should")):
                add(
                    errors,
                    path,
                    f"boolean TypeScript state '{name}' must start with is/has/can/should",
                )


def main() -> int:
    errors: list[str] = []
    check_files(errors)
    check_python(errors)
    check_rust(errors)
    check_typescript(errors)
    if errors:
        print("naming violations:")
        for error in sorted(set(errors)):
            print(f"  {error}")
        return 1
    print("naming conventions passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
