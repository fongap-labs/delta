"""File Reader capability worker - read-only connector (R8.7 vertical slice).

This worker implements the `file.read` capability using the Delta Worker SDK.
It demonstrates the Foundation -> Suite contract:
- Manifest declares capability_id, permissions, timeout
- Worker uses delta-worker-sdk for protocol handling
- Secrets accessed via grants.secrets (none for read-only)
- Progress emission for long-running reads
- Artifact staging for large files
"""

from __future__ import annotations

import hashlib
from pathlib import Path

from delta_worker_sdk import WorkerContext, run_worker


def handle_file_read(ctx: WorkerContext):
    """Handle file.read capability."""
    # Get arguments
    path = ctx.arguments.get("path")
    if not path:
        return ctx.error("Missing required argument: path", code="invalid_args")

    # Resolve path relative to workspace
    workspace = ctx.workspace
    if workspace:
        full_path = Path(workspace) / path
    else:
        full_path = Path(path)

    # Security check: ensure path is within workspace
    try:
        full_path.resolve().relative_to(Path(workspace).resolve() if workspace else Path.cwd())
    except ValueError:
        return ctx.error(
            f"Path '{path}' is outside workspace", code="permission_denied"
        )

    # Check if file exists
    if not full_path.exists():
        return ctx.error(f"File not found: {path}", code="not_found")

    if not full_path.is_file():
        return ctx.error(f"Path is not a file: {path}", code="invalid_path")

    # Emit progress
    ctx.emit_progress(0.2, stage="reading", message="Opening file")

    try:
        content = full_path.read_bytes()
    except PermissionError:
        return ctx.error(f"Permission denied: {path}", code="permission_denied")
    except OSError as e:
        return ctx.error(f"Failed to read file: {e}", code="io_error")

    ctx.emit_progress(0.6, stage="hashing", message="Computing SHA256")

    # Compute hash for artifact integrity
    sha256 = hashlib.sha256(content).hexdigest()

    # Stage as artifact if large, otherwise return inline
    MAX_INLINE = 1024 * 1024  # 1MB
    if len(content) > MAX_INLINE:
        ctx.emit_progress(0.8, stage="staging", message="Staging large file")
        artifact = ctx.stage_artifact(
            relative_path=f"read_{full_path.name}",
            content=content,
            kind="file",
            sha256=sha256,
        )
        return ctx.result(
            output=f"File staged as artifact ({len(content)} bytes)",
            result={"path": path, "size": len(content), "sha256": sha256},
            artifacts=[artifact],
        )
    else:
        # Return content inline for small files
        text_content = content.decode("utf-8", errors="replace")
        return ctx.result(
            output=f"Read {len(content)} bytes",
            result={
                "path": path,
                "size": len(content),
                "sha256": sha256,
                "content": text_content,
            },
        )


if __name__ == "__main__":
    run_worker(handle_file_read)