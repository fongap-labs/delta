"""Delta Worker SDK - Thin protocol adapter for capability workers.

This package provides a minimal, "small and dumb" protocol adapter layer
for Python capability workers. It handles:

- Reading CapabilityJob + secret payload from stdin
- ABI version negotiation
- Typed access to arguments, grants, and granted secrets
- Progress reporting via JSON frames
- Cancellation observation
- Artifact staging helper
- Typed result/error emission

It is NOT a runtime, agent, scheduler, state manager, or permission manager.
"""

from .context import WorkerContext, run_persistent_worker, run_worker
from .protocol import (
    CapabilityJob,
    CapabilityGrants,
    CapabilityBoundary,
    CapabilityArtifact,
    CapabilityProgress,
    CapabilityResult,
    CapabilityDiagnostics,
)

CAPABILITY_ABI_VERSION = 2

__version__ = "0.1.0"

__all__ = [
    "CAPABILITY_ABI_VERSION",
    "WorkerContext",
    "run_worker",
    "run_persistent_worker",
    "CapabilityJob",
    "CapabilityGrants",
    "CapabilityBoundary",
    "CapabilityArtifact",
    "CapabilityProgress",
    "CapabilityResult",
    "CapabilityDiagnostics",
]