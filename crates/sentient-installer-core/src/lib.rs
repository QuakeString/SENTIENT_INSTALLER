//! UI-agnostic engine for the SENTIENT installer. Phase 0: read-only preflight
//! checks. Later phases add the provisioning steps (WSL2, Docker, deploy).

pub mod checks;
