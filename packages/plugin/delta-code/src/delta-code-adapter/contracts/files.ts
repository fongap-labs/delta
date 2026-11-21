/** Delta-owned file-write semantics; a Host adapter maps them to provider signatures. */
export type DeltaFileWriteContract = 'legacy-overwrite' | 'native-unguarded' | 'guarded-intent'
