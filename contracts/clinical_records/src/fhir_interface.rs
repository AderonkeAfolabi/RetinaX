// Share FHIR data types and validators without linking a second contract's WASM exports.
#[path = "../../fhir/src/types.rs"]
pub mod types;
#[path = "../../fhir/src/validation.rs"]
pub mod validation;
