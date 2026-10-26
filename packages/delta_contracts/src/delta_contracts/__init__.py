"""Language-neutral Delta extension contracts.

The package is deliberately small: it contains data shapes and version constants, not runtime,
policy or authority code. Foundation and Suite may both depend on it.
"""

from .manifest import (
    CAPABILITY_ABI_VERSION,
    WORKER_MANIFEST_VERSION,
    CapabilityGrants,
    WorkerEntry,
    WorkerManifest,
)

__version__ = "0.1.0"

__all__ = [
    "CAPABILITY_ABI_VERSION",
    "WORKER_MANIFEST_VERSION",
    "CapabilityGrants",
    "WorkerEntry",
    "WorkerManifest",
]
