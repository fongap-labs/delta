import type { ArtifactInfo } from "./api";
import { summarizeChecks, trustFromMessages } from "./artifactTrust";

export type ArtifactReviewStatus = "passed" | "failed" | "unrecorded";
export interface ArtifactInventoryReview {
  byPath: Record<string, ArtifactReviewStatus>;
  passed: number;
  failed: number;
  unrecorded: number;
  priorVersions: number;
}

/** Historical producing-run checks only; never a current-file hash or delivery verdict. */
export function reviewArtifactInventory(
  artifacts: readonly ArtifactInfo[],
  messages: Parameters<typeof trustFromMessages>[0],
): ArtifactInventoryReview {
  const review: ArtifactInventoryReview = {
    byPath: {}, passed: 0, failed: 0, unrecorded: 0, priorVersions: 0,
  };
  const seen = new Set<string>();
  for (const artifact of artifacts) {
    if (seen.has(artifact.path)) continue;
    seen.add(artifact.path);
    const trust = trustFromMessages(messages, artifact.path);
    if (trust?.previousVersion) review.priorVersions++;
    const checks = trust ? summarizeChecks(trust) : null;
    const status: ArtifactReviewStatus = !checks || checks.total === 0
      ? "unrecorded" : trust?.isValidationOk ? "passed" : "failed";
    review.byPath[artifact.path] = status;
    review[status]++;
  }
  return review;
}
