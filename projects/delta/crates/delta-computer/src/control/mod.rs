//! Desktop input control (ADR-0062, computer capability step three). Only the consent policy is
//! here so far: the operating-system input and UI Automation backend is not written yet, because it
//! can only be verified on Windows.

pub mod policy;

#[cfg(test)]
mod tests;
